use std::collections::BTreeMap;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{ClassDescriptor, ClassField, LANGUAGE_PYTHON, StructKind, TypeExpr};

use crate::token::LangPython;

use super::origin::build_origin;

pub fn build<'tcx>(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ClassDescriptor> {
    if node.kind_id() != LangPython::class_definition {
        return None;
    }

    let name_node = node.opt_child_by_field(unit, LangPython::field_name)?;
    let ident = name_node.as_ident()?;
    let ts_node = node.inner_ts_node();

    let origin = build_origin(unit, node, ts_node);
    let mut descriptor = ClassDescriptor::new(origin, ident.name.clone());
    descriptor.kind = StructKind::Class;

    descriptor.decorators = collect_decorators(unit, ts_node);
    descriptor.base_types = collect_base_types(unit, node);

    let mut fields = BTreeMap::new();
    collect_members(unit, node, &mut descriptor.methods, &mut fields);
    descriptor.fields = fields.into_values().collect();

    Some(descriptor)
}

fn collect_base_types<'tcx>(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Vec<TypeExpr> {
    let mut types = Vec::new();
    for child_id in node.children() {
        let child = unit.hir_node(*child_id);
        if child.kind_id() == LangPython::argument_list {
            let ts_node = child.inner_ts_node();
            let mut cursor = ts_node.walk();
            for base in ts_node.named_children(&mut cursor) {
                let text = unit.get_text(base.start_byte(), base.end_byte());
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    types.push(TypeExpr::opaque(LANGUAGE_PYTHON, trimmed.to_string()));
                }
            }
        }
    }
    types
}

fn collect_members<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    methods: &mut Vec<String>,
    fields: &mut BTreeMap<String, ClassField>,
) {
    for child_id in node.children() {
        let child = unit.hir_node(*child_id);
        if child.kind_id() == LangPython::block {
            collect_block_members(unit, &child, methods, fields);
        }
    }
}

fn collect_block_members<'tcx>(
    unit: CompileUnit<'tcx>,
    body_node: &HirNode<'tcx>,
    methods: &mut Vec<String>,
    fields: &mut BTreeMap<String, ClassField>,
) {
    for child_id in body_node.children() {
        let child = unit.hir_node(*child_id);
        let kind_id = child.kind_id();

        if kind_id == LangPython::function_definition {
            if let Some(name_node) = child.opt_child_by_field(unit, LangPython::field_name) {
                if let Some(ident) = name_node.as_ident() {
                    methods.push(ident.name.clone());
                }
            }
            collect_instance_fields(unit, &child, fields);
        } else if kind_id == LangPython::decorated_definition {
            if let Some(method_name) = extract_decorated_method_name(unit, &child) {
                methods.push(method_name);
            }
            if let Some(method_node) = decorated_method_node(unit, &child) {
                collect_instance_fields(unit, &method_node, fields);
            }
        } else if kind_id == LangPython::assignment {
            if let Some(field) = extract_class_field(unit, &child) {
                upsert_field(fields, field);
            }
        } else if kind_id == LangPython::expression_statement {
            for stmt_child_id in child.children() {
                let stmt_child = unit.hir_node(*stmt_child_id);
                if stmt_child.kind_id() == LangPython::assignment {
                    if let Some(field) = extract_class_field(unit, &stmt_child) {
                        upsert_field(fields, field);
                    }
                }
            }
        }
    }
}

fn collect_instance_fields<'tcx>(
    unit: CompileUnit<'tcx>,
    method_node: &HirNode<'tcx>,
    fields: &mut BTreeMap<String, ClassField>,
) {
    collect_instance_fields_recursive(unit, method_node, fields);
}

fn collect_instance_fields_recursive<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    fields: &mut BTreeMap<String, ClassField>,
) {
    if node.kind_id() == LangPython::assignment {
        if let Some(field) = extract_instance_field_from_assignment(unit, node) {
            upsert_field(fields, field);
        }
    }

    for child_id in node.children() {
        let child = unit.hir_node(*child_id);
        collect_instance_fields_recursive(unit, &child, fields);
    }
}

fn extract_class_field<'tcx>(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ClassField> {
    let left_node = node.opt_child_by_field(unit, LangPython::field_left)?;
    let ident = left_node.as_ident()?;

    let mut field = ClassField::new(ident.name.clone());

    let ts_node = node.inner_ts_node();
    let type_hint = node
        .opt_child_by_field(unit, LangPython::field_type)
        .map(|type_node| {
            unit.get_text(
                type_node.inner_ts_node().start_byte(),
                type_node.inner_ts_node().end_byte(),
            )
        })
        .and_then(|text| {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .or_else(|| {
            let assignment_text = unit.get_text(ts_node.start_byte(), ts_node.end_byte());
            parse_annotation_from_assignment_text(&assignment_text)
        });

    if let Some(type_hint) = type_hint {
        field.type_annotation = Some(TypeExpr::opaque(LANGUAGE_PYTHON, type_hint));
    }

    Some(field)
}

fn extract_instance_field_from_assignment<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<ClassField> {
    let left_node = node.opt_child_by_field(unit, LangPython::field_left)?;
    if left_node.kind_id() != LangPython::attribute {
        return None;
    }

    let mut identifier_names = Vec::new();
    for child_id in left_node.children() {
        let child = unit.hir_node(*child_id);
        if child.kind_id() == LangPython::identifier {
            if let Some(ident) = child.as_ident() {
                identifier_names.push(ident.name.clone());
            }
        }
    }

    if identifier_names.first().map(String::as_str) != Some("self") {
        return None;
    }

    let field_name = match identifier_names.last() {
        Some(name) if name != "self" => name.clone(),
        _ => return None,
    };

    Some(ClassField::new(field_name))
}

fn upsert_field(fields: &mut BTreeMap<String, ClassField>, field: ClassField) {
    if let Some(name) = field.name.clone() {
        fields
            .entry(name)
            .and_modify(|existing| {
                if existing.type_annotation.is_none() {
                    existing.type_annotation = field.type_annotation.clone();
                }
            })
            .or_insert(field);
    }
}

fn parse_annotation_from_assignment_text(text: &str) -> Option<String> {
    let colon_idx = text.find(':')?;
    let after_colon = text[colon_idx + 1..].trim();
    if after_colon.is_empty() {
        return None;
    }

    let annotation = after_colon.split('=').next().map(str::trim).unwrap_or("");

    if annotation.is_empty() {
        None
    } else {
        Some(annotation.to_string())
    }
}

fn extract_decorated_method_name<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<String> {
    for child_id in node.children() {
        let child = unit.hir_node(*child_id);
        if child.kind_id() == LangPython::function_definition {
            if let Some(name_node) = child.opt_child_by_field(unit, LangPython::field_name) {
                if let Some(ident) = name_node.as_ident() {
                    return Some(ident.name.clone());
                }
            }
        }
    }
    None
}

fn decorated_method_node<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<HirNode<'tcx>> {
    for child_id in node.children() {
        let child = unit.hir_node(*child_id);
        if child.kind_id() == LangPython::function_definition {
            return Some(child);
        }
    }
    None
}

fn collect_decorators<'tcx>(unit: CompileUnit<'tcx>, ts_node: Node<'tcx>) -> Vec<String> {
    let mut decorators = Vec::new();
    if let Some(parent) = ts_node.parent() {
        if parent.kind() == "decorated_definition" {
            let mut cursor = parent.walk();
            for child in parent.named_children(&mut cursor) {
                if child.kind() == "decorator" {
                    let text = unit.get_text(child.start_byte(), child.end_byte());
                    let trimmed = text.trim_start_matches('@').trim().to_string();
                    if !trimmed.is_empty() {
                        decorators.push(trimmed);
                    }
                }
            }
        }
    }
    decorators
}
