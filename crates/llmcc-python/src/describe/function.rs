use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{
    DescriptorMeta, FunctionDescriptor, FunctionParameter, ParameterKind, TypeExpr, LANGUAGE_PYTHON,
};

use crate::token::LangPython;

use super::origin::build_origin;

pub fn build<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    meta: DescriptorMeta<'_>,
) -> Option<FunctionDescriptor> {
    if node.kind_id() != LangPython::function_definition {
        return None;
    }

    let DescriptorMeta::Function { fqn } = meta else {
        return None;
    };

    let name_node = node.opt_child_by_field(unit, LangPython::field_name)?;
    let ident = name_node.as_ident()?;
    let ts_node = node.inner_ts_node();

    let origin = build_origin(unit, node, ts_node);
    let mut descriptor = FunctionDescriptor::new(origin, ident.name.clone());
    descriptor.fqn = fqn.map(|value| value.to_string());

    descriptor.parameters = collect_parameters(unit, node);
    descriptor.return_type = extract_return_type(unit, node);
    descriptor.signature = Some(unit.get_text(ts_node.start_byte(), ts_node.end_byte()));
    descriptor.decorators = collect_decorators(unit, ts_node);

    Some(descriptor)
}

fn collect_parameters<'tcx>(
    unit: CompileUnit<'tcx>,
    func_node: &HirNode<'tcx>,
) -> Vec<FunctionParameter> {
    let mut params = Vec::new();

    for child_id in func_node.children() {
        let child = unit.hir_node(*child_id);
        if child.kind_id() == LangPython::parameters {
            for param_id in child.children() {
                let param_node = unit.hir_node(*param_id);
                if let Some(mut param) = parse_parameter_node(unit, &param_node) {
                    if matches!(param.name.as_deref(), Some("self")) {
                        param.kind = ParameterKind::Receiver;
                    }
                    params.push(param);
                }
            }
        }
    }

    params
}

fn parse_parameter_node<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<FunctionParameter> {
    let kind_id = node.kind_id();

    if kind_id == LangPython::Text_COMMA {
        return None;
    }

    if kind_id == LangPython::identifier {
        if let Some(ident) = node.as_ident() {
            let mut param = FunctionParameter::new(Some(ident.name.clone()));
            param.pattern = Some(ident.name.clone());
            return Some(param);
        }
        return None;
    }

    if kind_id == LangPython::typed_parameter || kind_id == LangPython::typed_default_parameter {
        return parse_typed_parameter(unit, node);
    }

    let text = unit.get_text(
        node.inner_ts_node().start_byte(),
        node.inner_ts_node().end_byte(),
    );
    parse_parameter_from_text(&text)
}

fn parse_typed_parameter<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
) -> Option<FunctionParameter> {
    let mut name = None;
    let mut type_hint = None;
    let mut default_value = None;

    for child_id in node.children() {
        let child = unit.hir_node(*child_id);
        let kind_id = child.kind_id();

        if kind_id == LangPython::identifier {
            if let Some(ident) = child.as_ident() {
                if name.is_none() {
                    name = Some(ident.name.clone());
                }
            }
        } else if kind_id == LangPython::type_node {
            let text = unit.get_text(
                child.inner_ts_node().start_byte(),
                child.inner_ts_node().end_byte(),
            );
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                type_hint = Some(trimmed.to_string());
            }
        } else if kind_id != LangPython::Text_COLON && kind_id != LangPython::Text_EQ {
            let text = unit.get_text(
                child.inner_ts_node().start_byte(),
                child.inner_ts_node().end_byte(),
            );
            let trimmed = text.trim();
            if !trimmed.is_empty() && trimmed != "=" && trimmed != ":" {
                default_value = Some(trimmed.to_string());
            }
        }
    }

    let name = name?;
    let mut param = FunctionParameter::new(Some(name.clone()));
    param.pattern = Some(name);
    if let Some(type_hint) = type_hint {
        param.type_hint = Some(TypeExpr::opaque(LANGUAGE_PYTHON, type_hint));
    }
    if let Some(default) = default_value {
        param.default_value = Some(default);
    }
    Some(param)
}

fn parse_parameter_from_text(param_text: &str) -> Option<FunctionParameter> {
    let trimmed = param_text.trim();
    if trimmed.is_empty() || matches!(trimmed, "(" | ")") {
        return None;
    }

    let (kind, base) = if let Some(rest) = trimmed.strip_prefix("**") {
        (ParameterKind::VariadicKeyword, rest)
    } else if let Some(rest) = trimmed.strip_prefix('*') {
        (ParameterKind::VariadicPositional, rest)
    } else {
        (ParameterKind::Positional, trimmed)
    };

    let mut name_part = base.trim();
    let mut type_hint = None;
    let mut default_value = None;

    if let Some(colon_pos) = name_part.find(':') {
        let (name, type_part) = name_part.split_at(colon_pos);
        name_part = name;
        let remaining = type_part.trim_start_matches(':').trim();
        if let Some(eq_pos) = remaining.find('=') {
            let (type_text, default_part) = remaining.split_at(eq_pos);
            if !type_text.trim().is_empty() {
                type_hint = Some(type_text.trim().to_string());
            }
            default_value = Some(default_part.trim_start_matches('=').trim().to_string());
        } else if !remaining.is_empty() {
            type_hint = Some(remaining.to_string());
        }
    }

    if let Some(eq_pos) = name_part.find('=') {
        let (name, default_part) = name_part.split_at(eq_pos);
        name_part = name;
        if default_value.is_none() {
            default_value = Some(default_part.trim_start_matches('=').trim().to_string());
        }
    }

    let cleaned_name = name_part.trim();
    let name_option = if cleaned_name.is_empty() {
        None
    } else {
        Some(cleaned_name.to_string())
    };

    let mut param = FunctionParameter::new(name_option.clone());
    param.pattern = Some(trimmed.to_string());
    param.kind = kind;
    if let Some(type_hint) = type_hint {
        param.type_hint = Some(TypeExpr::opaque(LANGUAGE_PYTHON, type_hint));
    }
    if let Some(default) = default_value {
        if !default.is_empty() {
            param.default_value = Some(default);
        }
    }

    Some(param)
}

fn extract_return_type<'tcx>(
    unit: CompileUnit<'tcx>,
    func_node: &HirNode<'tcx>,
) -> Option<TypeExpr> {
    let ts_node = func_node.inner_ts_node();
    let mut cursor = ts_node.walk();
    let mut found_arrow = false;

    for child in ts_node.children(&mut cursor) {
        let kind = child.kind();
        if found_arrow && child.is_named() {
            let text = unit.get_text(child.start_byte(), child.end_byte());
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(TypeExpr::opaque(LANGUAGE_PYTHON, trimmed.to_string()));
            }
        }

        if kind == "->" {
            found_arrow = true;
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
