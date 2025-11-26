use std::collections::{BTreeSet, HashMap, HashSet};

use crate::block::{BlockId, BlockKind, BlockRelation};
use crate::block_rel::BlockRelationMap;
use crate::context::CompileCtxt;
use crate::graph_render::{CompactNode, GraphRenderer, LabeledEdge};
use crate::pagerank::PageRanker;
use crate::symbol::{DepKind, SymId, SymKind};

#[derive(Debug, Clone)]
pub struct UnitGraph {
    /// Compile unit this graph belongs to
    unit_index: usize,
    /// Root block ID of this unit
    root: BlockId,
    /// Edge of this graph unit
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphNode {
    pub unit_index: usize,
    pub block_id: BlockId,
}

const INTERESTING_KINDS: [BlockKind; 4] = [
    BlockKind::Class,
    BlockKind::Trait,
    BlockKind::Enum,
    BlockKind::Func,
];

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
    top_k: Option<usize>,
    pagerank_enabled: bool,
    /// Component grouping depth from FQN for graph visualization
    component_depth: usize,
}

impl<'tcx> ProjectGraph<'tcx> {
    pub fn new(cc: &'tcx CompileCtxt<'tcx>) -> Self {
        Self {
            cc,
            units: Vec::new(),
            top_k: None,
            pagerank_enabled: false,
            component_depth: 2, // Default to top-level modules
        }
    }

    /// Set the component depth for graph visualization
    pub fn set_component_depth(&mut self, depth: usize) {
        self.component_depth = depth;
    }

    pub fn add_child(&mut self, graph: UnitGraph) {
        self.units.push(graph);
    }

    /// Add multiple unit graphs to the project graph.
    pub fn add_children(&mut self, graphs: Vec<UnitGraph>) {
        self.units.extend(graphs);
    }

    /// Configure the number of PageRank-filtered nodes retained when rendering compact graphs.
    pub fn set_top_k(&mut self, limit: Option<usize>) {
        self.top_k = match limit {
            Some(0) => None,
            other => other,
        };
        self.pagerank_enabled = self.top_k.is_some();
    }

    pub fn link_units(&mut self) {
        if self.units.is_empty() {
            return;
        }

        let unresolved_symbols = {
            let mut unresolved = self.cc.unresolve_symbols.write();
            std::mem::take(&mut *unresolved)
        };

        if unresolved_symbols.is_empty() {
            return;
        }

        let mut unique_symbols = Vec::new();
        let mut seen_targets = HashSet::new();
        for symbol_ref in unresolved_symbols {
            if seen_targets.insert(symbol_ref.id) {
                unique_symbols.push(symbol_ref);
            }
        }

        if unique_symbols.is_empty() {
            return;
        }

        let cross_edges = parking_lot::Mutex::new(Vec::new());

        use rayon::prelude::*;
        unique_symbols.into_par_iter().for_each(|symbol_ref| {
            let target = symbol_ref;
            let Some(target_block) = target.block_id() else {
                return;
            };

            let Some(target_unit) = target.unit_index() else {
                return;
            };

            let dependents_guard = target.depended.read();
            if dependents_guard.is_empty() {
                return;
            }

            let mut seen_dependents = HashSet::new();

            for &(dependent_id, _dep_kind) in dependents_guard.iter() {
                if !seen_dependents.insert(dependent_id) {
                    continue;
                }
                let Some(source_symbol) = self.cc.opt_get_symbol(dependent_id) else {
                    continue;
                };
                let Some(from_block) = source_symbol.block_id() else {
                    continue;
                };
                let Some(from_unit) = source_symbol.unit_index() else {
                    continue;
                };

                cross_edges
                    .lock()
                    .push((from_unit, target_unit, from_block, target_block));
            }
        });

        let collected_edges = cross_edges.into_inner();
        if collected_edges.is_empty() {
            return;
        }

        let unit_positions: HashMap<usize, usize> = self
            .units
            .iter()
            .enumerate()
            .map(|(pos, unit)| (unit.unit_index(), pos))
            .collect();

        let mut depends_map: HashMap<usize, HashMap<BlockId, Vec<BlockId>>> = HashMap::new();
        let mut depended_map: HashMap<usize, HashMap<BlockId, Vec<BlockId>>> = HashMap::new();

        for (from_unit, target_unit, from_block, target_block) in collected_edges {
            depends_map
                .entry(from_unit)
                .or_default()
                .entry(from_block)
                .or_default()
                .push(target_block);

            depended_map
                .entry(target_unit)
                .or_default()
                .entry(target_block)
                .or_default()
                .push(from_block);
        }

        for (unit_idx, mut edges) in depends_map {
            if let Some(&pos) = unit_positions.get(&unit_idx) {
                let unit_graph = &self.units[pos];
                for (from_block, mut targets) in edges.drain() {
                    targets.sort_unstable_by_key(|id| id.as_u32());
                    targets.dedup();
                    unit_graph.edges.add_relation_impls(
                        from_block,
                        BlockRelation::DependsOn,
                        &targets,
                    );
                }
            }
        }

        for (unit_idx, mut edges) in depended_map {
            if let Some(&pos) = unit_positions.get(&unit_idx) {
                let unit_graph = &self.units[pos];
                for (from_block, mut targets) in edges.drain() {
                    targets.sort_unstable_by_key(|id| id.as_u32());
                    targets.dedup();
                    unit_graph.edges.add_relation_impls(
                        from_block,
                        BlockRelation::DependedBy,
                        &targets,
                    );
                }
            }
        }
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
        let block_indexes = self.cc.block_indexes.read();
        let matches = block_indexes.find_by_name(name);

        matches.first().map(|(unit_index, _, block_id)| GraphNode {
            unit_index: *unit_index,
            block_id: *block_id,
        })
    }

    pub fn blocks_by_name(&self, name: &str) -> Vec<GraphNode> {
        let block_indexes = self.cc.block_indexes.read();
        let matches = block_indexes.find_by_name(name);

        matches
            .into_iter()
            .map(|(unit_index, _, block_id)| GraphNode {
                unit_index,
                block_id,
            })
            .collect()
    }

    pub fn render_design_graph(&self) -> String {
        let top_k = self.top_k;
        let nodes = self.collect_sorted_nodes(top_k);

        if nodes.is_empty() {
            return "digraph project {\n}\n".to_string();
        }

        let renderer = GraphRenderer::new(&nodes);
        let node_index = renderer.build_node_index();
        let edges = self.collect_edges(renderer.nodes(), &node_index);

        renderer.render(&edges, self.component_depth)
    }

    /// Render an architecture graph showing input/output relations.
    ///
    /// In an architecture graph:
    /// - Input arguments have edges pointing TO the function
    /// - Function has edges pointing TO return types (output)
    /// - Traits have edges pointing TO structs that implement them
    ///
    /// This is different from the dependency graph which shows "uses" relationships.
    pub fn render_arch_graph(&self) -> String {
        let top_k = self.top_k;
        let all_nodes = self.collect_sorted_nodes(top_k);

        // Filter nodes for arch-graph:
        // - Keep all types (Struct, Trait, Enum) regardless of visibility
        // - Only keep public functions (private functions are implementation details)
        let nodes: Vec<_> = all_nodes
            .into_iter()
            .filter(|node| {
                match node.sym_kind {
                    Some(SymKind::Function) => node.is_public,
                    _ => true, // Keep all other kinds (Struct, Trait, Enum, etc.)
                }
            })
            .collect();

        if nodes.is_empty() {
            return "digraph architecture {\n}\n".to_string();
        }

        let renderer = GraphRenderer::new(&nodes);
        let node_index = renderer.build_node_index();
        let edges = self.collect_arch_edges(renderer.nodes(), &node_index);

        renderer.render_arch(&edges, self.component_depth)
    }

    /// Collect edges for the architecture graph based on DepKind.
    ///
    /// Edge direction in arch graph:
    /// - ParamType: param_type -> func (input flows into function)
    /// - ReturnType: func -> return_type (output flows from function)
    /// - Implements: impl_struct -> trait (struct advertises trait capability)
    /// - Calls: caller -> callee (control flow follows call sites)
    /// - FieldType / Calls / Instantiates: from -> to (normal dependency direction)
    /// - Uses: fallback from -> to when no more specific edge exists
    fn collect_arch_edges(
        &self,
        nodes: &[CompactNode],
        node_index: &HashMap<BlockId, usize>,
    ) -> BTreeSet<LabeledEdge> {
        let mut edges = BTreeSet::new();
        let symbol_map = self.cc.symbol_map.read();

        for node in nodes {
            let Some(_unit_graph) = self.unit_graph(node.unit_index) else {
                continue;
            };
            let from_idx = node_index[&node.block_id];

            // Use the symbol captured during node collection when available
            let symbol_opt = node
                .sym_id
                .and_then(|sym_id| symbol_map.get(&sym_id))
                .copied();

            if let Some(symbol) = symbol_opt {
                // Use typed dependencies for arch graph
                let depends = symbol.depends.read();
                for &(dep_id, dep_kind) in depends.iter() {
                    let Some(dep_symbol) = symbol_map.get(&dep_id) else {
                        continue;
                    };
                    let Some(dep_block_id) = dep_symbol.block_id() else {
                        continue;
                    };
                    let Some(&to_idx) = node_index.get(&dep_block_id) else {
                        continue;
                    };

                    // Determine edge direction based on DepKind
                    // For arch-graph, prioritize specific kinds over generic Uses
                    match dep_kind {
                        DepKind::ParamType => {
                            // Input: param_type -> func
                            edges.insert(LabeledEdge::new(to_idx, from_idx, DepKind::ParamType));
                        }
                        DepKind::ReturnType => {
                            // Output: func -> return_type
                            edges.insert(LabeledEdge::new(from_idx, to_idx, DepKind::ReturnType));
                        }
                        DepKind::Implements => {
                            // trait -> implementing_struct (trait flows to implementor)
                            edges.insert(LabeledEdge::new(to_idx, from_idx, DepKind::Implements));
                        }
                        DepKind::FieldType => {
                            edges.insert(LabeledEdge::new(from_idx, to_idx, DepKind::FieldType));
                        }
                        DepKind::Calls => {
                            edges.insert(LabeledEdge::new(from_idx, to_idx, DepKind::Calls));
                        }
                        DepKind::Instantiates => {
                            edges.insert(LabeledEdge::new(from_idx, to_idx, DepKind::Instantiates));
                        }
                        DepKind::TypeBound => {
                            // Type bound flows into struct: trait_bound -> struct
                            edges.insert(LabeledEdge::new(to_idx, from_idx, DepKind::TypeBound));
                        }
                        DepKind::Uses => {
                            // For arch-graph, skip generic Uses dependencies
                            // Arch-graph focuses on structural relationships:
                            // - type flows (params, returns, fields, bounds)
                            // - trait implementations
                            // Uses captures too many incidental references (e.g., enum variants in expressions)
                        }
                    }
                }
            }
        }

        edges
    }

    fn ranked_block_filter(
        &self,
        top_k: Option<usize>,
        interesting_kinds: &[BlockKind],
    ) -> Option<HashSet<BlockId>> {
        let ranked_order = top_k.and_then(|limit| {
            let ranker = PageRanker::new(self);
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

        ranked_order.map(|ordered| ordered.into_iter().collect())
    }

    fn collect_nodes(
        &self,
        interesting_kinds: &[BlockKind],
        ranked_filter: Option<&HashSet<BlockId>>,
    ) -> Vec<CompactNode> {
        let block_indexes = self.cc.block_indexes.read();

        block_indexes
            .block_id_index
            .iter()
            .filter_map(|(&block_id, (unit_index, name_opt, kind))| {
                if !interesting_kinds.contains(kind) {
                    return None;
                }

                if let Some(ids) = ranked_filter
                    && !ids.contains(&block_id)
                {
                    return None;
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

                let raw_path = unit
                    .file_path()
                    .or_else(|| unit.file().path())
                    .unwrap_or("<unknown>");

                // Use raw path directly - canonicalize is very expensive
                let path = raw_path.to_string();

                let file_bytes = unit.file().content();
                let location = block
                    .opt_node()
                    .map(|node| {
                        let line = compact_byte_to_line(file_bytes, node.start_byte());
                        format!("{path}:{line}")
                    })
                    .or(Some(path.clone()));

                let mut sym_id: Option<SymId> = None;
                let mut sym_kind = None;
                let mut fqn = "unknown".to_string();
                let mut is_public = false;

                if let Some(symbol) = block
                    .opt_node()
                    .and_then(|node| node.as_scope())
                    .and_then(|scope_node| scope_node.opt_scope())
                    .and_then(|scope| scope.opt_symbol())
                {
                    sym_id = Some(symbol.id());
                    sym_kind = Some(symbol.kind());
                    is_public = symbol.is_global();
                    if let Some(resolved) = self.cc.interner.resolve_owned(symbol.fqn()) {
                        fqn = resolved;
                    }
                }

                Some(CompactNode {
                    block_id,
                    unit_index: *unit_index,
                    name: display_name,
                    location,
                    fqn,
                    sym_id,
                    sym_kind,
                    is_public,
                })
            })
            .collect()
    }

    fn collect_sorted_nodes(&self, top_k: Option<usize>) -> Vec<CompactNode> {
        let ranked_filter = if self.pagerank_enabled {
            self.ranked_block_filter(top_k, &INTERESTING_KINDS)
        } else {
            None
        };
        let mut nodes = self.collect_nodes(&INTERESTING_KINDS, ranked_filter.as_ref());
        nodes.sort_by(|a, b| a.name.cmp(&b.name));
        nodes
    }

    fn collect_edges(
        &self,
        nodes: &[CompactNode],
        node_index: &HashMap<BlockId, usize>,
    ) -> BTreeSet<(usize, usize)> {
        let mut edges = BTreeSet::new();

        for node in nodes {
            let Some(unit_graph) = self.unit_graph(node.unit_index) else {
                continue;
            };
            let from_idx = node_index[&node.block_id];

            let dependencies = unit_graph
                .edges()
                .get_related(node.block_id, BlockRelation::DependsOn);

            for dep_block_id in dependencies {
                if let Some(&to_idx) = node_index.get(&dep_block_id) {
                    edges.insert((from_idx, to_idx));
                }
            }
        }

        edges
    }

    pub fn block_by_name_in(&self, unit_index: usize, name: &str) -> Option<GraphNode> {
        let block_indexes = self.cc.block_indexes.read();
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
        let block_indexes = self.cc.block_indexes.read();
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
        let block_indexes = self.cc.block_indexes.read();
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
        let block_indexes = self.cc.block_indexes.read();
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
        let block_indexes = self.cc.block_indexes.read();
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
                    let dependencies = unit
                        .edges
                        .get_related(node.block_id, BlockRelation::DependsOn);
                    let block_indexes = self.cc.block_indexes.read();
                    for dep_block_id in dependencies {
                        let dep_unit_index = block_indexes
                            .get_block_info(dep_block_id)
                            .map(|(idx, _, _)| idx)
                            .unwrap_or(node.unit_index);
                        result.push(GraphNode {
                            unit_index: dep_unit_index,
                            block_id: dep_block_id,
                        });
                    }
                }
                BlockRelation::DependedBy => {
                    let mut seen = HashSet::new();

                    let dependents = unit
                        .edges
                        .get_related(node.block_id, BlockRelation::DependedBy);
                    if !dependents.is_empty() {
                        let indexes = self.cc.block_indexes.read();
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
                BlockRelation::Unknown => {}
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

    pub fn find_depended_blocks_recursive(&self, node: GraphNode) -> HashSet<GraphNode> {
        let mut visited = HashSet::new();
        let mut stack = vec![node];
        let relations = vec![BlockRelation::DependedBy];

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
        let block_indexes = self.cc.block_indexes.read();

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
                    let dep_unit_index = block_indexes
                        .get_block_info(dep_block_id)
                        .map(|(idx, _, _)| idx)
                        .unwrap_or(node.unit_index);
                    result.insert(GraphNode {
                        unit_index: dep_unit_index,
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
        let block_indexes = self.cc.block_indexes.read();

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
                    let dep_unit_index = block_indexes
                        .get_block_info(dep_block_id)
                        .map(|(idx, _, _)| idx)
                        .unwrap_or(node.unit_index);
                    result.insert(GraphNode {
                        unit_index: dep_unit_index,
                        block_id: dep_block_id,
                    });
                    stack.push(dep_block_id);
                }
            }
        }

        result
    }
}

fn compact_byte_to_line(content: &[u8], byte_pos: usize) -> usize {
    let clamped = byte_pos.min(content.len());
    content[..clamped].iter().filter(|&&ch| ch == b'\n').count() + 1
}
