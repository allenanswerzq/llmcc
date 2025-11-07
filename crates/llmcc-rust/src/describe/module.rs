use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use tree_sitter::Node;

use llmcc_descriptor::{ModuleDescriptor, Visibility};

use super::function::{build_origin, parse_visibility};

pub fn build<'tcx>(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ModuleDescriptor> {
    let ts_node = match node.inner_ts_node() {
        ts if ts.kind() == "mod_item" => ts,
        _ => return None,
    };

    let name_node = ts_node.child_by_field_name("name")?;
    let name = unit.ts_text(name_node);

    let visibility = header_visibility(unit, ts_node, name_node);
    let is_inline = ts_node.child_by_field_name("body").is_some();
    let origin = build_origin(unit, node, ts_node);

    let mut descriptor = ModuleDescriptor::new(origin, name);
    descriptor.visibility = visibility;
    descriptor.is_inline = is_inline;

    Some(descriptor)
}

fn header_visibility<'tcx>(
    unit: CompileUnit<'tcx>,
    ts_node: Node<'tcx>,
    name_node: Node<'tcx>,
) -> Visibility {
    let header = unit
        .file()
        .get_text(ts_node.start_byte(), name_node.start_byte());
    parse_visibility(&header)
}
