use std::mem;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{
    CallArgument, CallChain, CallChainRoot, CallDescriptor, CallInvocation, CallKind, CallSegment,
    CallSymbol, CallTarget,
};

use super::origin::build_origin;

pub fn build<'tcx>(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<CallDescriptor> {
    let ts_node = node.inner_ts_node();
    if ts_node.kind() != "call" && ts_node.kind() != "call_expression" {
        return None;
    }

    let arguments = ts_node
        .child_by_field_name("arguments")
        .map(|args| parse_arguments(unit, args))
        .unwrap_or_default();

    let function_node = ts_node.child_by_field_name("function");
    let target = function_node
        .and_then(|func| parse_chain(unit, func))
        .or_else(|| parse_chain(unit, ts_node))
        .unwrap_or_else(|| {
            function_node
                .and_then(|func| parse_symbol_target(unit, func, None))
                .unwrap_or_else(|| CallTarget::Dynamic {
                    repr: unit.ts_text(ts_node),
                })
        });

    let origin = build_origin(unit, node, ts_node);
    let mut descriptor = CallDescriptor::new(origin, target);
    descriptor.arguments = arguments;

    Some(descriptor)
}

fn parse_chain<'tcx>(unit: CompileUnit<'tcx>, mut node: Node<'tcx>) -> Option<CallTarget> {
    let mut parts = Vec::new();
    let mut pending_arguments = Vec::new();
    let mut pending_invocation = false;

    loop {
        match node.kind() {
            "call" | "call_expression" => {
                pending_arguments = node
                    .child_by_field_name("arguments")
                    .map(|args| parse_arguments(unit, args))
                    .unwrap_or_default();
                pending_invocation = true;
                node = node.child_by_field_name("function")?;
            }
            "attribute" => {
                let method = node
                    .child_by_field_name("attribute")
                    .map(|n| unit.ts_text(n))
                    .unwrap_or_default();
                let arguments = mem::take(&mut pending_arguments);
                parts.push(CallSegment {
                    name: method,
                    kind: CallKind::Method,
                    type_arguments: Vec::new(),
                    arguments,
                });
                pending_invocation = false;
                node = node.child_by_field_name("object")?;
            }
            _ => break,
        }
    }

    if parts.is_empty() {
        return None;
    }

    parts.reverse();
    let root = if pending_invocation {
        let target =
            parse_symbol_target(unit, node, Some(CallKind::Function)).unwrap_or_else(|| {
                CallTarget::Dynamic {
                    repr: unit.ts_text(node),
                }
            });
        CallChainRoot::Invocation(CallInvocation::new(target, Vec::new(), pending_arguments))
    } else {
        CallChainRoot::Expr(unit.ts_text(node))
    };
    let mut chain = CallChain::new(root);
    chain.parts = parts;
    Some(CallTarget::Chain(chain))
}

fn parse_symbol_target<'tcx>(
    unit: CompileUnit<'tcx>,
    node: Node<'tcx>,
    kind_hint: Option<CallKind>,
) -> Option<CallTarget> {
    match node.kind() {
        "identifier" => {
            let name = unit.ts_text(node);
            let mut symbol = CallSymbol::new(&name);
            if let Some(kind) = kind_hint {
                symbol.kind = kind;
            }
            Some(CallTarget::Symbol(symbol))
        }
        "attribute" => {
            let mut parts = flatten_attribute(unit, node)?;
            let name = parts.pop().unwrap_or_default();
            let mut symbol = CallSymbol::new(&name);
            symbol.qualifiers = parts;
            if let Some(kind) = kind_hint {
                symbol.kind = kind;
            }
            Some(CallTarget::Symbol(symbol))
        }
        _ => None,
    }
}

fn flatten_attribute<'tcx>(unit: CompileUnit<'tcx>, mut node: Node<'tcx>) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    loop {
        if node.kind() != "attribute" {
            break;
        }

        let attr_node = node.child_by_field_name("attribute")?;
        parts.push(unit.ts_text(attr_node));
        node = node.child_by_field_name("object")?;
    }

    let root_text = unit.ts_text(node);
    if root_text.is_empty() {
        return None;
    }
    parts.push(root_text);
    parts.reverse();
    Some(parts)
}

fn parse_arguments<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> Vec<CallArgument> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .map(|child| parse_argument_node(unit, child))
        .collect()
}

fn parse_argument_node<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> CallArgument {
    match node.kind() {
        "keyword_argument" => {
            let name = node
                .child_by_field_name("name")
                .map(|name_node| unit.ts_text(name_node));
            let value = node
                .child_by_field_name("value")
                .map(|value_node| unit.ts_text(value_node))
                .unwrap_or_else(|| unit.ts_text(node));
            let mut argument = CallArgument::new(value);
            argument.name = name;
            argument
        }
        _ => CallArgument::new(unit.ts_text(node)),
    }
}
