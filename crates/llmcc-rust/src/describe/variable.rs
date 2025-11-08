use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{VariableDescriptor, VariableKind, VariableScope, Visibility};

use super::function::{build_origin, parse_type_expr, parse_visibility};
use crate::token::LangRust;

fn push_binding_name<'tcx>(
    unit: CompileUnit<'tcx>,
    node: Node<'tcx>,
    out: &mut Vec<(usize, String)>,
) {
    let text = unit.ts_text(node);
    if text == "_" || text.is_empty() {
        return;
    }
    if let Some(first) = text.chars().next() {
        if first.is_uppercase() {
            return;
        }
    }
    out.push((node.start_byte(), text));
}

fn collect_pattern_bindings<'tcx>(
    unit: CompileUnit<'tcx>,
    node: Node<'tcx>,
    allow_binding: bool,
    out: &mut Vec<(usize, String)>,
) {
    match node.kind() {
        "identifier" if allow_binding => {
            push_binding_name(unit, node, out);
            return;
        }
        "identifier" => return,
        "shorthand_field_identifier" => {
            push_binding_name(unit, node, out);
            return;
        }
        "field_pattern" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "field_identifier" {
                    continue;
                }
                collect_pattern_bindings(unit, child, true, out);
            }
            return;
        }
        "struct_pattern" | "tuple_struct_pattern" => {
            let mut cursor = node.walk();
            for (idx, child) in node.named_children(&mut cursor).enumerate() {
                if idx == 0 {
                    // Skip the constructor/type path.
                    continue;
                }
                collect_pattern_bindings(unit, child, true, out);
            }
            return;
        }
        "scoped_identifier" | "type_identifier" | "primitive_type" if !allow_binding => return,
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_pattern_bindings(unit, child, allow_binding, out);
    }
}

fn binding_names_from_pattern<'tcx>(unit: CompileUnit<'tcx>, pattern: Node<'tcx>) -> Vec<String> {
    let mut names = Vec::new();
    collect_pattern_bindings(unit, pattern, true, &mut names);
    names.sort_by_key(|entry| entry.0);
    names.into_iter().map(|(_, name)| name).collect()
}

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

    let pattern_node = node.opt_child_by_field(unit, LangRust::field_pattern)?;
    let mut names = binding_names_from_pattern(unit, pattern_node.inner_ts_node());
    if names.is_empty() {
        return None;
    }
    let name = names.remove(0);

    let mut descriptor = VariableDescriptor::new(origin, name);
    descriptor.kind = VariableKind::Binding;
    descriptor.scope = VariableScope::Function;
    descriptor.is_mutable = Some(is_mut);
    descriptor.type_annotation = ty;
    descriptor.visibility = Visibility::Private;
    if !names.is_empty() {
        descriptor.extra_binding_names = Some(names);
    }

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
