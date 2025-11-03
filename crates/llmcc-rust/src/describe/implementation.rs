use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{ClassDescriptor, TypeExpr};

use super::function::{build_origin, parse_type_expr};

/// Build a descriptor for a Rust `impl` block.
pub fn build<'tcx>(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ClassDescriptor> {
    let ts_node = match node.inner_ts_node() {
        ts if ts.kind() == "impl_item" => ts,
        _ => return None,
    };

    let type_node = ts_node.child_by_field_name("type")?;
    let type_repr = clean(&node_text(unit, type_node));
    let self_type_expr = parse_type_expr(unit, type_node);
    let (self_name, raw_self_fqn) = impl_target_info(&self_type_expr, &type_repr);
    let canonical_self_fqn = canonical_impl_target_fqn(unit, &raw_self_fqn);

    let origin = build_origin(unit, node, ts_node);

    let mut descriptor = ClassDescriptor::new(origin, self_name);
    descriptor
        .extras
        .insert("self_type_fqn".to_string(), canonical_self_fqn.clone());
    if canonical_self_fqn != raw_self_fqn {
        descriptor
            .extras
            .insert("self_type_fqn_raw".to_string(), raw_self_fqn.clone());
    }
    descriptor
        .extras
        .insert("self_type_repr".to_string(), type_repr.clone());

    if let Some(trait_node) = ts_node.child_by_field_name("trait") {
        let trait_expr = parse_type_expr(unit, trait_node);
        descriptor.base_types.push(trait_expr);
        descriptor.extras.insert(
            "trait_repr".to_string(),
            clean(&node_text(unit, trait_node)),
        );
    }

    if let Some(generics_node) = ts_node.child_by_field_name("type_parameters") {
        descriptor.extras.insert(
            "generics".to_string(),
            clean(&node_text(unit, generics_node)),
        );
    }

    if let Some(where_node) = ts_node.child_by_field_name("where_clause") {
        descriptor.extras.insert(
            "where_clause".to_string(),
            clean(&node_text(unit, where_node)),
        );
    }

    if let Some(header) = impl_header(unit, ts_node) {
        descriptor.extras.insert("header".to_string(), header);
    }

    descriptor.impl_target_fqn = Some(canonical_self_fqn);

    Some(descriptor)
}

fn canonical_impl_target_fqn<'tcx>(unit: CompileUnit<'tcx>, value: &str) -> String {
    if value.is_empty() {
        return format!("unit{}", unit.index);
    }

    let trimmed = value.trim_start_matches("::");
    trimmed.to_string()
}

fn impl_target_info(ty: &TypeExpr, fallback: &str) -> (String, String) {
    if let Some(segments) = type_expr_segments(ty) {
        if !segments.is_empty() {
            let name = segments
                .last()
                .cloned()
                .unwrap_or_else(|| fallback_name(fallback));
            let fqn = segments.join("::");
            return (name, fqn);
        }
    }

    let name = fallback_name(fallback);
    (name.clone(), fallback.to_string())
}

fn type_expr_segments(expr: &TypeExpr) -> Option<Vec<String>> {
    match expr {
        TypeExpr::Path { segments, .. } => Some(segments.clone()),
        TypeExpr::Reference { inner, .. } => type_expr_segments(inner),
        TypeExpr::Tuple(items) if items.len() == 1 => type_expr_segments(&items[0]),
        _ => None,
    }
}

fn fallback_name(fallback: &str) -> String {
    fallback
        .split("::")
        .last()
        .filter(|s| !s.is_empty())
        .unwrap_or("impl")
        .to_string()
}

fn impl_header<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> Option<String> {
    let type_node = node.child_by_field_name("type")?;
    if type_node.start_byte() <= node.start_byte() {
        return None;
    }
    let text = unit
        .file()
        .get_text(node.start_byte(), type_node.start_byte())
        .trim()
        .to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
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
