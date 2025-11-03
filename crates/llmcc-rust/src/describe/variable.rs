use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{VariableDescriptor, VariableKind, VariableScope, Visibility};

use super::function::{build_origin, parse_type_expr, parse_visibility};
use crate::token::LangRust;

fn ident_name_from_field<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    field_id: u16,
) -> Option<String> {
    node.opt_child_by_field(unit, field_id)
        .and_then(|child| child.find_ident(unit))
        .map(|ident| ident.name.clone())
}

/// Build a shared descriptor for a local `let` binding.
pub fn build_let<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<VariableDescriptor> {
    let ts_node = node.inner_ts_node();
    let ty = ts_node
        .child_by_field_name("type")
        .map(|n| parse_type_expr(unit, n));
    let is_mut = has_mutable_specifier(ts_node);
    let origin = build_origin(unit, node, ts_node);

    let name = ident_name_from_field(unit, node, LangRust::field_pattern)?;

    let mut descriptor = VariableDescriptor::new(origin, name);
    descriptor.kind = VariableKind::Binding;
    descriptor.scope = VariableScope::Function;
    descriptor.is_mutable = Some(is_mut);
    descriptor.type_annotation = ty;
    descriptor.visibility = Visibility::Private;

    Some(descriptor)
}

/// Build a shared descriptor for a `const` item.
pub fn build_const_item<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<VariableDescriptor> {
    let ts_node = node.inner_ts_node();
    let ty = ts_node
        .child_by_field_name("type")
        .map(|n| parse_type_expr(unit, n));
    let origin = build_origin(unit, node, ts_node);

    let name = ident_name_from_field(unit, node, LangRust::field_name)?;
    let name_node = ts_node.child_by_field_name("name")?;
    let header_text = unit
        .file()
        .get_text(ts_node.start_byte(), name_node.start_byte());
    let visibility = parse_visibility(&header_text);

    let mut descriptor = VariableDescriptor::new(origin, name);
    descriptor.kind = VariableKind::Constant;
    descriptor.scope = VariableScope::Global;
    descriptor.is_mutable = Some(false);
    descriptor.type_annotation = ty;
    descriptor.visibility = visibility;

    Some(descriptor)
}

/// Build a shared descriptor for a `static` item.
pub fn build_static_item<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<VariableDescriptor> {
    let ts_node = node.inner_ts_node();
    let ty = ts_node
        .child_by_field_name("type")
        .map(|n| parse_type_expr(unit, n));
    let is_mut = has_mutable_specifier(ts_node);
    let origin = build_origin(unit, node, ts_node);

    let name = ident_name_from_field(unit, node, LangRust::field_name)?;
    let name_node = ts_node.child_by_field_name("name")?;
    let header_text = unit
        .file()
        .get_text(ts_node.start_byte(), name_node.start_byte());
    let visibility = parse_visibility(&header_text);

    let mut descriptor = VariableDescriptor::new(origin, name);
    descriptor.kind = VariableKind::Static;
    descriptor.scope = VariableScope::Global;
    descriptor.is_mutable = Some(is_mut);
    descriptor.type_annotation = ty;
    descriptor.visibility = visibility;

    Some(descriptor)
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
