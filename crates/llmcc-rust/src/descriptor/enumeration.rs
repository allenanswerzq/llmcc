use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirNode};
use tree_sitter::Node;

use super::function::{parse_type_expr, FnVisibility, TypeExpr};

/// Classification of enum variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumVariantKind {
    Unit,
    Tuple,
    Struct,
}

/// Captures metadata for a single enum variant.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub kind: EnumVariantKind,
    pub fields: Vec<EnumVariantField>,
    pub discriminant: Option<String>,
}

/// Field metadata for tuple or struct variants.
#[derive(Debug, Clone)]
pub struct EnumVariantField {
    pub name: Option<String>,
    pub ty: Option<TypeExpr>,
}

/// Structured metadata for Rust enums.
#[derive(Debug, Clone)]
pub struct EnumDescriptor {
    pub hir_id: HirId,
    pub name: String,
    pub fqn: String,
    pub visibility: FnVisibility,
    pub generics: Option<String>,
    pub variants: Vec<EnumVariant>,
}

impl EnumDescriptor {
    pub fn from_enum<'tcx>(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        fqn: String,
    ) -> Option<Self> {
        let ts_node = match node.inner_ts_node() {
            ts if ts.kind() == "enum_item" => ts,
            _ => return None,
        };

        let name_node = ts_node.child_by_field_name("name")?;
        let name = clean(&node_text(unit, name_node));
        let header_text = unit
            .file()
            .get_text(ts_node.start_byte(), name_node.start_byte());
        let visibility = FnVisibility::from_header(&header_text);

        let generics = ts_node
            .child_by_field_name("type_parameters")
            .map(|n| clean(&node_text(unit, n)));

        let variants = ts_node
            .child_by_field_name("body")
            .map(|body| parse_enum_variants(unit, body))
            .unwrap_or_default();

        Some(EnumDescriptor {
            hir_id: node.hir_id(),
            name,
            fqn,
            visibility,
            generics,
            variants,
        })
    }
}

fn parse_enum_variants<'tcx>(unit: CompileUnit<'tcx>, body: Node<'tcx>) -> Vec<EnumVariant> {
    let mut variants = Vec::new();
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() == "enum_variant" {
            variants.push(parse_enum_variant(unit, child));
        }
    }
    variants
}

fn parse_enum_variant<'tcx>(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> EnumVariant {
    let name_node = node
        .child_by_field_name("name")
        .unwrap_or_else(|| node.child(0).unwrap_or(node));
    let name = clean(&node_text(unit, name_node));

    let discriminant = node
        .child_by_field_name("value")
        .map(|n| clean(&node_text(unit, n)));

    let (kind, fields) = match node.child_by_field_name("body") {
        Some(body) => match body.kind() {
            "field_declaration_list" => (
                EnumVariantKind::Struct,
                parse_named_variant_fields(unit, body),
            ),
            "ordered_field_declaration_list" | "tuple_field_declaration_list" => (
                EnumVariantKind::Tuple,
                parse_tuple_variant_fields(unit, body),
            ),
            other => parse_variant_body(unit, body, other),
        },
        None => (EnumVariantKind::Unit, Vec::new()),
    };

    EnumVariant {
        name,
        kind,
        fields,
        discriminant,
    }
}

fn parse_variant_body<'tcx>(
    unit: CompileUnit<'tcx>,
    body: Node<'tcx>,
    kind: &str,
) -> (EnumVariantKind, Vec<EnumVariantField>) {
    match kind {
        "field_declaration_list" => (
            EnumVariantKind::Struct,
            parse_named_variant_fields(unit, body),
        ),
        "ordered_field_declaration_list" | "tuple_field_declaration_list" => (
            EnumVariantKind::Tuple,
            parse_tuple_variant_fields(unit, body),
        ),
        _ => (EnumVariantKind::Unit, Vec::new()),
    }
}

fn parse_named_variant_fields<'tcx>(
    unit: CompileUnit<'tcx>,
    list: Node<'tcx>,
) -> Vec<EnumVariantField> {
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
            fields.push(EnumVariantField { name, ty });
        }
    }
    fields
}

fn parse_tuple_variant_fields<'tcx>(
    unit: CompileUnit<'tcx>,
    list: Node<'tcx>,
) -> Vec<EnumVariantField> {
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
                            .find_map(|n| is_type_node(n.kind()).then(|| parse_type_expr(unit, n)))
                    });
                fields.push(EnumVariantField { name: None, ty });
            }
            kind if is_type_node(kind) => {
                fields.push(EnumVariantField {
                    name: None,
                    ty: Some(parse_type_expr(unit, child)),
                });
            }
            _ => {}
        }
    }
    fields
}

fn is_type_node(kind: &str) -> bool {
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
