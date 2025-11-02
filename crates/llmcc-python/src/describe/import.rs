use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;

use llmcc_descriptor::{DescriptorMeta, ImportDescriptor, ImportKind};

use super::origin::build_origin;

pub fn build<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    meta: DescriptorMeta<'_>,
) -> Option<ImportDescriptor> {
    if !matches!(meta, DescriptorMeta::Import) {
        return None;
    }

    let ts_node = node.inner_ts_node();
    if ts_node.kind() != "import_statement" && ts_node.kind() != "import_from_statement" {
        return None;
    }

    let origin = build_origin(unit, node, ts_node);
    let source_text = unit
        .get_text(ts_node.start_byte(), ts_node.end_byte())
        .trim()
        .to_string();

    let mut descriptor = ImportDescriptor::new(origin, source_text);
    descriptor.kind = if ts_node.kind() == "import_from_statement" {
        ImportKind::Item
    } else {
        ImportKind::Module
    };

    Some(descriptor)
}
