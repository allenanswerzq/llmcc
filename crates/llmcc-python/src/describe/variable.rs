use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;

use llmcc_descriptor::{DescriptorMeta, VariableDescriptor, VariableKind, VariableScope};

use crate::token::LangPython;

use super::origin::build_origin;

pub fn build<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    meta: DescriptorMeta<'_>,
) -> Option<VariableDescriptor> {
    let (fqn, name, scope) = match meta {
        DescriptorMeta::Variable { fqn, name, scope } => (fqn?, name?, scope),
        _ => return None,
    };

    let ts_node = node.inner_ts_node();
    if node.kind_id() != LangPython::assignment {
        return None;
    }

    let origin = build_origin(unit, node, ts_node);
    let mut descriptor = VariableDescriptor::new(origin, name.to_string());
    descriptor.fqn = Some(fqn.to_string());
    descriptor.scope = scope.unwrap_or(VariableScope::Unknown);
    descriptor.kind = VariableKind::Binding;

    let value_text = unit.get_text(ts_node.start_byte(), ts_node.end_byte());
    let trimmed = value_text.trim();
    if !trimmed.is_empty() {
        descriptor.value_repr = Some(trimmed.to_string());
    }

    Some(descriptor)
}
