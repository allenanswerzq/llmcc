use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{VariableDescriptor, VariableKind, VariableScope};

use super::function::{build_origin, parse_type_expr};

/// Build a shared descriptor for a local `let` binding.
pub fn build_let<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    name: String,
    fqn: String,
) -> VariableDescriptor {
    let ts_node = node.inner_ts_node();
    let ty = ts_node
        .child_by_field_name("type")
        .map(|n| parse_type_expr(unit, n));
    let is_mut = has_mutable_specifier(ts_node);
    let origin = build_origin(unit, node, ts_node);

    let mut descriptor = VariableDescriptor::new(origin, name);
    descriptor.fqn = Some(fqn);
    descriptor.kind = VariableKind::Binding;
    descriptor.scope = VariableScope::Function;
    descriptor.is_mutable = Some(is_mut);
    descriptor.type_annotation = ty;

    descriptor
}

/// Build a shared descriptor for a `const` item.
pub fn build_const_item<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    name: String,
    fqn: String,
) -> VariableDescriptor {
    let ts_node = node.inner_ts_node();
    let ty = ts_node
        .child_by_field_name("type")
        .map(|n| parse_type_expr(unit, n));
    let origin = build_origin(unit, node, ts_node);

    let mut descriptor = VariableDescriptor::new(origin, name);
    descriptor.fqn = Some(fqn);
    descriptor.kind = VariableKind::Constant;
    descriptor.scope = VariableScope::Global;
    descriptor.is_mutable = Some(false);
    descriptor.type_annotation = ty;

    descriptor
}

/// Build a shared descriptor for a `static` item.
pub fn build_static_item<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    name: String,
    fqn: String,
) -> VariableDescriptor {
    let ts_node = node.inner_ts_node();
    let ty = ts_node
        .child_by_field_name("type")
        .map(|n| parse_type_expr(unit, n));
    let is_mut = has_mutable_specifier(ts_node);
    let origin = build_origin(unit, node, ts_node);

    let mut descriptor = VariableDescriptor::new(origin, name);
    descriptor.fqn = Some(fqn);
    descriptor.kind = VariableKind::Static;
    descriptor.scope = VariableScope::Global;
    descriptor.is_mutable = Some(is_mut);
    descriptor.type_annotation = ty;

    descriptor
}

fn has_mutable_specifier(ts_node: Node<'_>) -> bool {
    if ts_node.child_by_field_name("mutable_specifier").is_some() {
        return true;
    }
    let mut cursor = ts_node.walk();
    if cursor.goto_first_child() {
        loop {
            if cursor.node().kind() == "mutable_specifier" {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}
