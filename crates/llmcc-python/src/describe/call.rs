use std::mem;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{
    CallArgument, CallChain, CallDescriptor, CallKind, CallSegment, CallSymbol, CallTarget,
    DescriptorMeta,
};

use super::origin::build_origin;

pub fn build<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    meta: DescriptorMeta<'_>,
) -> Option<CallDescriptor> {
    let DescriptorMeta::Call {
        enclosing,
        kind_hint,
        ..
    } = meta
    else {
        return None;
    };

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
            let hint = kind_hint;
            function_node
                .and_then(|func| parse_symbol_target(unit, func, hint))
                .unwrap_or_else(|| CallTarget::Dynamic {
                    repr: clean(&node_text(unit, ts_node)),
                })
        });

    let origin = build_origin(unit, node, ts_node);
    let mut descriptor = CallDescriptor::new(origin, target);
    descriptor.arguments = arguments;
    descriptor.enclosing = enclosing.map(|value| value.to_string());

    Some(descriptor)
}

fn parse_chain<'tcx>(unit: CompileUnit<'tcx>, mut node: Node<'tcx>) -> Option<CallTarget> {
    let mut segments = Vec::new();
    let mut pending_arguments = Vec::new();

    loop {
        match node.kind() {
            "call" | "call_expression" => {
                pending_arguments = node
                    .child_by_field_name("arguments")
                    .map(|args| parse_arguments(unit, args))
                    .unwrap_or_default();
                node = node.child_by_field_name("function")?;
            }
            "attribute" => {
                let method = node
                    .child_by_field_name("attribute")
                    .map(|n| clean(&node_text(unit, n)))
                    .unwrap_or_default();
                let arguments = mem::take(&mut pending_arguments);
                segments.push(CallSegment {
                    name: method,
                    kind: CallKind::Method,
                    type_arguments: Vec::new(),
                    arguments,
                });
                node = node.child_by_field_name("object")?;
            }
            _ => break,
        }
    }

    if segments.is_empty() {
        return None;
    }

    segments.reverse();
    let root = clean(&node_text(unit, node));
    let mut chain = CallChain::new(root);
    chain.segments = segments;
    Some(CallTarget::Chain(chain))
}

fn parse_symbol_target<'tcx>(
    unit: CompileUnit<'tcx>,
    node: Node<'tcx>,
    kind_hint: Option<CallKind>,
) -> Option<CallTarget> {
    match node.kind() {
        "identifier" => {
            let name = clean(&node_text(unit, node));
            let mut symbol = CallSymbol::new(&name);
            if let Some(kind) = kind_hint {
                symbol.kind = kind;
            }
            Some(CallTarget::Symbol(symbol))
        }
        "attribute" => {
            let mut segments = flatten_attribute(unit, node)?;
            let name = segments.pop().unwrap_or_default();
            let mut symbol = CallSymbol::new(&name);
            symbol.qualifiers = segments;
            if let Some(kind) = kind_hint {
                symbol.kind = kind;
            }
            Some(CallTarget::Symbol(symbol))
        }
        _ => None,
    }
}

fn flatten_attribute<'tcx>(unit: CompileUnit<'tcx>, mut node: Node<'tcx>) -> Option<Vec<String>> {
    let mut segments = Vec::new();
    loop {
        if node.kind() != "attribute" {
            break;
        }

        let attr_node = node.child_by_field_name("attribute")?;
        segments.push(clean(&node_text(unit, attr_node)));
        node = node.child_by_field_name("object")?;
    }

    let root_text = clean(&node_text(unit, node));
    if root_text.is_empty() {
        return None;
    }
    segments.push(root_text);
    segments.reverse();
    Some(segments)
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
                .map(|name_node| clean(&node_text(unit, name_node)));
            let value = node
                .child_by_field_name("value")
                .map(|value_node| clean(&node_text(unit, value_node)))
                .unwrap_or_else(|| clean(&node_text(unit, node)));
            let mut argument = CallArgument::new(value);
            argument.name = name;
            argument
        }
        _ => CallArgument::new(clean(&node_text(unit, node))),
    }
}

fn node_text<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> String {
    unit.get_text(node.start_byte(), node.end_byte())
}

fn clean(text: &str) -> String {
    text.trim().to_string()
}
