use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;

use llmcc_descriptor::{VariableDescriptor, VariableKind, VariableScope};

use crate::token::LangPython;

use super::origin::build_origin;

pub fn build<'tcx>(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<VariableDescriptor> {
    let ts_node = node.inner_ts_node();
    if node.kind_id() != LangPython::assignment {
        return None;
    }

    let origin = build_origin(unit, node, ts_node);
    let mut descriptor = VariableDescriptor::new(origin, "".to_string());
    descriptor.scope = VariableScope::Unknown;
    descriptor.kind = VariableKind::Binding;

    let value_text = unit.get_text(ts_node.start_byte(), ts_node.end_byte());
    let trimmed = value_text.trim();
    if !trimmed.is_empty() {
        descriptor.value_repr = Some(trimmed.to_string());
    }

    Some(descriptor)
}
