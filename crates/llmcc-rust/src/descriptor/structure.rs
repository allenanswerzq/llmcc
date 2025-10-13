use llmcc_core::context::Context;
use llmcc_core::ir::{HirId, HirNode};
use tree_sitter::Node;

use super::function::{parse_type_expr, FnVisibility, TypeExpr};

/// High level classification for a struct declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructKind {
    Named,
    Tuple,
    Unit,
}

/// Structured metadata for Rust structs.
#[derive(Debug, Clone)]
pub struct StructDescriptor {
    pub hir_id: HirId,
    pub name: String,
    pub fqn: String,
    pub visibility: FnVisibility,
    pub generics: Option<String>,
    pub fields: Vec<StructField>,
    pub kind: StructKind,
}

/// Field metadata for named or tuple structs.
#[derive(Debug, Clone)]
pub struct StructField {
    pub name: Option<String>,
    pub ty: Option<TypeExpr>,
}

impl StructDescriptor {
    pub fn from_struct<'tcx>(
        ctx: Context<'tcx>,
        node: &HirNode<'tcx>,
        fqn: String,
    ) -> Option<Self> {
        let ts_node = match node.inner_ts_node() {
            ts if ts.kind() == "struct_item" => ts,
            _ => return None,
        };

        #[cfg(test)]
        eprintln!(
            "struct_item fields? {} tuple? {} sexp: {}",
            ts_node.child_by_field_name("fields").is_some(),
            ts_node.child_by_field_name("body").is_some(),
            ts_node.to_sexp()
        );

        let name_node = ts_node.child_by_field_name("name")?;
        let name = clean(&node_text(ctx, name_node));
        let header_text = ctx
            .file()
            .get_text(ts_node.start_byte(), name_node.start_byte());
        let visibility = FnVisibility::from_header(&header_text);

        let generics = ts_node
            .child_by_field_name("type_parameters")
            .map(|n| clean(&node_text(ctx, n)));

        let (fields, kind) = parse_struct_fields(ctx, ts_node);

        Some(StructDescriptor {
            hir_id: node.hir_id(),
            name,
            fqn,
            visibility,
            generics,
            fields,
            kind,
        })
    }
}

fn parse_struct_fields<'tcx>(
    ctx: Context<'tcx>,
    node: Node<'tcx>,
) -> (Vec<StructField>, StructKind) {
    match node.kind() {
        "field_declaration_list" => return (parse_named_fields(ctx, node), StructKind::Named),
        "tuple_field_declaration_list" | "ordered_field_declaration_list" => {
            return (parse_tuple_fields(ctx, node), StructKind::Tuple)
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
                    return (parse_named_fields(ctx, child), StructKind::Named)
                }
                "tuple_field_declaration_list" | "ordered_field_declaration_list" => {
                    return (parse_tuple_fields(ctx, child), StructKind::Tuple)
                }
                "field_declaration" => named.push(parse_named_field_node(ctx, child)),
                "tuple_field_declaration" | "ordered_field_declaration" => {
                    tuple.push(parse_tuple_field_node(ctx, child))
                }
                _ => {
                    let (fields, kind) = parse_struct_fields(ctx, child);
                    match kind {
                        StructKind::Named if !fields.is_empty() => {
                            return (fields, StructKind::Named)
                        }
                        StructKind::Tuple if !fields.is_empty() => {
                            return (fields, StructKind::Tuple)
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if !named.is_empty() {
        return (named, StructKind::Named);
    }
    if !tuple.is_empty() {
        return (tuple, StructKind::Tuple);
    }

    (Vec::new(), StructKind::Unit)
}

fn parse_named_fields<'tcx>(ctx: Context<'tcx>, list: Node<'tcx>) -> Vec<StructField> {
    let mut fields = Vec::new();
    let mut cursor = list.walk();
    for child in list.named_children(&mut cursor) {
        if child.kind() == "field_declaration" {
            let name = child
                .child_by_field_name("name")
                .map(|n| clean(&node_text(ctx, n)));
            let ty = child
                .child_by_field_name("type")
                .map(|n| parse_type_expr(ctx, n));
            fields.push(StructField { name, ty });
        }
    }
    fields
}

fn parse_tuple_fields<'tcx>(ctx: Context<'tcx>, list: Node<'tcx>) -> Vec<StructField> {
    let mut fields = Vec::new();
    let mut cursor = list.walk();
    for child in list.children(&mut cursor) {
        match child.kind() {
            "tuple_field_declaration" | "ordered_field_declaration" => {
                let ty = child
                    .child_by_field_name("type")
                    .map(|n| parse_type_expr(ctx, n))
                    .or_else(|| {
                        // ordered_field_declaration may expose type nodes directly
                        child
                            .children(&mut child.walk())
                            .find_map(|n| match n.kind() {
                                "type_identifier"
                                | "primitive_type"
                                | "scoped_type_identifier"
                                | "generic_type"
                                | "tuple_type"
                                | "reference_type"
                                | "impl_trait_type" => Some(parse_type_expr(ctx, n)),
                                _ => None,
                            })
                    });
                fields.push(StructField { name: None, ty });
            }
            kind if is_type_kind(kind) => {
                fields.push(StructField {
                    name: None,
                    ty: Some(parse_type_expr(ctx, child)),
                });
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

fn node_text<'tcx>(ctx: Context<'tcx>, node: Node<'tcx>) -> String {
    ctx.file().get_text(node.start_byte(), node.end_byte())
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

fn parse_named_field_node<'tcx>(ctx: Context<'tcx>, node: Node<'tcx>) -> StructField {
    let name = node
        .child_by_field_name("name")
        .map(|n| clean(&node_text(ctx, n)));
    let ty = node
        .child_by_field_name("type")
        .map(|n| parse_type_expr(ctx, n));
    StructField { name, ty }
}

fn parse_tuple_field_node<'tcx>(ctx: Context<'tcx>, node: Node<'tcx>) -> StructField {
    let ty = node
        .child_by_field_name("type")
        .map(|n| parse_type_expr(ctx, n));
    StructField { name: None, ty }
}
