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
    let type_repr = unit.ts_text(type_node);
    let self_type_expr = parse_type_expr(unit, type_node);
    let (self_name, self_fqn) = impl_target_info(&self_type_expr, &type_repr);

    let origin = build_origin(unit, node, ts_node);

    let mut descriptor = ClassDescriptor::new(origin, self_name);
    descriptor.impl_target_fqn = Some(self_fqn.clone());
    descriptor
        .extras
        .insert("self_type_repr".to_string(), type_repr.clone());

    if let Some(trait_node) = ts_node.child_by_field_name("trait") {
        let trait_expr = parse_type_expr(unit, trait_node);
        descriptor.base_types.push(trait_expr);
        descriptor
            .extras
            .insert("trait_repr".to_string(), unit.ts_text(trait_node));
    }

    if let Some(generics_node) = ts_node.child_by_field_name("type_parameters") {
        descriptor
            .extras
            .insert("generics".to_string(), unit.ts_text(generics_node));
    }

    if let Some(where_node) = ts_node.child_by_field_name("where_clause") {
        descriptor
            .extras
            .insert("where_clause".to_string(), unit.ts_text(where_node));
    }

    if let Some(header) = impl_header(unit, ts_node) {
        descriptor.extras.insert("header".to_string(), header);
    }

    Some(descriptor)
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
