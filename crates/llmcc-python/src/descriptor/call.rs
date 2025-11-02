use std::mem;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_descriptor::{
    CallArgument, CallChain, CallDescriptor, CallKind, CallSegment, CallSymbol, CallTarget,
};
use tree_sitter::Node;

use super::origin::build_origin;

/// Build a shared call descriptor for a Python call expression.
pub fn build_call_descriptor<'tcx, F>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    enclosing: Option<String>,
    classify_symbol: F,
) -> CallDescriptor
where
    F: Fn(&str) -> CallKind,
{
    let ts_node = node.inner_ts_node();
    let origin = build_origin(unit, node, ts_node);
    let arguments = ts_node
        .child_by_field_name("arguments")
        .map(|args| parse_arguments(unit, args))
        .unwrap_or_default();

    let function_node = ts_node.child_by_field_name("function");
    let target = function_node
        .and_then(|func| parse_chain(unit, func))
        .or_else(|| parse_chain(unit, ts_node))
        .or_else(|| {
            function_node.and_then(|func| parse_symbol_target(unit, func, &classify_symbol))
        })
        .unwrap_or_else(|| CallTarget::Dynamic {
            repr: clean(&node_text(unit, ts_node)),
        });

    let mut descriptor = CallDescriptor::new(origin, target);
    descriptor.enclosing = enclosing;
    descriptor.arguments = arguments;
    descriptor
}

fn parse_chain<'tcx>(unit: CompileUnit<'tcx>, mut node: Node<'tcx>) -> Option<CallTarget> {
    let mut segments = Vec::new();
    let mut pending_arguments = Vec::new();

    loop {
        match node.kind() {
            "call" => {
                pending_arguments = node
                    .child_by_field_name("arguments")
                    .map(|args| parse_arguments(unit, args))
                    .unwrap_or_default();
                node = node.child_by_field_name("function")?;
            }
            "attribute" => {
                let name_node = node.child_by_field_name("attribute")?;
                let method = clean(&node_text(unit, name_node));
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

fn parse_symbol_target<'tcx, F>(
    unit: CompileUnit<'tcx>,
    node: Node<'tcx>,
    classify_symbol: &F,
) -> Option<CallTarget>
where
    F: Fn(&str) -> CallKind,
{
    match node.kind() {
        "identifier" => {
            let name = clean(&node_text(unit, node));
            let mut symbol = CallSymbol::new(&name);
            symbol.kind = classify_symbol(&symbol.name);
            Some(CallTarget::Symbol(symbol))
        }
        "attribute" => {
            let mut segments = flatten_attribute(unit, node)?;
            let name = segments.pop().unwrap_or_default();
            let mut symbol = CallSymbol::new(&name);
            symbol.qualifiers = segments;
            symbol.kind = classify_symbol(&symbol.name);
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
