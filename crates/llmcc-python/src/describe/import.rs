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
    let statement = unit.get_text(ts_node.start_byte(), ts_node.end_byte());
    let mut descriptor = ImportDescriptor::new(origin, "");

    if ts_node.kind() == "import_statement" {
        parse_import_statement(&statement, &mut descriptor);
    } else {
        parse_from_statement(&statement, &mut descriptor);
    }

    if descriptor.source.is_empty() {
        None
    } else {
        Some(descriptor)
    }
}

fn parse_import_statement(text: &str, descriptor: &mut ImportDescriptor) {
    let rest = text.trim_start().trim_start_matches("import").trim();
    if rest.is_empty() {
        return;
    }

    let first_segment = rest.split(',').next().unwrap_or("").trim();
    if first_segment.is_empty() {
        return;
    }

    let (module, alias) = split_alias(first_segment);
    descriptor.source = module.to_string();
    descriptor.alias = alias;
    descriptor.kind = ImportKind::Module;
}

fn parse_from_statement(text: &str, descriptor: &mut ImportDescriptor) {
    let rest = text.trim_start().trim_start_matches("from").trim();
    let Some((module_part, import_part)) = rest.split_once(" import ") else {
        return;
    };

    let module = module_part.trim();
    let names_part = import_part.trim();
    if names_part.is_empty() {
        descriptor.source = module.to_string();
        descriptor.kind = ImportKind::Unknown;
        return;
    }

    let first_name = names_part.split(',').next().unwrap_or("").trim();
    if first_name.is_empty() {
        descriptor.source = module.to_string();
        descriptor.kind = ImportKind::Unknown;
        return;
    }

    if first_name == "*" {
        descriptor.source = module.to_string();
        descriptor.kind = ImportKind::Wildcard;
        return;
    }

    let (name, alias) = split_alias(first_name);
    descriptor.alias = alias;

    if module.is_empty() {
        descriptor.source = name.to_string();
    } else {
        descriptor.source = format!("{}::{}", module, name);
    }
    descriptor.kind = ImportKind::Item;
}

fn split_alias(segment: &str) -> (&str, Option<String>) {
    let mut parts = segment.splitn(2, " as ");
    let module = parts.next().unwrap_or("").trim();
    let alias = parts
        .next()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    (module, alias)
}
