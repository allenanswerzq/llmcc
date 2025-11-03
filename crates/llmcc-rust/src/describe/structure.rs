use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{StructDescriptor, StructField, StructKind};

use super::function::{build_origin, parse_type_expr, parse_visibility};

/// Build a shared struct descriptor from the Rust AST node.
pub fn build<'tcx>(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<StructDescriptor> {
    let ts_node = node.inner_ts_node();
    let kind = ts_node.kind();
    if kind != "struct_item" && kind != "trait_item" {
        return None;
    }
    let is_trait = kind == "trait_item";

    let name_node = ts_node.child_by_field_name("name")?;
    let name = clean(&node_text(unit, name_node));
    let header_text = unit
        .file()
        .get_text(ts_node.start_byte(), name_node.start_byte());
    let visibility = parse_visibility(&header_text);

    let generics = ts_node
        .child_by_field_name("type_parameters")
        .map(|n| clean(&node_text(unit, n)));

    let (fields, struct_kind) = if is_trait {
        (Vec::new(), StructKind::Other)
    } else {
        let (fields, shape) = parse_struct_fields(unit, ts_node);
        let kind = match shape {
            StructShape::Named => StructKind::Record,
            StructShape::Tuple => StructKind::Tuple,
            StructShape::Unit => StructKind::Unit,
        };
        (fields, kind)
    };

    let origin = build_origin(unit, node, ts_node);

    let mut descriptor = StructDescriptor::new(origin, name);
    descriptor.visibility = visibility;
    descriptor.generics = generics;
    descriptor.kind = struct_kind;
    descriptor.fields = fields;

    Some(descriptor)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructShape {
    Named,
    Tuple,
    Unit,
}

fn parse_struct_fields<'tcx>(
    unit: CompileUnit<'tcx>,
    node: Node<'tcx>,
) -> (Vec<StructField>, StructShape) {
    match node.kind() {
        "field_declaration_list" => return (parse_named_fields(unit, node), StructShape::Named),
        "tuple_field_declaration_list" | "ordered_field_declaration_list" => {
            return (parse_tuple_fields(unit, node), StructShape::Tuple)
        }
        _ => {}
    }

    let mut named = Vec::new();
    let mut tuple = Vec::new();
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            match child.kind() {
                "field_declaration_list" => {
                    return (parse_named_fields(unit, child), StructShape::Named)
                }
                "tuple_field_declaration_list" | "ordered_field_declaration_list" => {
                    return (parse_tuple_fields(unit, child), StructShape::Tuple)
                }
                "field_declaration" => named.push(parse_named_field_node(unit, child)),
                "tuple_field_declaration" | "ordered_field_declaration" => {
                    tuple.push(parse_tuple_field_node(unit, child))
                }
                _ => {
                    let (fields, kind) = parse_struct_fields(unit, child);
                    match kind {
                        StructShape::Named if !fields.is_empty() => {
                            return (fields, StructShape::Named)
                        }
                        StructShape::Tuple if !fields.is_empty() => {
                            return (fields, StructShape::Tuple)
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if !named.is_empty() {
        return (named, StructShape::Named);
    }
    if !tuple.is_empty() {
        return (tuple, StructShape::Tuple);
    }

    (Vec::new(), StructShape::Unit)
}

fn parse_named_fields<'tcx>(unit: CompileUnit<'tcx>, list: Node<'tcx>) -> Vec<StructField> {
    let mut fields = Vec::new();
    let mut cursor = list.walk();
    for child in list.named_children(&mut cursor) {
        if child.kind() == "field_declaration" {
            let name = child
                .child_by_field_name("name")
                .map(|n| clean(&node_text(unit, n)));
            let ty = child
                .child_by_field_name("type")
                .map(|n| parse_type_expr(unit, n));

            let mut field = StructField::new(name);
            field.type_annotation = ty;
            fields.push(field);
        }
    }
    fields
}

fn parse_tuple_fields<'tcx>(unit: CompileUnit<'tcx>, list: Node<'tcx>) -> Vec<StructField> {
    let mut fields = Vec::new();
    let mut cursor = list.walk();
    for child in list.children(&mut cursor) {
        match child.kind() {
            "tuple_field_declaration" | "ordered_field_declaration" => {
                let ty = child
                    .child_by_field_name("type")
                    .map(|n| parse_type_expr(unit, n))
                    .or_else(|| {
                        child
                            .children(&mut child.walk())
                            .find_map(|n| is_type_kind(n.kind()).then(|| parse_type_expr(unit, n)))
                    });
                let mut field = StructField::new(None);
                field.type_annotation = ty;
                fields.push(field);
            }
            kind if is_type_kind(kind) => {
                let mut field = StructField::new(None);
                field.type_annotation = Some(parse_type_expr(unit, child));
                fields.push(field);
            }
            _ => {}
        }
    }
    fields
}

fn is_type_kind(kind: &str) -> bool {
    matches!(
        kind,
        "type_identifier"
            | "primitive_type"
            | "scoped_type_identifier"
            | "generic_type"
            | "tuple_type"
            | "reference_type"
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

fn parse_named_field_node<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> StructField {
    let name = node
        .child_by_field_name("name")
        .map(|n| clean(&node_text(unit, n)));
    let ty = node
        .child_by_field_name("type")
        .map(|n| parse_type_expr(unit, n));
    let mut field = StructField::new(name);
    field.type_annotation = ty;
    field
}

fn parse_tuple_field_node<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> StructField {
    let ty = node
        .child_by_field_name("type")
        .map(|n| parse_type_expr(unit, n));
    let mut field = StructField::new(None);
    field.type_annotation = ty;
    field
}
