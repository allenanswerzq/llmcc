use std::fmt::Write as _;
use std::path::Path;

use llmcc_core::ViewDepth;
use llmcc_core::block::BlockKind;
use llmcc_core::context::{CompileCtxt, CompileUnit};
use llmcc_core::ir_builder::{HirBuildOptions, build_hir};
use llmcc_core::lang_def::Language;
use llmcc_core::{
    BlockId, CollectedGraph, GraphBuildOptions, ProjectGraph, ResolveOptions, build_graphs,
};
use llmcc_dot::RenderOptions;
use llmcc_error::{Error, ErrorKind, Result};
use llmcc_resolver::{bind_symbols, collect_symbols};
use llmcc_rust::LangRust;
use llmcc_ts::LangTypeScript;
use walkdir::WalkDir;

use super::{
    BlockRelationSnapshot, BlockSnapshot, PipelineOptions, PipelineSummary,
    SymbolDependencySnapshot, SymbolSnapshot,
};

pub(super) fn collect_pipeline<L>(
    project_root: &Path,
    options: &PipelineOptions,
) -> Result<PipelineSummary>
where
    L: Language,
{
    let files = if options.file_paths.is_empty() {
        discover_language_files::<L>(project_root, options.parallel)?
    } else {
        let supported = L::extensions();
        options
            .file_paths
            .iter()
            .filter(|path| {
                Path::new(path)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| supported.iter().any(|s| s.eq_ignore_ascii_case(ext)))
            })
            .cloned()
            .collect::<Vec<_>>()
    };

    let cc = CompileCtxt::from_files::<L>(&files)?;
    let sequential = !options.parallel;
    let ir_options = HirBuildOptions::new().with_sequential(sequential);
    build_hir::<L>(&cc, ir_options)?;

    let resolve_options = ResolveOptions::default()
        .with_print_ir(options.print_ir)
        .with_sequential(sequential);
    let globals = collect_symbols::<L>(&cc, &resolve_options)?;
    bind_symbols::<L>(&cc, globals, &resolve_options)?;

    let mut project_graph = if options.build_block_reports
        || options.build_block_graph
        || options.keep_block_relations
        || options.build_arch_graph
    {
        Some(ProjectGraph::new(&cc))
    } else {
        None
    };

    if let Some(project) = project_graph.as_mut() {
        let unit_graphs =
            build_graphs::<L>(&cc, GraphBuildOptions::new().with_sequential(sequential)).unwrap();
        project.add_units(unit_graphs);
    }

    let (
        dep_graph_dot,
        arch_graph_dot,
        arch_graph_depth_0,
        arch_graph_depth_1,
        arch_graph_depth_2,
        arch_graph_depth_3,
        block_list,
        block_deps,
        block_graph,
        block_relations,
    ) = if let Some(project) = project_graph {
        project.link_blocks();
        let graph = CollectedGraph::new(&project);
        let opts = RenderOptions::default();
        let render_at = |level: ViewDepth| llmcc_dot::render(&graph, level, &opts);

        let dep_graph: Option<String> = None;
        let arch_graph: Option<String> = if options.build_arch_graph {
            Some(render_at(options.view_depth))
        } else {
            None
        };
        let arch_graph_d0: Option<String> = if options.build_arch_graph_depth_0 {
            Some(render_at(ViewDepth::Project))
        } else {
            None
        };
        let arch_graph_d1: Option<String> = if options.build_arch_graph_depth_1 {
            Some(render_at(ViewDepth::Package))
        } else {
            None
        };
        let arch_graph_d2: Option<String> = if options.build_arch_graph_depth_2 {
            Some(render_at(ViewDepth::Module))
        } else {
            None
        };
        let arch_graph_d3: Option<String> = if options.build_arch_graph_depth_3 {
            Some(render_at(ViewDepth::File))
        } else {
            None
        };
        let (list, deps) = if options.build_block_reports {
            let (blocks, deps) = render_block_reports(&project);
            (Some(blocks), Some(deps))
        } else {
            (None, None)
        };
        let block_graph = if options.build_block_graph {
            Some(render_block_graph(&project))
        } else {
            None
        };
        let block_relations = if options.keep_block_relations {
            Some(snapshot_block_relations(&project))
        } else {
            None
        };
        (
            dep_graph,
            arch_graph,
            arch_graph_d0,
            arch_graph_d1,
            arch_graph_d2,
            arch_graph_d3,
            list,
            deps,
            block_graph,
            block_relations,
        )
    } else {
        (None, None, None, None, None, None, None, None, None, None)
    };

    let symbols = if options.keep_symbols {
        Some(snapshot_symbols(&cc))
    } else {
        None
    };

    let symbol_types = if options.keep_symbol_types {
        Some(snapshot_symbols(&cc))
    } else {
        None
    };

    let symbol_deps = if options.keep_symbol_deps {
        Some(snapshot_symbol_dependencies(&cc))
    } else {
        None
    };

    Ok(PipelineSummary {
        symbols,
        symbol_types,
        block_relations,
        dep_graph_dot,
        arch_graph_dot,
        arch_graph_depth_0,
        arch_graph_depth_1,
        arch_graph_depth_2,
        arch_graph_depth_3,
        block_list,
        block_deps,
        symbol_deps,
        block_graph,
        temp_dir_path: None,
    })
}

fn discover_language_files<L: Language>(root: &Path, _parallel: bool) -> Result<Vec<String>> {
    let supported = L::extensions();
    let mut files = Vec::new();

    for entry in WalkDir::new(root) {
        let entry = entry.map_err(|e| {
            Error::new(
                ErrorKind::IoFailed,
                format!("failed to walk {}: {}", root.display(), e),
            )
        })?;
        if !entry.file_type().is_file() {
            continue;
        }

        let Some(ext) = entry.path().extension().and_then(|ext| ext.to_str()) else {
            continue;
        };

        if !supported
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(ext))
        {
            continue;
        }

        files.push(entry.path().to_string_lossy().to_string());
    }

    files.sort_by(|a, b| {
        let get_prefix = |path: &str| -> Option<usize> {
            let filename = Path::new(path).file_name()?.to_str()?;
            let prefix_end = filename.find('_')?;
            filename[..prefix_end].parse::<usize>().ok()
        };
        get_prefix(a).cmp(&get_prefix(b))
    });

    Ok(files)
}

pub(super) fn collect_pipeline_auto(
    project_root: &Path,
    options: &PipelineOptions,
) -> Result<PipelineSummary> {
    let rust_files = discover_language_files::<LangRust>(project_root, options.parallel)?;
    let ts_files = discover_language_files::<LangTypeScript>(project_root, options.parallel)?;

    let mut arch_graphs: Vec<String> = Vec::new();
    let mut arch_graphs_d0: Vec<String> = Vec::new();
    let mut arch_graphs_d1: Vec<String> = Vec::new();
    let mut arch_graphs_d2: Vec<String> = Vec::new();
    let mut arch_graphs_d3: Vec<String> = Vec::new();

    if !rust_files.is_empty() {
        let summary = collect_pipeline::<LangRust>(project_root, options)?;
        if let Some(graph) = summary.arch_graph_dot {
            arch_graphs.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_0 {
            arch_graphs_d0.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_1 {
            arch_graphs_d1.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_2 {
            arch_graphs_d2.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_3 {
            arch_graphs_d3.push(graph);
        }
    }

    if !ts_files.is_empty() {
        let summary = collect_pipeline::<LangTypeScript>(project_root, options)?;
        if let Some(graph) = summary.arch_graph_dot {
            arch_graphs.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_0 {
            arch_graphs_d0.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_1 {
            arch_graphs_d1.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_2 {
            arch_graphs_d2.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_3 {
            arch_graphs_d3.push(graph);
        }
    }

    let merged_arch = if arch_graphs.is_empty() {
        None
    } else if arch_graphs.len() == 1 {
        Some(arch_graphs.into_iter().next().unwrap())
    } else {
        Some(merge_dot_graphs(&arch_graphs))
    };

    let merged_d0 = merge_if_multiple(arch_graphs_d0);
    let merged_d1 = merge_if_multiple(arch_graphs_d1);
    let merged_d2 = merge_if_multiple(arch_graphs_d2);
    let merged_d3 = merge_if_multiple(arch_graphs_d3);

    Ok(PipelineSummary {
        symbols: None,
        symbol_types: None,
        block_relations: None,
        dep_graph_dot: None,
        arch_graph_dot: merged_arch,
        arch_graph_depth_0: merged_d0,
        arch_graph_depth_1: merged_d1,
        arch_graph_depth_2: merged_d2,
        arch_graph_depth_3: merged_d3,
        block_list: None,
        block_deps: None,
        symbol_deps: None,
        block_graph: None,
        temp_dir_path: None,
    })
}

fn merge_if_multiple(graphs: Vec<String>) -> Option<String> {
    if graphs.is_empty() {
        None
    } else if graphs.len() == 1 {
        Some(graphs.into_iter().next().unwrap())
    } else {
        Some(merge_dot_graphs(&graphs))
    }
}

fn merge_dot_graphs(graphs: &[String]) -> String {
    let mut merged = String::new();
    let _ = writeln!(merged, "digraph architecture {{");
    let _ = writeln!(merged, "  rankdir=TB;");
    let _ = writeln!(merged, "  ranksep=0.8;");
    let _ = writeln!(merged, "  nodesep=0.4;");
    let _ = writeln!(merged, "  splines=ortho;");
    let _ = writeln!(merged, "  concentrate=true;");
    let _ = writeln!(merged);
    let _ = writeln!(
        merged,
        r##"  node [shape=box, style="rounded,filled", fillcolor="#f0f0f0", fontname="Helvetica"];"##
    );
    let _ = writeln!(merged, r##"  edge [color="#888888", arrowsize=0.7];"##);
    let _ = writeln!(merged);
    let _ = writeln!(merged, "  labelloc=t;");
    let _ = writeln!(merged, "  fontsize=16;");
    let _ = writeln!(merged);

    for output in graphs {
        let lines: Vec<&str> = output.lines().collect();
        let mut in_content = false;
        for line in &lines {
            let trimmed = line.trim();
            if trimmed.starts_with("digraph")
                || trimmed.starts_with("rankdir")
                || trimmed.starts_with("ranksep")
                || trimmed.starts_with("nodesep")
                || trimmed.starts_with("splines")
                || trimmed.starts_with("concentrate")
                || trimmed.starts_with("node [")
                || trimmed.starts_with("edge [")
                || trimmed.starts_with("labelloc")
                || trimmed.starts_with("fontsize")
                || trimmed.is_empty()
            {
                in_content = true;
                continue;
            }
            if trimmed == "}" {
                continue;
            }
            if in_content {
                let _ = writeln!(merged, "{line}");
            }
        }
        let _ = writeln!(merged);
    }

    let _ = writeln!(merged, "}}");
    merged
}

fn render_block_graph(project: &ProjectGraph) -> String {
    let mut units: Vec<_> = project.units().iter().collect();
    if units.is_empty() {
        return "none\n".to_string();
    }

    units.sort_by_key(|unit| unit.unit_index());

    let mut sections = Vec::new();
    for unit_graph in units {
        let unit = project.context().compile_unit(unit_graph.unit_index());
        let mut buf = String::new();
        render_block_graph_node(unit_graph.root(), unit, 0, &mut buf);
        sections.push(buf.trim_end().to_string());
    }

    if sections.is_empty() {
        "none\n".to_string()
    } else {
        let mut joined = sections.join("\n\n");
        joined.push('\n');
        joined
    }
}

fn render_block_graph_node(
    block_id: BlockId,
    unit: CompileUnit<'_>,
    depth: usize,
    buf: &mut String,
) {
    let block = unit.block(block_id);
    let indent = "  ".repeat(depth);

    let label = block.to_string();
    let deps = block.dependency_labels(unit);

    let _ = write!(buf, "{indent}({label}");

    let children = block.children();
    if children.is_empty() && deps.is_empty() {
        buf.push(')');
        buf.push('\n');
        return;
    }

    buf.push('\n');
    for child_id in children {
        if unit.block(child_id).kind() == BlockKind::Call {
            continue;
        }
        render_block_graph_node(child_id, unit, depth + 1, buf);
    }
    let child_indent = "  ".repeat(depth + 1);
    for dep in deps {
        let _ = writeln!(buf, "{child_indent}({dep})");
    }
    buf.push_str(&indent);
    buf.push_str(")\n");
}

fn snapshot_symbols<'a>(cc: &'a CompileCtxt<'a>) -> Vec<SymbolSnapshot> {
    let symbols = cc.symbols();
    let interner = cc.interner();
    let mut rows = Vec::with_capacity(symbols.len());
    for symbol in symbols {
        let name_str = interner
            .try_resolve(symbol.name)
            .unwrap_or_else(|| "?".to_string());

        let type_of = symbol.type_of().and_then(|sym_id| {
            cc.try_symbol(sym_id).map(|type_sym| {
                let type_name = interner
                    .try_resolve(type_sym.name)
                    .unwrap_or_else(|| "?".to_string());
                let type_unit = type_sym.unit_index().unwrap_or_default();
                format!("u{}:{} ({})", type_unit, sym_id.0 as u32, type_name)
            })
        });

        let block_id = symbol.block_id().map(|bid| {
            format!(
                "u{}:{}",
                symbol.unit_index().unwrap_or_default(),
                bid.as_u32()
            )
        });

        rows.push(SymbolSnapshot {
            unit: symbol.unit_index().unwrap_or_default(),
            id: symbol.id().0 as u32,
            kind: format!("{:?}", symbol.kind()),
            name: name_str,
            is_global: symbol.is_global(),
            type_of,
            block_id,
        });
    }

    rows
}

fn snapshot_symbol_dependencies<'a>(_cc: &'a CompileCtxt<'a>) -> Vec<SymbolDependencySnapshot> {
    Vec::new()
}

fn snapshot_block_relations(project: &ProjectGraph) -> Vec<BlockRelationSnapshot> {
    use std::collections::BTreeMap;

    let cc = project.context();
    let related_map = cc.block_relations();
    let mut block_map: BTreeMap<BlockId, BlockRelationSnapshot> = BTreeMap::new();

    for block_id in related_map.blocks() {
        let Some(desc) = describe_block(block_id, cc) else {
            continue;
        };

        let label = format!("u{}:{}", desc.unit, block_id.as_u32());
        let relations = related_map.relations_from(block_id);
        let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for relation in relations.iter() {
            let rel_name = relation.relation.to_string();

            for target_id in &relation.targets {
                let target_label = if let Some(target_desc) = describe_block(*target_id, cc) {
                    format!("u{}:{}", target_desc.unit, target_id.as_u32())
                } else {
                    format!("?:{}", target_id.as_u32())
                };

                grouped
                    .entry(rel_name.clone())
                    .or_default()
                    .push(target_label);
            }
        }

        for targets in grouped.values_mut() {
            targets.sort();
        }

        block_map.insert(
            block_id,
            BlockRelationSnapshot {
                label,
                kind: desc.kind.clone(),
                name: desc.name.clone(),
                relations: grouped.into_iter().collect(),
            },
        );
    }

    block_map.into_values().collect()
}

fn render_block_reports(
    project: &ProjectGraph,
) -> (Vec<BlockSnapshot>, Vec<SymbolDependencySnapshot>) {
    use std::collections::BTreeMap;

    let mut units: BTreeMap<usize, Vec<BlockDescriptor>> = BTreeMap::new();

    for unit_graph in project.units() {
        let unit_index = unit_graph.unit_index();
        let mut entries = Vec::new();

        for entry in project.context().find_blocks_in_unit(unit_index) {
            let Some(mut desc) = describe_block(entry.block_id, project.context()) else {
                continue;
            };
            desc.kind = entry.kind.to_string();
            entries.push(desc);
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        if !entries.is_empty() {
            units.insert(unit_index, entries);
        }
    }

    let mut block_rows = Vec::new();
    let deps: Vec<SymbolDependencySnapshot> = Vec::new();

    for (_unit, blocks) in units {
        for block in blocks {
            let label = format!("u{}:{}", block.unit, block.id.as_u32());
            block_rows.push(BlockSnapshot {
                label,
                kind: block.kind.clone(),
                name: block.name.clone(),
            });
        }
    }

    block_rows.sort_by(|a, b| a.label.cmp(&b.label));
    (block_rows, deps)
}

#[derive(Clone)]
struct BlockDescriptor {
    name: String,
    kind: String,
    unit: usize,
    id: BlockId,
}

fn describe_block<'a>(block_id: BlockId, cc: &'a CompileCtxt<'a>) -> Option<BlockDescriptor> {
    let entry = cc.block_info(block_id)?;

    let name = entry
        .name
        .or_else(|| {
            cc.try_block(block_id)
                .and_then(|block| block.try_name().map(|name| name.to_string()))
        })
        .or_else(|| {
            cc.try_block(block_id)
                .and_then(|block| find_first_ident_name(cc, &block.base().node))
        })
        .or_else(|| {
            cc.find_symbol_by_block_id(block_id)
                .and_then(|sym| cc.interner().try_resolve(sym.name))
        })
        .unwrap_or_else(|| format!("block#{block_id}"));

    Some(BlockDescriptor {
        name,
        kind: entry.kind.to_string(),
        unit: entry.unit_index,
        id: block_id,
    })
}

fn find_first_ident_name<'a>(
    cc: &'a CompileCtxt<'a>,
    node: &llmcc_core::ir::HirNode<'a>,
) -> Option<String> {
    use llmcc_core::ir::HirKind;

    if node.is_kind(HirKind::Identifier)
        && let Some(ident) = node.as_ident()
    {
        return Some(ident.name.to_string());
    }

    for child_id in node.child_ids() {
        if let Some(child_node) = cc.try_hir_node(*child_id) {
            if child_node.is_kind(HirKind::Identifier)
                && let Some(ident) = child_node.as_ident()
            {
                return Some(ident.name.to_string());
            }
            if child_node.is_kind(HirKind::Internal)
                && let Some(name) = find_first_ident_name(cc, &child_node)
            {
                return Some(name);
            }
        }
    }
    None
}
