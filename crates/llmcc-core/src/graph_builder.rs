use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::marker::PhantomData;

pub use crate::block::{BasicBlock, BlockId, BlockKind, BlockRelation};
use crate::block::{
    BlockCall, BlockClass, BlockConst, BlockEnum, BlockField, BlockFunc, BlockImpl, BlockRoot,
    BlockStmt,
};
use crate::block_rel::BlockRelationMap;
use crate::context::{CompileCtxt, CompileUnit};
use crate::ir::HirNode;
use crate::lang_def::LanguageTrait;
use crate::pagerank::{PageRankDirection, PageRanker};
use crate::symbol::{SymId, Symbol};
use crate::visit::HirVisitor;

#[derive(Debug, Clone)]
pub struct UnitGraph {
    /// Compile unit this graph belongs to
    unit_index: usize,
    /// Root block ID of this unit
    root: BlockId,
    /// Edges of this graph unit
    edges: BlockRelationMap,
}

impl UnitGraph {
    pub fn new(unit_index: usize, root: BlockId, edges: BlockRelationMap) -> Self {
        Self {
            unit_index,
            root,
            edges,
        }
    }

    pub fn unit_index(&self) -> usize {
        self.unit_index
    }

    pub fn root(&self) -> BlockId {
        self.root
    }

    pub fn edges(&self) -> &BlockRelationMap {
        &self.edges
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GraphBuildConfig {
    pub compact: bool,
}

impl GraphBuildConfig {
    pub fn compact() -> Self {
        Self { compact: true }
    }
}

impl Default for GraphBuildConfig {
    fn default() -> Self {
        Self { compact: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphNode {
    pub unit_index: usize,
    pub block_id: BlockId,
}

/// ProjectGraph represents a complete compilation project with all units and their inter-dependencies.
///
/// # Overview
/// ProjectGraph maintains a collection of per-unit compilation graphs (UnitGraph) and facilitates
/// cross-unit dependency resolution. It provides efficient multi-dimensional indexing for block
/// lookups by name, kind, unit, and ID, enabling quick context retrieval for LLM consumption.
///
/// # Architecture
/// The graph consists of:
/// - **UnitGraphs**: One per compilation unit (file), containing blocks and intra-unit relations
/// - **Block Indexes**: Multi-dimensional indexes via BlockIndexMaps for O(1) to O(log n) lookups
/// - **Cross-unit Links**: Dependencies tracked between blocks across different units
///
/// # Primary Use Cases
/// 1. **Symbol Resolution**: Find blocks by name across the entire project
/// 2. **Context Gathering**: Collect all related blocks for code analysis
/// 3. **LLM Serialization**: Export graph as text or JSON for LLM model consumption
/// 4. **Dependency Analysis**: Traverse dependency graphs to understand block relationships
///
#[derive(Debug)]
pub struct ProjectGraph<'tcx> {
    /// Reference to the compilation context containing all symbols, HIR nodes, and blocks
    pub cc: &'tcx CompileCtxt<'tcx>,
    /// Per-unit graphs containing blocks and intra-unit relations
    units: Vec<UnitGraph>,
    compact_rank_limit: Option<usize>,
    pagerank_direction: PageRankDirection,
}

impl<'tcx> ProjectGraph<'tcx> {
    pub fn new(cc: &'tcx CompileCtxt<'tcx>) -> Self {
        Self {
            cc,
            units: Vec::new(),
            compact_rank_limit: None,
            pagerank_direction: PageRankDirection::default(),
        }
    }

    pub fn add_child(&mut self, graph: UnitGraph) {
        self.units.push(graph);
    }

    /// Configure the number of PageRank-filtered nodes retained when rendering compact graphs.
    pub fn set_compact_rank_limit(&mut self, limit: Option<usize>) {
        self.compact_rank_limit = match limit {
            Some(0) => None,
            other => other,
        };
    }

    /// Configure the PageRank direction for ranking nodes.
    pub fn set_pagerank_direction(&mut self, direction: PageRankDirection) {
        self.pagerank_direction = direction;
    }

    pub fn link_units(&mut self) {
        if self.units.is_empty() {
            return;
        }

        let mut unresolved = self.cc.unresolve_symbols.borrow_mut();

        unresolved.retain(|symbol_ref| {
            let target = *symbol_ref;
            let Some(target_block) = target.block_id() else {
                return false;
            };

            let dependents: Vec<SymId> = target.depended.borrow().clone();
            for dependent_id in dependents {
                let Some(source_symbol) = self.cc.opt_get_symbol(dependent_id) else {
                    continue;
                };
                let Some(from_block) = source_symbol.block_id() else {
                    continue;
                };
                self.add_cross_edge(
                    source_symbol.unit_index().unwrap(),
                    target.unit_index().unwrap(),
                    from_block,
                    target_block,
                );
            }

            false
        });
    }

    pub fn units(&self) -> &[UnitGraph] {
        &self.units
    }

    pub fn unit_graph(&self, unit_index: usize) -> Option<&UnitGraph> {
        self.units
            .iter()
            .find(|unit| unit.unit_index() == unit_index)
    }

    pub fn block_by_name(&self, name: &str) -> Option<GraphNode> {
        let block_indexes = self.cc.block_indexes.borrow();
        let matches = block_indexes.find_by_name(name);

        matches.first().map(|(unit_index, _, block_id)| GraphNode {
            unit_index: *unit_index,
            block_id: *block_id,
        })
    }

    pub fn blocks_by_name(&self, name: &str) -> Vec<GraphNode> {
        let block_indexes = self.cc.block_indexes.borrow();
        let matches = block_indexes.find_by_name(name);

        matches
            .into_iter()
            .map(|(unit_index, _, block_id)| GraphNode {
                unit_index,
                block_id,
            })
            .collect()
    }

    pub fn render_compact_graph(&self) -> String {
        self.render_compact_graph_inner(self.compact_rank_limit)
    }

    pub fn block_by_name_in(&self, unit_index: usize, name: &str) -> Option<GraphNode> {
        let block_indexes = self.cc.block_indexes.borrow();
        let matches = block_indexes.find_by_name(name);

        matches
            .iter()
            .find(|(u, _, _)| *u == unit_index)
            .map(|(_, _, block_id)| GraphNode {
                unit_index,
                block_id: *block_id,
            })
    }

    pub fn blocks_by_kind(&self, block_kind: BlockKind) -> Vec<GraphNode> {
        let block_indexes = self.cc.block_indexes.borrow();
        let matches = block_indexes.find_by_kind(block_kind);

        matches
            .into_iter()
            .map(|(unit_index, _, block_id)| GraphNode {
                unit_index,
                block_id,
            })
            .collect()
    }

    pub fn blocks_by_kind_in(&self, block_kind: BlockKind, unit_index: usize) -> Vec<GraphNode> {
        let block_indexes = self.cc.block_indexes.borrow();
        let block_ids = block_indexes.find_by_kind_and_unit(block_kind, unit_index);

        block_ids
            .into_iter()
            .map(|block_id| GraphNode {
                unit_index,
                block_id,
            })
            .collect()
    }

    pub fn blocks_in(&self, unit_index: usize) -> Vec<GraphNode> {
        let block_indexes = self.cc.block_indexes.borrow();
        let matches = block_indexes.find_by_unit(unit_index);

        matches
            .into_iter()
            .map(|(_, _, block_id)| GraphNode {
                unit_index,
                block_id,
            })
            .collect()
    }

    pub fn block_info(&self, block_id: BlockId) -> Option<(usize, Option<String>, BlockKind)> {
        let block_indexes = self.cc.block_indexes.borrow();
        block_indexes.get_block_info(block_id)
    }

    pub fn find_related_blocks(
        &self,
        node: GraphNode,
        relations: Vec<BlockRelation>,
    ) -> Vec<GraphNode> {
        if node.unit_index >= self.units.len() {
            return Vec::new();
        }

        let unit = &self.units[node.unit_index];
        let mut result = Vec::new();

        for relation in relations {
            match relation {
                BlockRelation::DependsOn => {
                    // Get all blocks that this block depends on
                    let dependencies = unit
                        .edges
                        .get_related(node.block_id, BlockRelation::DependsOn);
                    for dep_block_id in dependencies {
                        result.push(GraphNode {
                            unit_index: node.unit_index,
                            block_id: dep_block_id,
                        });
                    }
                }
                BlockRelation::DependedBy => {
                    let mut seen = HashSet::new();

                    // Direct dependents tracked on this unit (covers cross-unit edges too)
                    let dependents = unit
                        .edges
                        .get_related(node.block_id, BlockRelation::DependedBy);
                    if !dependents.is_empty() {
                        let indexes = self.cc.block_indexes.borrow();
                        for dep_block_id in dependents {
                            if !seen.insert(dep_block_id) {
                                continue;
                            }
                            if let Some((dep_unit_idx, _, _)) = indexes.get_block_info(dep_block_id)
                            {
                                result.push(GraphNode {
                                    unit_index: dep_unit_idx,
                                    block_id: dep_block_id,
                                });
                            } else {
                                result.push(GraphNode {
                                    unit_index: node.unit_index,
                                    block_id: dep_block_id,
                                });
                            }
                        }
                    }

                    // Fallback: scan current unit for reverse DependsOn edges
                    let local_dependents = unit
                        .edges
                        .find_reverse_relations(node.block_id, BlockRelation::DependsOn);
                    for dep_block_id in local_dependents {
                        if !seen.insert(dep_block_id) {
                            continue;
                        }
                        result.push(GraphNode {
                            unit_index: node.unit_index,
                            block_id: dep_block_id,
                        });
                    }
                }
                BlockRelation::Unknown => {
                    // Skip unknown relations
                }
            }
        }

        result
    }

    pub fn find_dpends_blocks_recursive(&self, node: GraphNode) -> HashSet<GraphNode> {
        let mut visited = HashSet::new();
        let mut stack = vec![node];
        let relations = vec![BlockRelation::DependsOn];

        while let Some(current) = stack.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            for related in self.find_related_blocks(current, relations.clone()) {
                if !visited.contains(&related) {
                    stack.push(related);
                }
            }
        }

        visited.remove(&node);
        visited
    }

    pub fn traverse_bfs<F>(&self, start: GraphNode, mut callback: F)
    where
        F: FnMut(GraphNode),
    {
        let mut visited = HashSet::new();
        let mut queue = vec![start];
        let relations = vec![BlockRelation::DependsOn, BlockRelation::DependedBy];

        while !queue.is_empty() {
            let current = queue.remove(0);
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);
            callback(current);

            for related in self.find_related_blocks(current, relations.clone()) {
                if !visited.contains(&related) {
                    queue.push(related);
                }
            }
        }
    }

    pub fn traverse_dfs<F>(&self, start: GraphNode, mut callback: F)
    where
        F: FnMut(GraphNode),
    {
        let mut visited = HashSet::new();
        self.traverse_dfs_impl(start, &mut visited, &mut callback);
    }

    fn traverse_dfs_impl<F>(
        &self,
        node: GraphNode,
        visited: &mut HashSet<GraphNode>,
        callback: &mut F,
    ) where
        F: FnMut(GraphNode),
    {
        if visited.contains(&node) {
            return;
        }
        visited.insert(node);
        callback(node);

        let relations = vec![BlockRelation::DependsOn, BlockRelation::DependedBy];
        for related in self.find_related_blocks(node, relations) {
            if !visited.contains(&related) {
                self.traverse_dfs_impl(related, visited, callback);
            }
        }
    }

    pub fn get_block_depends(&self, node: GraphNode) -> HashSet<GraphNode> {
        if node.unit_index >= self.units.len() {
            return HashSet::new();
        }

        let unit = &self.units[node.unit_index];
        let mut result = HashSet::new();
        let mut visited = HashSet::new();
        let mut stack = vec![node.block_id];

        while let Some(current_block) = stack.pop() {
            if visited.contains(&current_block) {
                continue;
            }
            visited.insert(current_block);

            let dependencies = unit
                .edges
                .get_related(current_block, BlockRelation::DependsOn);
            for dep_block_id in dependencies {
                if dep_block_id != node.block_id {
                    result.insert(GraphNode {
                        unit_index: node.unit_index,
                        block_id: dep_block_id,
                    });
                    stack.push(dep_block_id);
                }
            }
        }

        result
    }

    pub fn get_block_depended(&self, node: GraphNode) -> HashSet<GraphNode> {
        if node.unit_index >= self.units.len() {
            return HashSet::new();
        }

        let unit = &self.units[node.unit_index];
        let mut result = HashSet::new();
        let mut visited = HashSet::new();
        let mut stack = vec![node.block_id];

        while let Some(current_block) = stack.pop() {
            if visited.contains(&current_block) {
                continue;
            }
            visited.insert(current_block);

            let dependencies = unit
                .edges
                .get_related(current_block, BlockRelation::DependedBy);
            for dep_block_id in dependencies {
                if dep_block_id != node.block_id {
                    result.insert(GraphNode {
                        unit_index: node.unit_index,
                        block_id: dep_block_id,
                    });
                    stack.push(dep_block_id);
                }
            }
        }

        result
    }

    fn render_compact_graph_inner(&self, top_k: Option<usize>) -> String {
        #[derive(Clone)]
        struct CompactNode {
            block_id: BlockId,
            unit_index: usize,
            // kind: BlockKind,
            name: String,
            location: Option<String>,
        }

        fn byte_to_line(content: &[u8], byte_pos: usize) -> usize {
            let clamped = byte_pos.min(content.len());
            let mut line = 1;
            for &ch in &content[..clamped] {
                if ch == b'\n' {
                    line += 1;
                }
            }
            line
        }

        fn escape_label(input: &str) -> String {
            input
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
        }

        fn escape_attr(input: &str) -> String {
            input.replace('\\', "\\\\").replace('"', "\\\"")
        }

        fn summarize_location(location: &str) -> (String, String) {
            let (path_part, line_part) = location
                .rsplit_once(':')
                .map(|(path, line)| (path, Some(line)))
                .unwrap_or((location, None));

            let path = Path::new(path_part);
            let components: Vec<_> = path
                .components()
                .filter_map(|comp| comp.as_os_str().to_str())
                .collect();

            let start = components
                .len()
                .saturating_sub(3);
            let mut shortened = components[start..].join("/");
            if shortened.is_empty() {
                shortened = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path_part)
                    .to_string();
            }

            let display = if let Some(line) = line_part {
                format!("{shortened}:{line}")
            } else {
                shortened
            };

            (display, location.to_string())
        }

        fn extract_crate_path(location: &str) -> String {
            // Extract crate path from file location so all nodes from the same crate cluster together.
            // Examples:
            //   /path/to/crate/src/module/file.rs -> crate
            //   C:\path\to\crate\src\module\file.rs -> crate

            let path = location.split(':').next().unwrap_or(location);
            let parts: Vec<&str> = path.split(['/', '\\']).collect();

            // Find the "src" directory index
            if let Some(src_idx) = parts.iter().position(|&p| p == "src") {
                if src_idx > 0 {
                    // The directory before `src` is the crate root.
                    return parts[src_idx - 1].to_string();
                }
            }

            // Fallback: try to extract just the filename without extension
            if let Some(filename) = parts.last() {
                if !filename.is_empty() {
                    return filename.split('.').next().unwrap_or("unknown").to_string();
                }
            }

            "unknown".to_string()
        }

        fn extract_python_module_path(location: &str) -> String {
            const MAX_MODULE_DEPTH: usize = 2;

            let path_str = location.split(':').next().unwrap_or(location);
            let path = Path::new(path_str);

            // If the path is not a Python file, fall back to generic handling.
            if path.extension().and_then(|ext| ext.to_str()) != Some("py") {
                return extract_crate_path(location);
            }

            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());

            let mut packages: Vec<String> = Vec::new();
            let mut current = path.parent();

            while let Some(dir) = current {
                let dir_name = match dir.file_name().and_then(|n| n.to_str()) {
                    Some(name) if !name.is_empty() => name.to_string(),
                    _ => break,
                };

                let has_init = dir.join("__init__.py").exists() || dir.join("__init__.pyi").exists();

                if has_init {
                    packages.push(dir_name);
                }

                current = dir.parent();
            }

            if packages.is_empty() {
                if let Some(stem) = file_stem.as_ref().filter(|stem| stem.as_str() != "__init__") {
                    return stem.clone();
                }

                if let Some(parent_name) = path
                    .parent()
                    .and_then(|dir| dir.file_name().and_then(|n| n.to_str()))
                    .map(|s| s.to_string())
                {
                    return parent_name;
                }

                return "unknown".to_string();
            }

            packages.reverse();
            if packages.len() > MAX_MODULE_DEPTH {
                packages.truncate(MAX_MODULE_DEPTH);
            }

            packages.join(".")
        }

        fn extract_group_path(location: &str) -> String {
            let path = location.split(':').next().unwrap_or(location);
            if path.ends_with(".py") {
                extract_python_module_path(location)
            } else {
                extract_crate_path(location)
            }
        }

        fn strongly_connected_components(adjacency: &[Vec<usize>]) -> Vec<Vec<usize>> {
            fn strongconnect(
                v: usize,
                index: &mut usize,
                adjacency: &[Vec<usize>],
                indices: &mut [Option<usize>],
                lowlink: &mut [usize],
                stack: &mut Vec<usize>,
                on_stack: &mut [bool],
                components: &mut Vec<Vec<usize>>,
            ) {
                indices[v] = Some(*index);
                lowlink[v] = *index;
                *index += 1;
                stack.push(v);
                on_stack[v] = true;

                for &w in &adjacency[v] {
                    if indices[w].is_none() {
                        strongconnect(
                            w, index, adjacency, indices, lowlink, stack, on_stack, components,
                        );
                        lowlink[v] = lowlink[v].min(lowlink[w]);
                    } else if on_stack[w] {
                        let w_index = indices[w].unwrap();
                        lowlink[v] = lowlink[v].min(w_index);
                    }
                }

                if lowlink[v] == indices[v].unwrap() {
                    let mut component = Vec::new();
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        component.push(w);
                        if w == v {
                            break;
                        }
                    }
                    components.push(component);
                }
            }

            let mut index = 0;
            let mut stack = Vec::new();
            let mut indices = vec![None; adjacency.len()];
            let mut lowlink = vec![0; adjacency.len()];
            let mut on_stack = vec![false; adjacency.len()];
            let mut components = Vec::new();

            for v in 0..adjacency.len() {
                if indices[v].is_none() {
                    strongconnect(
                        v,
                        &mut index,
                        adjacency,
                        &mut indices,
                        &mut lowlink,
                        &mut stack,
                        &mut on_stack,
                        &mut components,
                    );
                }
            }

            components
        }

        let interesting_kinds = [BlockKind::Class, BlockKind::Enum];

        let ranked_order = top_k.and_then(|limit| {
            let mut ranker = PageRanker::new(self);
            // Apply configured PageRank direction
            ranker.config_mut().direction = self.pagerank_direction;
            let mut collected = Vec::new();

            for ranked in ranker.rank() {
                if interesting_kinds.contains(&ranked.kind) {
                    collected.push(ranked.node.block_id);
                }
                if collected.len() >= limit {
                    break;
                }
            }

            if collected.is_empty() {
                None
            } else {
                Some(collected)
            }
        });

        let ranked_filter: Option<HashSet<BlockId>> = ranked_order
            .as_ref()
            .map(|ordered| ordered.iter().copied().collect());

        let mut nodes: Vec<CompactNode> = {
            let block_indexes = self.cc.block_indexes.borrow();
            block_indexes
                .block_id_index
                .iter()
                .filter_map(|(&block_id, (unit_index, name_opt, kind))| {
                    if !interesting_kinds.contains(kind) {
                        return None;
                    }

                    if let Some(ref ranked_ids) = ranked_filter {
                        if !ranked_ids.contains(&block_id) {
                            return None;
                        }
                    }

                    let unit = self.cc.compile_unit(*unit_index);
                    let block = unit.bb(block_id);
                    let display_name = name_opt
                        .clone()
                        .or_else(|| {
                            block
                                .base()
                                .and_then(|base| base.opt_get_name().map(|s| s.to_string()))
                        })
                        .unwrap_or_else(|| format!("{}:{}", kind, block_id.as_u32()));

                    let path = std::fs::canonicalize(
                        unit.file_path()
                            .or_else(|| unit.file().path())
                            .unwrap_or("<unknown>"),
                    )
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| {
                        unit.file_path()
                            .or_else(|| unit.file().path())
                            .unwrap_or("<unknown>")
                            .to_string()
                    });
                    let location = block
                        .opt_node()
                        .and_then(|node| {
                            unit.file()
                                .file
                                .content
                                .as_ref()
                                .map(|bytes| byte_to_line(bytes.as_slice(), node.start_byte()))
                                .map(|line| format!("{path}:{line}"))
                        })
                        .or_else(|| Some(path.clone()));

                    Some(CompactNode {
                        block_id,
                        unit_index: *unit_index,
                        // kind: *kind,
                        name: display_name,
                        location,
                    })
                })
                .collect()
        };

        nodes.sort_by(|a, b| a.name.cmp(&b.name));

        if nodes.is_empty() {
            return "digraph CompactProject {\n}\n".to_string();
        }

        let mut node_index = HashMap::new();
        for (idx, node) in nodes.iter().enumerate() {
            node_index.insert(node.block_id, idx);
        }

        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
        for node in &nodes {
            let Some(unit_graph) = self.unit_graph(node.unit_index) else {
                continue;
            };
            let from_idx = node_index[&node.block_id];

            let dependencies = unit_graph
                .edges()
                .get_related(node.block_id, BlockRelation::DependsOn);
            let mut targets = dependencies
                .into_iter()
                .filter_map(|dep_id| node_index.get(&dep_id).copied())
                .collect::<Vec<_>>();

            targets.sort_unstable();
            targets.dedup();
            adjacency[from_idx] = targets;
        }

        let mut components: Vec<Vec<usize>> = strongly_connected_components(&adjacency)
            .into_iter()
            .filter(|component| !component.is_empty())
            .collect();

        if components.is_empty() {
            return "digraph CompactProject {\n}\n".to_string();
        }

        components.sort_by(|a, b| b.len().cmp(&a.len()));

        let target_limit = ranked_order
            .as_ref()
            .map(|order| order.len())
            .unwrap_or_else(|| nodes.len());

        let mut keep = vec![false; nodes.len()];
        let mut kept = 0usize;

        for component in components.iter().filter(|component| component.len() > 1) {
            for &idx in component {
                if !keep[idx] {
                    keep[idx] = true;
                    kept += 1;
                }
            }
            if kept >= target_limit {
                break;
            }
        }

        if kept < target_limit {
            if let Some(order) = ranked_order.as_ref() {
                for block_id in order {
                    if let Some(&idx) = node_index.get(block_id) {
                        if !keep[idx] {
                            keep[idx] = true;
                            kept += 1;
                            if kept >= target_limit {
                                break;
                            }
                        }
                    }
                }
            } else {
                for idx in 0..nodes.len() {
                    if !keep[idx] {
                        keep[idx] = true;
                        kept += 1;
                        if kept >= target_limit {
                            break;
                        }
                    }
                }
            }
        }

        if kept == 0 {
            if let Some(component) = components.first() {
                for &idx in component {
                    keep[idx] = true;
                }
                kept = component.len();
            }
        }

        let mut filtered_nodes = Vec::with_capacity(kept);
        let mut remap = HashMap::new();
        for (old_idx, node) in nodes.into_iter().enumerate() {
            if keep[old_idx] {
                let new_idx = filtered_nodes.len();
                remap.insert(old_idx, new_idx);
                filtered_nodes.push(node);
            }
        }

        let nodes = filtered_nodes;

        let mut edges = BTreeSet::new();
        for (old_idx, neighbours) in adjacency.iter().enumerate() {
            let Some(&from_idx) = remap.get(&old_idx) else {
                continue;
            };
            for &target in neighbours {
                if let Some(&to_idx) = remap.get(&target) {
                    edges.insert((from_idx, to_idx));
                }
            }
        }

        // Remove isolated nodes (nodes with no incoming or outgoing edges in the filtered graph).
        let mut in_degree = vec![0usize; nodes.len()];
        let mut out_degree = vec![0usize; nodes.len()];
        for &(from, to) in &edges {
            out_degree[from] += 1;
            in_degree[to] += 1;
        }

        let mut final_keep: Vec<bool> = (0..nodes.len())
            .map(|idx| in_degree[idx] > 0 || out_degree[idx] > 0)
            .collect();

        let mut kept_after_degree = final_keep.iter().filter(|&&keep| keep).count();
        if kept_after_degree < target_limit {
            for idx in 0..nodes.len() {
                if !final_keep[idx] {
                    final_keep[idx] = true;
                    kept_after_degree += 1;
                    if kept_after_degree >= target_limit {
                        break;
                    }
                }
            }
        }

        if kept_after_degree == 0 && !nodes.is_empty() {
            let retain = target_limit.max(1).min(nodes.len());
            for idx in 0..retain {
                final_keep[idx] = true;
            }
        }

        let mut final_nodes = Vec::new();
        let mut final_remap = HashMap::new();
        for (old_idx, node) in nodes.into_iter().enumerate() {
            if final_keep[old_idx] {
                let new_idx = final_nodes.len();
                final_remap.insert(old_idx, new_idx);
                final_nodes.push(node);
            }
        }

        let nodes = final_nodes;

        let final_edges: BTreeSet<_> = edges
            .into_iter()
            .filter_map(|(from, to)| {
                let new_from = final_remap.get(&from).copied()?;
                let new_to = final_remap.get(&to).copied()?;
                Some((new_from, new_to))
            })
            .collect();
        let edges = final_edges;

        // Extract crate/module paths from node locations
        let mut crate_groups: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, node) in nodes.iter().enumerate() {
            if let Some(location) = &node.location {
                let crate_path = extract_group_path(location);
                crate_groups
                    .entry(crate_path)
                    .or_insert_with(Vec::new)
                    .push(idx);
            }
        }

        let mut output = String::from("digraph CompactProject {\n");

        // Generate subgraphs for each crate/module group
        let mut subgraph_counter = 0;
        for (crate_path, node_indices) in crate_groups.iter() {
            output.push_str(&format!("  subgraph cluster_{} {{\n", subgraph_counter));
            output.push_str(&format!("    label=\"{}\";\n", escape_label(crate_path)));
            output.push_str("    style=filled;\n");
            output.push_str("    color=lightgrey;\n");

            for &idx in node_indices {
                let node = &nodes[idx];
                let parts = vec![node.name.clone()];
                let mut tooltip = None;
                if let Some(location) = &node.location {
                    let (_display, full) = summarize_location(location);
                    tooltip = Some(full);
                }
                let label = parts
                    .into_iter()
                    .map(|part| escape_label(&part))
                    .collect::<Vec<_>>()
                    .join("\\n");
                let mut attrs = vec![format!("label=\"{}\"", label)];
                if let Some(full) = tooltip {
                    let escaped_full = escape_attr(&full);
                    attrs.push(format!("full_path=\"{}\"", escaped_full));
                }
                output.push_str(&format!("    n{} [{}];\n", idx, attrs.join(", ")));
            }

            output.push_str("  }\n");
            subgraph_counter += 1;
        }

        for (from, to) in edges {
            output.push_str(&format!("  n{} -> n{};\n", from, to));
        }
        output.push_str("}\n");
        output
    }

    fn add_cross_edge(
        &self,
        from_idx: usize,
        to_idx: usize,
        from_block: BlockId,
        to_block: BlockId,
    ) {
        if from_idx == to_idx {
            let unit = &self.units[from_idx];
            if !unit
                .edges
                .has_relation(from_block, BlockRelation::DependsOn, to_block)
            {
                unit.edges.add_relation(from_block, to_block);
            }
            return;
        }

        let from_unit = &self.units[from_idx];
        from_unit
            .edges
            .add_relation_if_not_exists(from_block, BlockRelation::DependsOn, to_block);

        let to_unit = &self.units[to_idx];
        to_unit
            .edges
            .add_relation_if_not_exists(to_block, BlockRelation::DependedBy, from_block);
    }
}

#[derive(Debug)]
struct GraphBuilder<'tcx, Language> {
    unit: CompileUnit<'tcx>,
    root: Option<BlockId>,
    children_stack: Vec<Vec<BlockId>>,
    config: GraphBuildConfig,
    _marker: PhantomData<Language>,
}

impl<'tcx, Language: LanguageTrait> GraphBuilder<'tcx, Language> {
    fn new(unit: CompileUnit<'tcx>, config: GraphBuildConfig) -> Self {
        Self {
            unit,
            root: None,
            children_stack: Vec::new(),
            config,
            _marker: PhantomData,
        }
    }

    fn next_id(&self) -> BlockId {
        self.unit.reserve_block_id()
    }

    fn create_block(
        &self,
        id: BlockId,
        node: HirNode<'tcx>,
        kind: BlockKind,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> BasicBlock<'tcx> {
        let arena = &self.unit.cc.block_arena;
        match kind {
            BlockKind::Root => {
                // Extract file_name from HirFile node if available
                let file_name = node.as_file().map(|file| file.file_path.clone());
                let block = BlockRoot::from_hir(id, node, parent, children, file_name);
                BasicBlock::Root(arena.alloc(block))
            }
            BlockKind::Func => {
                let block = BlockFunc::from_hir(id, node, parent, children);
                BasicBlock::Func(arena.alloc(block))
            }
            BlockKind::Class => {
                let block = BlockClass::from_hir(id, node, parent, children);
                BasicBlock::Class(arena.alloc(block))
            }
            BlockKind::Stmt => {
                let stmt = BlockStmt::from_hir(id, node, parent, children);
                BasicBlock::Stmt(arena.alloc(stmt))
            }
            BlockKind::Call => {
                let stmt = BlockCall::from_hir(id, node, parent, children);
                BasicBlock::Call(arena.alloc(stmt))
            }
            BlockKind::Enum => {
                let enum_ty = BlockEnum::from_hir(id, node, parent, children);
                BasicBlock::Enum(arena.alloc(enum_ty))
            }
            BlockKind::Const => {
                let stmt = BlockConst::from_hir(id, node, parent, children);
                BasicBlock::Const(arena.alloc(stmt))
            }
            BlockKind::Impl => {
                let block = BlockImpl::from_hir(id, node, parent, children);
                BasicBlock::Impl(arena.alloc(block))
            }
            BlockKind::Field => {
                let block = BlockField::from_hir(id, node, parent, children);
                BasicBlock::Field(arena.alloc(block))
            }
            _ => {
                panic!("unknown block kind: {}", kind)
            }
        }
    }

    fn build_edges(&self, node: HirNode<'tcx>) -> BlockRelationMap {
        let edges = BlockRelationMap::default();
        let mut visited = HashSet::new();
        let mut unresolved = HashSet::new();
        self.collect_edges(node, &edges, &mut visited, &mut unresolved);
        edges
    }

    fn collect_edges(
        &self,
        node: HirNode<'tcx>,
        edges: &BlockRelationMap,
        visited: &mut HashSet<SymId>,
        unresolved: &mut HashSet<SymId>,
    ) {
        // Try to process symbol dependencies for this node
        if let Some(scope) = self.unit.opt_get_scope(node.hir_id()) {
            if let Some(symbol) = scope.symbol() {
                self.process_symbol(symbol, edges, visited, unresolved);
            }
        }

        // Recurse into children
        for &child_id in node.children() {
            let child = self.unit.hir_node(child_id);
            self.collect_edges(child, edges, visited, unresolved);
        }
    }

    fn process_symbol(
        &self,
        symbol: &'tcx Symbol,
        edges: &BlockRelationMap,
        visited: &mut HashSet<SymId>,
        unresolved: &mut HashSet<SymId>,
    ) {
        let symbol_id = symbol.id;

        // Avoid processing the same symbol twice
        if !visited.insert(symbol_id) {
            return;
        }

        let Some(from_block) = symbol.block_id() else {
            return;
        };

        for &dep_id in symbol.depends.borrow().iter() {
            self.link_dependency(dep_id, from_block, edges, unresolved);
        }
    }
    fn link_dependency(
        &self,
        dep_id: SymId,
        from_block: BlockId,
        edges: &BlockRelationMap,
        unresolved: &mut HashSet<SymId>,
    ) {
        // If target symbol exists and has a block, add the dependency edge
        if let Some(target_symbol) = self.unit.opt_get_symbol(dep_id) {
            if let Some(to_block) = target_symbol.block_id() {
                if !edges.has_relation(from_block, BlockRelation::DependsOn, to_block) {
                    edges.add_relation(from_block, to_block);
                }
                let target_unit = target_symbol.unit_index();
                if target_unit.is_some()
                    && target_unit != Some(self.unit.index)
                    && unresolved.insert(dep_id)
                {
                    self.unit.add_unresolved_symbol(target_symbol);
                }
                return;
            }

            // Target symbol exists but block not yet known
            if unresolved.insert(dep_id) {
                self.unit.add_unresolved_symbol(target_symbol);
            }
            return;
        }

        // Target symbol not found at all
        unresolved.insert(dep_id);
    }

    fn build_block(&mut self, node: HirNode<'tcx>, parent: BlockId, recursive: bool) {
        let id = self.next_id();
        let block_kind = Language::block_kind(node.kind_id());
        assert_ne!(block_kind, BlockKind::Undefined);

        if self.root.is_none() {
            self.root = Some(id);
        }

        let children = if recursive {
            self.children_stack.push(Vec::new());
            self.visit_children(node, id);

            self.children_stack.pop().unwrap()
        } else {
            Vec::new()
        };

        let block = self.create_block(id, node, block_kind, Some(parent), children);
        if let Some(scope) = self.unit.opt_get_scope(node.hir_id()) {
            if let Some(symbol) = scope.symbol() {
                // Only set the block ID if it hasn't been set before
                // This prevents impl blocks from overwriting struct block IDs
                if symbol.block_id().is_none() {
                    symbol.set_block_id(Some(id));
                }
            }
        }
        self.unit.insert_block(id, block, parent);

        if let Some(children) = self.children_stack.last_mut() {
            children.push(id);
        }
    }
}

impl<'tcx, Language: LanguageTrait> HirVisitor<'tcx> for GraphBuilder<'tcx, Language> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_file(&mut self, node: HirNode<'tcx>, parent: BlockId) {
        self.children_stack.push(Vec::new());
        self.build_block(node, parent, true);
    }

    fn visit_internal(&mut self, node: HirNode<'tcx>, parent: BlockId) {
        let kind = Language::block_kind(node.kind_id());
        if self.config.compact {
            if kind == BlockKind::Root {
                self.build_block(node, parent, true);
            } else {
                self.visit_children(node, parent);
            }
            return;
        }

        // Non-compact mode: process all defined kinds
        if kind != BlockKind::Undefined {
            self.build_block(node, parent, false);
        } else {
            self.visit_children(node, parent);
        }
    }

    fn visit_scope(&mut self, node: HirNode<'tcx>, parent: BlockId) {
        let kind = Language::block_kind(node.kind_id());
        if self.config.compact {
            // In compact mode, only create blocks for major constructs (Class, Enum, Impl)
            // Skip functions, fields, and scopes to reduce graph size
            match kind {
                BlockKind::Class | BlockKind::Enum => {
                    // Build with recursion enabled to capture nested major constructs
                    self.build_block(node, parent, true);
                }
                // Skip all other scopes - don't recurse
                _ => {
                    // Stop here, do not visit children
                    // self.visit_children(node, parent);
                }
            }
            return;
        }

        // Non-compact mode: build blocks for all major constructs
        match kind {
            BlockKind::Func
            | BlockKind::Class
            | BlockKind::Enum
            | BlockKind::Const
            | BlockKind::Impl
            | BlockKind::Field => self.build_block(node, parent, true),
            _ => self.visit_children(node, parent),
        }
    }
}

pub fn build_llmcc_graph_with_config<'tcx, L: LanguageTrait>(
    unit: CompileUnit<'tcx>,
    unit_index: usize,
    config: GraphBuildConfig,
) -> Result<UnitGraph, Box<dyn std::error::Error>> {
    let root_hir = unit
        .file_start_hir_id()
        .ok_or("missing file start HIR id")?;
    let mut builder = GraphBuilder::<L>::new(unit, config);
    let root_node = unit.hir_node(root_hir);
    builder.visit_node(root_node, BlockId::ROOT_PARENT);

    let root_block = builder.root;
    let root_block = root_block.ok_or("graph builder produced no root")?;
    let edges = builder.build_edges(root_node);
    Ok(UnitGraph::new(unit_index, root_block, edges))
}

pub fn build_llmcc_graph<'tcx, L: LanguageTrait>(
    unit: CompileUnit<'tcx>,
    unit_index: usize,
) -> Result<UnitGraph, Box<dyn std::error::Error>> {
    build_llmcc_graph_with_config::<L>(unit, unit_index, GraphBuildConfig::default())
}
