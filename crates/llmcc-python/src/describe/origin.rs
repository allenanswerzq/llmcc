use llmcc_core::{context::CompileUnit, ir::HirNode};
use tree_sitter::Node;

use llmcc_descriptor::{DescriptorOrigin, LANGUAGE_PYTHON, SourceLocation, SourceSpan};

pub fn build_origin<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    ts_node: Node<'tcx>,
) -> DescriptorOrigin {
    let file = unit
        .file_path()
        .or_else(|| unit.file().path())
        .map(|path| path.to_string());
    let span = SourceSpan::new(ts_node.start_byte() as u32, ts_node.end_byte() as u32);
    let location = SourceLocation::new(file, Some(span));

    DescriptorOrigin::new(LANGUAGE_PYTHON)
        .with_id(node.hir_id().0 as u64)
        .with_location(location)
}
