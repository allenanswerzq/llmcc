use std::mem;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{
    CallArgument, CallChain, CallChainRoot, CallDescriptor, CallInvocation, CallKind, CallSegment,
    CallSymbol, CallTarget, TypeExpr,
};

use crate::path::parse_rust_path;

use super::function::{build_origin, parse_type_expr};

/// Build a shared call descriptor from a Rust call expression.
pub fn build<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    enclosing_function: Option<&str>,
) -> CallDescriptor {
    let ts_node = node.inner_ts_node();
    match ts_node.kind() {
        "macro_invocation" => build_macro_invocation(unit, node, ts_node, enclosing_function),
        _ => build_call_expression(unit, node, ts_node, enclosing_function),
    }
}

fn build_call_expression<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    ts_node: Node<'tcx>,
    enclosing_function: Option<&str>,
) -> CallDescriptor {
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
                repr: unit.ts_text(ts_node),
            },
        });

    let arguments = find_arguments_node(ts_node)
        .map(|args| parse_arguments(unit, args))
        .unwrap_or_default();

    let origin = build_origin(unit, node, ts_node);
    let mut descriptor = CallDescriptor::new(origin, target);
    descriptor.enclosing = enclosing_function.map(|value| value.to_string());
    descriptor.arguments = arguments;

    descriptor
}

fn build_macro_invocation<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    ts_node: Node<'tcx>,
    enclosing_function: Option<&str>,
) -> CallDescriptor {
    let macro_node = ts_node.child_by_field_name("macro").unwrap_or(ts_node);
    let macro_text = unit.ts_text(macro_node);
    let target = match macro_node.kind() {
        "identifier" | "scoped_identifier" => {
            symbol_target_from_path(&macro_text, Vec::new(), CallKind::Macro)
        }
        _ => CallTarget::Dynamic {
            repr: unit.ts_text(ts_node),
        },
    };

    let arguments = ts_node
        .child_by_field_name("token_tree")
        .and_then(|tree| {
            let text = unit.ts_text(tree);
            if text.trim().is_empty() {
                None
            } else {
                Some(vec![CallArgument::new(text)])
            }
        })
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
        .map(|arg| CallArgument::new(unit.ts_text(arg)))
        .collect()
}

fn find_arguments_node(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(args) = node.child_by_field_name("arguments") {
        return Some(args);
    }

    let mut cursor = node.walk();
    let result = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "arguments");
    result
}

fn parse_call_target<'tcx>(
    unit: CompileUnit<'tcx>,
    node: Node<'tcx>,
    call_generics: Vec<TypeExpr>,
) -> CallTarget {
    match node.kind() {
        "identifier" | "scoped_identifier" | "type_identifier" => {
            symbol_target_from_path(&unit.ts_text(node), call_generics, CallKind::Function)
        }
        "generic_type" => {
            let base = node.child_by_field_name("type").unwrap_or(node);
            let base_text = unit.ts_text(base);
            let generics = node
                .child_by_field_name("type_arguments")
                .map(|args| parse_type_arguments(unit, args))
                .unwrap_or(call_generics);
            symbol_target_from_path(&base_text, generics, CallKind::Function)
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
                .map(|n| unit.ts_text(n))
                .unwrap_or_else(|| unit.ts_text(node));
            let method = node
                .child_by_field_name("field")
                .map(|n| unit.ts_text(n))
                .unwrap_or_default();
            let generics = node
                .child_by_field_name("type_arguments")
                .map(|n| parse_type_arguments(unit, n))
                .unwrap_or(call_generics);

            let mut chain = CallChain::new(receiver);
            chain.parts.push(CallSegment {
                name: method,
                kind: CallKind::Method,
                type_arguments: generics,
                arguments: Vec::new(),
            });
            CallTarget::Chain(chain)
        }
        _ => CallTarget::Dynamic {
            repr: unit.ts_text(node),
        },
    }
}

fn parse_chain<'tcx>(
    unit: CompileUnit<'tcx>,
    mut node: Node<'tcx>,
    call_generics: Vec<TypeExpr>,
) -> Option<CallTarget> {
    let mut parts = Vec::new();
    let mut pending_generics = call_generics;
    let mut pending_arguments = Vec::new();
    let mut pending_invocation = false;

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
                pending_invocation = true;
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
                    .map(|n| unit.ts_text(n))
                    .unwrap_or_default();
                let generics = mem::take(&mut pending_generics);
                let arguments = mem::take(&mut pending_arguments);
                parts.push(CallSegment {
                    name: method,
                    kind: CallKind::Method,
                    type_arguments: generics,
                    arguments,
                });
                pending_invocation = false;
                node = node.child_by_field_name("value")?;
            }
            _ => break,
        }
    }

    if parts.is_empty() {
        return None;
    }

    parts.reverse();
    let root = if pending_invocation {
        let target = parse_call_target(unit, node, pending_generics.clone());
        CallChainRoot::Invocation(CallInvocation::new(
            target,
            pending_generics,
            pending_arguments,
        ))
    } else {
        CallChainRoot::Expr(unit.ts_text(node))
    };
    let mut chain = CallChain::new(root);
    chain.parts = parts;
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

fn symbol_target_from_path(raw: &str, generics: Vec<TypeExpr>, kind: CallKind) -> CallTarget {
    let qualifier = parse_rust_path(raw);
    let mut parts: Vec<String> = qualifier
        .parts()
        .iter()
        .map(strip_generics)
        .filter(|segment| !segment.is_empty())
        .collect();
    if parts.is_empty() {
        parts = raw
            .split("::")
            .filter(|s| !s.is_empty())
            .map(strip_generics)
            .filter(|segment| !segment.is_empty())
            .collect();
    }

    if parts.is_empty() {
        return CallTarget::Dynamic {
            repr: raw.to_string(),
        };
    }

    let name = parts.pop().unwrap();

    let mut symbol = CallSymbol::new(name);
    let mut qualifiers = qualifier.prefix_segments();
    qualifiers.extend(parts);
    symbol.qualifiers = qualifiers;
    symbol.kind = kind;
    symbol.type_arguments = generics;
    CallTarget::Symbol(symbol)
}

fn strip_generics(segment: impl AsRef<str>) -> String {
    let s = segment.as_ref();
    match s.find('<') {
        Some(idx) => s[..idx].to_string(),
        None => s.to_string(),
    }
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
