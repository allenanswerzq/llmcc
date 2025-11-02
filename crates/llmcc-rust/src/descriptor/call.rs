use std::mem;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{
    CallArgument, CallChain, CallDescriptor, CallKind, CallSegment, CallSymbol, CallTarget,
    TypeExpr,
};

use super::function::{build_origin, parse_type_expr};

/// Build a shared call descriptor from a Rust call expression.
pub fn from_call<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    enclosing_function: Option<&str>,
) -> CallDescriptor {
    let ts_node = node.inner_ts_node();
    let function_node = ts_node.child_by_field_name("function");
    let call_generics = ts_node
        .child_by_field_name("type_arguments")
        .map(|n| parse_type_arguments(unit, n))
        .unwrap_or_default();

    let target = function_node
        .and_then(|func| parse_chain(unit, func, call_generics.clone()))
        .or_else(|| parse_chain(unit, ts_node, call_generics.clone()))
        .unwrap_or_else(|| match function_node {
            Some(func) => parse_call_target(unit, func, call_generics.clone()),
            None => CallTarget::Dynamic {
                repr: clean(&node_text(unit, ts_node)),
            },
        });

    let arguments = ts_node
        .child_by_field_name("arguments")
        .map(|args| parse_arguments(unit, args))
        .unwrap_or_default();

    let origin = build_origin(unit, node, ts_node);
    let mut descriptor = CallDescriptor::new(origin, target);
    descriptor.enclosing = enclosing_function.map(|value| value.to_string());
    descriptor.arguments = arguments;

    descriptor
}

fn parse_arguments<'tcx>(unit: CompileUnit<'tcx>, args_node: Node<'tcx>) -> Vec<CallArgument> {
    let mut cursor = args_node.walk();
    args_node
        .named_children(&mut cursor)
        .map(|arg| CallArgument::new(clean(&node_text(unit, arg))))
        .collect()
}

fn parse_call_target<'tcx>(
    unit: CompileUnit<'tcx>,
    node: Node<'tcx>,
    call_generics: Vec<TypeExpr>,
) -> CallTarget {
    match node.kind() {
        "identifier" | "scoped_identifier" | "type_identifier" => {
            let segments: Vec<String> = clean(&node_text(unit, node))
                .split("::")
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            symbol_target_from_segments(segments, call_generics, CallKind::Function)
        }
        "generic_type" => {
            let base = node.child_by_field_name("type").unwrap_or(node);
            let mut segments: Vec<String> = clean(&node_text(unit, base))
                .split("::")
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            if segments.is_empty() {
                segments.push(clean(&node_text(unit, base)));
            }
            let generics = node
                .child_by_field_name("type_arguments")
                .map(|args| parse_type_arguments(unit, args))
                .unwrap_or(call_generics);
            symbol_target_from_segments(segments, generics, CallKind::Function)
        }
        "generic_function" => {
            let generics = node
                .child_by_field_name("type_arguments")
                .map(|args| parse_type_arguments(unit, args))
                .unwrap_or_default();
            let inner = node
                .child_by_field_name("function")
                .unwrap_or(node.child(0).unwrap_or(node));
            let mut target = parse_call_target(unit, inner, call_generics);
            if let CallTarget::Symbol(symbol) = &mut target {
                symbol.type_arguments = generics;
            }
            target
        }
        "field_expression" => {
            let receiver = node
                .child_by_field_name("value")
                .map(|n| clean(&node_text(unit, n)))
                .unwrap_or_else(|| clean(&node_text(unit, node)));
            let method = node
                .child_by_field_name("field")
                .map(|n| clean(&node_text(unit, n)))
                .unwrap_or_default();
            let generics = node
                .child_by_field_name("type_arguments")
                .map(|n| parse_type_arguments(unit, n))
                .unwrap_or(call_generics);

            let mut chain = CallChain::new(receiver);
            chain.segments.push(CallSegment {
                name: method,
                kind: CallKind::Method,
                type_arguments: generics,
                arguments: Vec::new(),
            });
            CallTarget::Chain(chain)
        }
        _ => CallTarget::Dynamic {
            repr: clean(&node_text(unit, node)),
        },
    }
}

fn parse_chain<'tcx>(
    unit: CompileUnit<'tcx>,
    mut node: Node<'tcx>,
    call_generics: Vec<TypeExpr>,
) -> Option<CallTarget> {
    let mut segments = Vec::new();
    let mut pending_generics = call_generics;
    let mut pending_arguments = Vec::new();

    loop {
        match node.kind() {
            "call_expression" => {
                pending_generics = node
                    .child_by_field_name("type_arguments")
                    .map(|n| parse_type_arguments(unit, n))
                    .unwrap_or_default();
                pending_arguments = node
                    .child_by_field_name("arguments")
                    .map(|args| parse_arguments(unit, args))
                    .unwrap_or_default();
                node = node.child_by_field_name("function")?;
            }
            "generic_function" => {
                pending_generics = node
                    .child_by_field_name("type_arguments")
                    .map(|n| parse_type_arguments(unit, n))
                    .unwrap_or_default();
                node = node.child_by_field_name("function")?;
            }
            "field_expression" => {
                let method = node
                    .child_by_field_name("field")
                    .map(|n| clean(&node_text(unit, n)))
                    .unwrap_or_default();
                let generics = mem::take(&mut pending_generics);
                let arguments = mem::take(&mut pending_arguments);
                segments.push(CallSegment {
                    name: method,
                    kind: CallKind::Method,
                    type_arguments: generics,
                    arguments,
                });
                node = node.child_by_field_name("value")?;
            }
            _ => break,
        }
    }

    if segments.is_empty() {
        return None;
    }

    segments.reverse();
    let base = clean(&node_text(unit, node));
    let mut chain = CallChain::new(base);
    chain.segments = segments;
    Some(CallTarget::Chain(chain))
}

fn parse_type_arguments<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> Vec<TypeExpr> {
    let mut args = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "type_argument" => {
                if let Some(inner) = child.child_by_field_name("type") {
                    args.push(parse_type_expr(unit, inner));
                }
            }
            kind if is_type_node(kind) => {
                args.push(parse_type_expr(unit, child));
            }
            _ => {}
        }
    }
    if args.is_empty() {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if is_type_node(child.kind()) {
                args.push(parse_type_expr(unit, child));
            }
        }
    }
    args
}

fn symbol_target_from_segments(
    segments: Vec<String>,
    generics: Vec<TypeExpr>,
    kind: CallKind,
) -> CallTarget {
    if segments.is_empty() {
        return CallTarget::Dynamic {
            repr: String::new(),
        };
    }

    let mut segments = segments;
    let name = segments.pop().unwrap();

    let mut symbol = CallSymbol::new(name);
    symbol.qualifiers = segments;
    symbol.kind = kind;
    symbol.type_arguments = generics;
    CallTarget::Symbol(symbol)
}

fn is_type_node(kind: &str) -> bool {
    matches!(
        kind,
        "type_identifier"
            | "scoped_type_identifier"
            | "generic_type"
            | "reference_type"
            | "tuple_type"
            | "primitive_type"
            | "impl_trait_type"
    )
}

fn node_text<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> String {
    unit.file().get_text(node.start_byte(), node.end_byte())
}

fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut last_was_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_was_ws && !out.is_empty() {
                out.push(' ');
            }
            last_was_ws = true;
        } else {
            out.push(ch);
            last_was_ws = false;
        }
    }
    out.trim().to_string()
}
