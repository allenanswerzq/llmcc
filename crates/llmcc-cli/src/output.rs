//! Output generation for DOT and agent-native reports.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde::Serialize;
use tracing::info;

use llmcc_collect::{RenderEdge, RenderNode, collect_edges, collect_nodes};
use llmcc_core::block::{BlockKind, BlockRelation};
use llmcc_core::graph::ProjectGraph;
use llmcc_core::pagerank::{PageRanker, RankedBlock};
use llmcc_core::{BlockId, Result};
use llmcc_dot::{RenderOptions, render_graph_with_options};

use crate::{LlmccOptions, OutputFormat};

#[derive(Serialize)]
pub struct AgentGraph {
    schema_version: u8,
    nodes: Vec<AgentNode>,
    edges: Vec<AgentEdge>,
    pagerank: Vec<AgentRank>,
}

#[derive(Clone, Serialize)]
pub struct AgentNode {
    id: String,
    unit_index: usize,
    block_id: u32,
    name: String,
    block_kind: String,
    sym_kind: Option<String>,
    location: Option<String>,
    file_path: Option<String>,
    line_start: Option<usize>,
    crate_name: Option<String>,
    module_path: Option<String>,
    is_exported: bool,
}

#[derive(Clone, Serialize)]
pub struct AgentEdge {
    from: String,
    to: String,
    relation: String,
    from_label: String,
    to_label: String,
}

#[derive(Clone, Serialize)]
pub struct AgentRank {
    rank: usize,
    node_id: String,
    score: f64,
    influence_score: f64,
    orchestration_score: f64,
}

/// Generate output for a project graph.
pub fn generate_output<'tcx>(
    opts: &LlmccOptions,
    pg: &'tcx ProjectGraph<'tcx>,
) -> Result<Option<String>> {
    if should_render_dot(opts) {
        return Ok(Some(render_dot(opts, pg)));
    }

    if opts.tests_for.is_some() {
        return Ok(Some(render_tests_for(opts)));
    }

    if opts.blast_radius {
        return render_blast_radius(opts, pg).map(Some);
    }

    if opts.package_deps {
        let graph = build_agent_graph(opts, pg);
        return Ok(Some(render_package_deps(&graph)));
    }

    if opts.agent_summary || opts.git_diff {
        let graph = build_agent_graph(opts, pg);
        return Ok(Some(render_markdown_summary(opts, pg, &graph)));
    }

    match effective_format(opts) {
        Some(OutputFormat::Json) => {
            let graph = build_agent_graph(opts, pg);
            Ok(Some(
                serde_json::to_string_pretty(&graph).map_err(|err| err.to_string())?,
            ))
        }
        Some(OutputFormat::Markdown) => {
            let graph = build_agent_graph(opts, pg);
            Ok(Some(render_markdown_summary(opts, pg, &graph)))
        }
        Some(OutputFormat::Text) => {
            if let Some(k) = opts.pagerank_top_k {
                Ok(Some(render_pagerank_table(opts, pg, k)))
            } else {
                Ok(Some(String::new()))
            }
        }
        Some(OutputFormat::Dot) | None => {
            if let Some(k) = opts.pagerank_top_k {
                Ok(Some(render_pagerank_table(opts, pg, k)))
            } else {
                Ok(None)
            }
        }
    }
}

fn effective_format(opts: &LlmccOptions) -> Option<OutputFormat> {
    if opts.pagerank_top_k.is_some() && opts.output_format.is_none() {
        return Some(OutputFormat::Text);
    }
    opts.output_format
}

fn should_render_dot(opts: &LlmccOptions) -> bool {
    opts.graph || opts.output_format == Some(OutputFormat::Dot)
}

fn render_dot<'tcx>(opts: &LlmccOptions, pg: &'tcx ProjectGraph<'tcx>) -> String {
    let render_start = Instant::now();
    let render_options = RenderOptions {
        show_orphan_nodes: false,
        pagerank_top_k: opts.pagerank_top_k,
        cluster_by_crate: opts.cluster_by_crate,
        short_labels: opts.short_labels,
        only_exported: opts.only_exported,
    };

    let result = render_graph_with_options(pg, opts.component_depth, &render_options);

    info!(
        "Graph rendering: {:.2}s",
        render_start.elapsed().as_secs_f64()
    );

    result
}

fn render_pagerank_table<'tcx>(
    opts: &LlmccOptions,
    pg: &'tcx ProjectGraph<'tcx>,
    k: usize,
) -> String {
    let mut ranked = ranked_display_blocks(opts, pg);
    ranked.truncate(k);

    let mut output = String::new();
    let _ = writeln!(
        output,
        "rank score influence orchestration kind symbol path"
    );
    for (idx, block) in ranked.iter().enumerate() {
        let _ = writeln!(
            output,
            "{} {:.6} {:.6} {:.6} {} {} {}",
            idx + 1,
            block.score,
            block.influence_score,
            block.orchestration_score,
            block.kind,
            block.name,
            block.file_path.as_deref().unwrap_or("")
        );
    }
    output
}

fn ranked_display_blocks<'tcx>(
    opts: &LlmccOptions,
    pg: &'tcx ProjectGraph<'tcx>,
) -> Vec<RankedBlock> {
    let nodes = filtered_render_nodes(opts, pg);
    let node_ids: HashSet<BlockId> = nodes.iter().map(|node| node.block_id).collect();
    let mut ranked: Vec<_> = PageRanker::new(pg)
        .rank()
        .blocks
        .into_iter()
        .filter(|block| node_ids.contains(&block.node.block_id))
        .collect();
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
    ranked
}

fn filtered_render_nodes<'tcx>(
    opts: &LlmccOptions,
    pg: &'tcx ProjectGraph<'tcx>,
) -> Vec<RenderNode> {
    let mut nodes = collect_nodes(pg);
    if opts.only_exported {
        nodes.retain(|node| node.is_exported);
    }
    nodes
}

fn build_agent_graph<'tcx>(opts: &LlmccOptions, pg: &'tcx ProjectGraph<'tcx>) -> AgentGraph {
    let nodes = filtered_render_nodes(opts, pg);
    let node_set: HashSet<BlockId> = nodes.iter().map(|node| node.block_id).collect();
    let edges = collect_edges(pg, &node_set);
    let node_ids: HashMap<BlockId, String> = nodes
        .iter()
        .map(|node| (node.block_id, node_id(node.unit_index, node.block_id)))
        .collect();

    let agent_nodes = nodes.iter().map(agent_node_from_render_node).collect();
    let agent_edges = edges
        .iter()
        .filter_map(|edge| agent_edge_from_render_edge(edge, &node_ids))
        .collect();

    let pagerank = opts
        .pagerank_top_k
        .map(|k| {
            let mut ranked = ranked_display_blocks(opts, pg);
            ranked.truncate(k);
            ranked
                .iter()
                .enumerate()
                .filter_map(|(idx, block)| {
                    let node_id = node_ids.get(&block.node.block_id)?;
                    Some(AgentRank {
                        rank: idx + 1,
                        node_id: node_id.clone(),
                        score: block.score,
                        influence_score: block.influence_score,
                        orchestration_score: block.orchestration_score,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    AgentGraph {
        schema_version: 1,
        nodes: agent_nodes,
        edges: agent_edges,
        pagerank,
    }
}

fn node_id(unit_index: usize, block_id: BlockId) -> String {
    format!("u{unit_index}:b{}", block_id.as_u32())
}

fn agent_node_from_render_node(node: &RenderNode) -> AgentNode {
    AgentNode {
        id: node_id(node.unit_index, node.block_id),
        unit_index: node.unit_index,
        block_id: node.block_id.as_u32(),
        name: node.name.clone(),
        block_kind: node.block_kind.to_string(),
        sym_kind: node.sym_kind.map(|kind| format!("{kind:?}")),
        location: node.location.clone(),
        file_path: node.file_path.clone(),
        line_start: node.line_start,
        crate_name: node.crate_name.clone(),
        module_path: node.module_path.clone(),
        is_exported: node.is_exported,
    }
}

fn agent_edge_from_render_edge(
    edge: &RenderEdge,
    node_ids: &HashMap<BlockId, String>,
) -> Option<AgentEdge> {
    Some(AgentEdge {
        from: node_ids.get(&edge.from_id)?.clone(),
        to: node_ids.get(&edge.to_id)?.clone(),
        relation: format!("{}->{}", edge.from_label, edge.to_label),
        from_label: edge.from_label.to_string(),
        to_label: edge.to_label.to_string(),
    })
}

fn render_package_deps(graph: &AgentGraph) -> String {
    let nodes_by_id: HashMap<&str, &AgentNode> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();

    for edge in &graph.edges {
        let Some(from) = nodes_by_id.get(edge.from.as_str()) else {
            continue;
        };
        let Some(to) = nodes_by_id.get(edge.to.as_str()) else {
            continue;
        };
        let source = package_key(from);
        let target = package_key(to);
        if source != target {
            *counts.entry((source, target)).or_insert(0) += 1;
        }
    }

    let mut output = String::new();
    let _ = writeln!(output, "source target edges");
    for ((source, target), count) in counts {
        let _ = writeln!(output, "{source} {target} {count}");
    }
    output
}

fn package_key(node: &AgentNode) -> String {
    node.crate_name
        .clone()
        .or_else(|| node.module_path.clone())
        .or_else(|| {
            node.file_path
                .as_ref()
                .and_then(|path| Path::new(path).parent())
                .map(|path| path.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn render_markdown_summary<'tcx>(
    opts: &LlmccOptions,
    pg: &'tcx ProjectGraph<'tcx>,
    graph: &AgentGraph,
) -> String {
    let mut output = String::new();

    if opts.git_diff {
        render_changed_files_section(opts, pg, graph, &mut output);
    }

    output.push_str("## Top Symbols\n");
    if graph.pagerank.is_empty() {
        output.push_str("- No PageRank rows requested.\n");
    } else {
        let nodes_by_id: HashMap<&str, &AgentNode> = graph
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        for rank in &graph.pagerank {
            if let Some(node) = nodes_by_id.get(rank.node_id.as_str()) {
                let _ = writeln!(
                    output,
                    "- {:.6} {} {} {}",
                    rank.score,
                    node.block_kind,
                    node.name,
                    node.file_path.as_deref().unwrap_or("")
                );
            }
        }
    }

    output.push_str("\n## Public API Surface\n");
    let exported: Vec<_> = graph.nodes.iter().filter(|node| node.is_exported).collect();
    if exported.is_empty() && !graph.nodes.is_empty() {
        output.push_str("- No exported symbols were reported for this language or fixture.\n");
    } else {
        for node in exported {
            let _ = writeln!(
                output,
                "- {} {} {}",
                node.block_kind,
                node.name,
                node.file_path.as_deref().unwrap_or("")
            );
        }
    }

    output.push_str("\n## Caller Callee Clusters\n");
    let mut degree: BTreeMap<String, usize> = BTreeMap::new();
    for edge in graph
        .edges
        .iter()
        .filter(|edge| edge.relation == "caller->callee")
    {
        *degree.entry(edge.from.clone()).or_insert(0) += 1;
        *degree.entry(edge.to.clone()).or_insert(0) += 1;
    }
    render_degree_rows(&mut output, graph, degree);

    output.push_str("\n## Cross File Coupling\n");
    let cross_file = cross_file_counts(graph);
    if cross_file.is_empty() {
        output.push_str("- No cross-file edges.\n");
    } else {
        for (path, count) in cross_file.iter().take(10) {
            let _ = writeln!(output, "- {path} {count}");
        }
    }

    output.push_str("\n## Likely Refactor Entry Points\n");
    let refactor_points = refactor_entry_points(graph);
    if refactor_points.is_empty() {
        output.push_str("- No refactor entry points available.\n");
    } else {
        for (score, node) in refactor_points.into_iter().take(10) {
            let _ = writeln!(
                output,
                "- {:.6} {} {}",
                score,
                node.name,
                node.file_path.as_deref().unwrap_or("")
            );
        }
    }

    output.push_str("\n## Inferred Tests\n");
    let files: BTreeSet<_> = graph
        .nodes
        .iter()
        .filter_map(|node| node.file_path.clone())
        .collect();
    let tests = infer_tests_for_files(opts, files.iter().map(String::as_str));
    render_test_rows(&mut output, &tests);

    output
}

fn render_degree_rows(output: &mut String, graph: &AgentGraph, degree: BTreeMap<String, usize>) {
    let nodes_by_id: HashMap<&str, &AgentNode> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut rows: Vec<_> = degree.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if rows.is_empty() {
        output.push_str("- No caller/callee edges.\n");
        return;
    }
    for (id, count) in rows.into_iter().take(10) {
        if let Some(node) = nodes_by_id.get(id.as_str()) {
            let _ = writeln!(output, "- {} {} {}", node.name, node.block_kind, count);
        }
    }
}

fn cross_file_counts(graph: &AgentGraph) -> BTreeMap<String, usize> {
    let nodes_by_id: HashMap<&str, &AgentNode> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut counts = BTreeMap::new();
    for edge in &graph.edges {
        let Some(from) = nodes_by_id.get(edge.from.as_str()) else {
            continue;
        };
        let Some(to) = nodes_by_id.get(edge.to.as_str()) else {
            continue;
        };
        let Some(from_path) = from.file_path.as_ref() else {
            continue;
        };
        let Some(to_path) = to.file_path.as_ref() else {
            continue;
        };
        if from_path != to_path {
            *counts.entry(from_path.clone()).or_insert(0) += 1;
            *counts.entry(to_path.clone()).or_insert(0) += 1;
        }
    }
    counts
}

fn refactor_entry_points(graph: &AgentGraph) -> Vec<(f64, &AgentNode)> {
    let cross_file = cross_file_counts(graph);
    let max_rank = graph
        .pagerank
        .iter()
        .map(|rank| rank.score)
        .fold(0.0_f64, f64::max);
    let max_cross = cross_file.values().copied().max().unwrap_or(0) as f64;
    let ranks: HashMap<&str, f64> = graph
        .pagerank
        .iter()
        .map(|rank| (rank.node_id.as_str(), rank.score))
        .collect();

    let mut rows = Vec::new();
    for node in &graph.nodes {
        let rank_norm = ranks.get(node.id.as_str()).copied().unwrap_or(0.0)
            / if max_rank <= f64::EPSILON {
                1.0
            } else {
                max_rank
            };
        let cross_norm = node
            .file_path
            .as_ref()
            .and_then(|path| cross_file.get(path))
            .copied()
            .unwrap_or(0) as f64
            / if max_cross <= f64::EPSILON {
                1.0
            } else {
                max_cross
            };
        // Refactor score blends normalized PageRank and normalized cross-file degree equally.
        rows.push(((rank_norm + cross_norm) / 2.0, node));
    }
    rows.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.name.cmp(&b.1.name))
    });
    rows
}

fn render_blast_radius<'tcx>(opts: &LlmccOptions, pg: &'tcx ProjectGraph<'tcx>) -> Result<String> {
    let symbol = opts
        .symbol
        .as_deref()
        .ok_or_else(|| "--blast-radius requires --symbol".to_string())?;
    let target = resolve_symbol(pg, symbol)?;
    let direct_callers = related_names(pg, target, BlockRelation::CalledBy);
    let callees = related_names(pg, target, BlockRelation::Calls);
    let dependent_types = related_names(pg, target, BlockRelation::UsedBy);
    let transitive_callers = transitive_related_names(pg, target, BlockRelation::CalledBy);
    let affected_files = affected_files(pg, target);
    let tests = infer_tests_for_files(opts, affected_files.iter().map(String::as_str));

    let mut output = String::new();
    output.push_str("## Direct Callers\n");
    render_string_rows(&mut output, &direct_callers);
    output.push_str("\n## Transitive Callers\n");
    render_string_rows(&mut output, &transitive_callers);
    output.push_str("\n## Callees\n");
    render_string_rows(&mut output, &callees);
    output.push_str("\n## Dependent Types\n");
    render_string_rows(&mut output, &dependent_types);
    output.push_str("\n## Affected Files\n");
    render_string_rows(&mut output, &affected_files);
    output.push_str("\n## Inferred Tests\n");
    render_test_rows(&mut output, &tests);
    Ok(output)
}

fn resolve_symbol<'tcx>(pg: &'tcx ProjectGraph<'tcx>, name: &str) -> Result<BlockId> {
    let matches: Vec<_> = pg
        .cc
        .get_all_blocks()
        .into_iter()
        .filter(|(_, _, block_name, kind)| {
            block_name.as_deref() == Some(name)
                && matches!(
                    kind,
                    BlockKind::Func
                        | BlockKind::Method
                        | BlockKind::Class
                        | BlockKind::Trait
                        | BlockKind::Interface
                        | BlockKind::Enum
                )
        })
        .collect();
    match matches.as_slice() {
        [(block_id, ..)] => Ok(*block_id),
        [] => Err(format!("symbol not found: {name}").into()),
        _ => Err(format!("ambiguous symbol: {name}").into()),
    }
}

fn related_names<'tcx>(
    pg: &'tcx ProjectGraph<'tcx>,
    block_id: BlockId,
    relation: BlockRelation,
) -> BTreeSet<String> {
    pg.cc
        .related_map
        .get_related(block_id, relation)
        .into_iter()
        .filter_map(|id| block_display_name(pg, id))
        .collect()
}

fn transitive_related_names<'tcx>(
    pg: &'tcx ProjectGraph<'tcx>,
    block_id: BlockId,
    relation: BlockRelation,
) -> BTreeSet<String> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([block_id]);
    let mut names = BTreeSet::new();
    while let Some(current) = queue.pop_front() {
        for next in pg.cc.related_map.get_related(current, relation) {
            if seen.insert(next) {
                if let Some(name) = block_display_name(pg, next) {
                    names.insert(name);
                }
                queue.push_back(next);
            }
        }
    }
    names
}

fn affected_files<'tcx>(pg: &'tcx ProjectGraph<'tcx>, block_id: BlockId) -> BTreeSet<String> {
    let mut files = BTreeSet::new();
    if let Some(path) = block_file_path(pg, block_id) {
        files.insert(path);
    }
    for relation in [
        BlockRelation::CalledBy,
        BlockRelation::Calls,
        BlockRelation::UsedBy,
        BlockRelation::Uses,
        BlockRelation::TypeFor,
        BlockRelation::TypeOf,
    ] {
        for related in pg.cc.related_map.get_related(block_id, relation) {
            if let Some(path) = block_file_path(pg, related) {
                files.insert(path);
            }
        }
    }
    files
}

fn block_display_name<'tcx>(pg: &'tcx ProjectGraph<'tcx>, block_id: BlockId) -> Option<String> {
    let (_, name, kind) = pg.cc.get_block_info(block_id)?;
    Some(name.unwrap_or_else(|| format!("{kind}:{}", block_id.as_u32())))
}

fn block_file_path<'tcx>(pg: &'tcx ProjectGraph<'tcx>, block_id: BlockId) -> Option<String> {
    let (unit_index, _, _) = pg.cc.get_block_info(block_id)?;
    pg.cc.file_path(unit_index).map(ToString::to_string)
}

fn render_tests_for(opts: &LlmccOptions) -> String {
    let tests_for = opts.tests_for.as_deref().into_iter();
    let tests = infer_tests_for_files(opts, tests_for);
    let mut output = String::new();
    render_test_rows(&mut output, &tests);
    output
}

fn infer_tests_for_files<'a>(
    opts: &LlmccOptions,
    files: impl Iterator<Item = &'a str>,
) -> BTreeSet<String> {
    let roots = input_roots(opts);
    let all_tests = roots
        .iter()
        .flat_map(|root| collect_test_files(root))
        .collect::<BTreeSet<_>>();
    let mut inferred = BTreeSet::new();
    for file in files {
        let file_path = Path::new(file);
        let file_name = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        for test in &all_tests {
            let normalized = normalize_path_for_output(opts, test);
            if normalized.contains(file_name)
                || normalized.contains("tests/")
                || normalized.contains("__tests__/")
            {
                inferred.insert(normalized);
            }
        }
    }
    inferred
}

fn collect_test_files(root: &Path) -> BTreeSet<PathBuf> {
    let mut tests = BTreeSet::new();
    collect_test_files_recursive(root, &mut tests);
    tests
}

fn collect_test_files_recursive(path: &Path, tests: &mut BTreeSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_test_files_recursive(&path, tests);
        } else {
            let text = path.to_string_lossy();
            if text.contains("/tests/")
                || text.ends_with("_test.go")
                || text.ends_with(".test.ts")
                || text.ends_with(".spec.ts")
                || text.contains("/__tests__/")
                || has_rust_cfg_test(&path)
            {
                tests.insert(path);
            }
        }
    }
}

fn has_rust_cfg_test(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("rs")
        && std::fs::read_to_string(path)
            .map(|content| content.contains("#[cfg(test)]"))
            .unwrap_or(false)
}

fn input_roots(opts: &LlmccOptions) -> Vec<PathBuf> {
    if !opts.dirs.is_empty() {
        return opts.dirs.iter().map(PathBuf::from).collect();
    }
    opts.files
        .iter()
        .filter_map(|file| Path::new(file).parent().map(Path::to_path_buf))
        .collect()
}

fn normalize_path_for_output(opts: &LlmccOptions, path: &Path) -> String {
    for root in input_roots(opts) {
        if let Ok(stripped) = path.strip_prefix(&root) {
            return stripped.to_string_lossy().to_string();
        }
    }
    path.to_string_lossy().to_string()
}

fn render_string_rows(output: &mut String, rows: &BTreeSet<String>) {
    if rows.is_empty() {
        output.push_str("- None\n");
        return;
    }
    for row in rows {
        let _ = writeln!(output, "- {row}");
    }
}

fn render_test_rows(output: &mut String, tests: &BTreeSet<String>) {
    if tests.is_empty() {
        output.push_str("- None\n");
        return;
    }
    for test in tests {
        let _ = writeln!(output, "{test}");
    }
}

fn render_changed_files_section<'tcx>(
    opts: &LlmccOptions,
    pg: &'tcx ProjectGraph<'tcx>,
    graph: &AgentGraph,
    output: &mut String,
) {
    let changed = git_changed_files(opts);
    output.push_str("## Changed Files\n");
    if changed.is_empty() {
        output.push_str("- No changed files from git diff.\n\n");
        return;
    }

    let ranks: HashMap<&str, f64> = graph
        .pagerank
        .iter()
        .map(|rank| (rank.node_id.as_str(), rank.score))
        .collect();

    for changed_file in changed {
        let score_total: f64 = graph
            .nodes
            .iter()
            .filter(|node| path_matches_changed(node.file_path.as_deref(), &changed_file))
            .map(|node| ranks.get(node.id.as_str()).copied().unwrap_or(0.0))
            .sum();
        let tests = infer_tests_for_files(opts, std::iter::once(changed_file.as_str()));
        let _ = writeln!(output, "- {changed_file} pagerank_total={score_total:.6}");
        let mut related = BTreeSet::new();
        for node in graph
            .nodes
            .iter()
            .filter(|node| path_matches_changed(node.file_path.as_deref(), &changed_file))
        {
            related.extend(related_names(
                pg,
                BlockId::new(node.block_id),
                BlockRelation::CalledBy,
            ));
            related.extend(related_names(
                pg,
                BlockId::new(node.block_id),
                BlockRelation::Calls,
            ));
        }
        render_string_rows(output, &related);
        render_test_rows(output, &tests);
    }
    output.push('\n');
}

fn git_changed_files(opts: &LlmccOptions) -> BTreeSet<String> {
    let Some(root) = input_roots(opts).into_iter().next() else {
        return BTreeSet::new();
    };
    let Ok(output) = Command::new("git")
        .args(["diff", "--name-only"])
        .current_dir(root)
        .output()
    else {
        return BTreeSet::new();
    };
    if !output.status.success() {
        return BTreeSet::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn path_matches_changed(path: Option<&str>, changed: &str) -> bool {
    let Some(path) = path else {
        return false;
    };
    path == changed || path.ends_with(changed)
}

/// Merge multiple DOT graph outputs into a single graph.
pub fn merge_dot_outputs(outputs: &[String]) -> String {
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

    for output in outputs {
        let mut in_content = false;
        for line in output.lines() {
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
                let _ = writeln!(merged, "{}", line);
            }
        }
        let _ = writeln!(merged);
    }

    let _ = writeln!(merged, "}}");
    merged
}
