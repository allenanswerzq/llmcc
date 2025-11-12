use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::TypeExpr;

use super::function::parse_type_expr;
use crate::token::LangRust;

#[derive(Debug, Default, Clone)]
pub struct ParameterDescriptor {
    names: Vec<String>,
    type_annotation: Option<TypeExpr>,
}

impl ParameterDescriptor {
    pub fn build<'tcx>(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
    ) -> Option<ParameterDescriptor> {
        let mut descriptor = ParameterDescriptor::default();

        if let Some(pattern) = node.opt_child_by_field(unit, LangRust::field_pattern) {
            descriptor.names = collect_pattern_bindings(unit, pattern.inner_ts_node());
        }

        if descriptor.names.is_empty() {
            if let Some(ident) = node
                .opt_child_by_field(unit, LangRust::field_pattern)
                .and_then(|child| child.find_ident(unit))
            {
                descriptor.names.push(ident.name.to_string());
            }
        }

        descriptor.type_annotation = node
            .opt_child_by_field(unit, LangRust::field_type)
            .map(|child| parse_type_expr(unit, child.inner_ts_node()));

        if descriptor.names.is_empty() {
            return None;
        }

        Some(descriptor)
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn type_annotation(&self) -> Option<&TypeExpr> {
        self.type_annotation.as_ref()
    }
}

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

fn collect_pattern_bindings_internal<'tcx>(
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
                collect_pattern_bindings_internal(unit, child, true, out);
            }
            return;
        }
        "struct_pattern" | "tuple_struct_pattern" => {
            let mut cursor = node.walk();
            for (idx, child) in node.named_children(&mut cursor).enumerate() {
                if idx == 0 {
                    continue;
                }
                collect_pattern_bindings_internal(unit, child, true, out);
            }
            return;
        }
        "scoped_identifier" | "type_identifier" | "primitive_type" if !allow_binding => return,
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_pattern_bindings_internal(unit, child, allow_binding, out);
    }
}

fn collect_pattern_bindings<'tcx>(unit: CompileUnit<'tcx>, pattern: Node<'tcx>) -> Vec<String> {
    let mut names = Vec::new();
    collect_pattern_bindings_internal(unit, pattern, true, &mut names);
    names.sort_by_key(|entry| entry.0);
    names.into_iter().map(|(_, name)| name).collect()
}
