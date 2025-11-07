use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;

use llmcc_descriptor::ImplDescriptor;

use super::function::{build_origin, parse_type_expr};

/// Build a descriptor for a Rust `impl` block.
pub fn build<'tcx>(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ImplDescriptor> {
    let ts_node = match node.inner_ts_node() {
        ts if ts.kind() == "impl_item" => ts,
        _ => return None,
    };

    let type_node = ts_node.child_by_field_name("type")?;
    let target_ty = parse_type_expr(unit, type_node);

    let origin = build_origin(unit, node, ts_node);
    let mut descriptor = ImplDescriptor::new(origin, target_ty);

    if let Some(trait_node) = ts_node.child_by_field_name("trait") {
        let trait_ty = parse_type_expr(unit, trait_node);
        descriptor.trait_ty = Some(trait_ty);
    }

    Some(descriptor)
}
