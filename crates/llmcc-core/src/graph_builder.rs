use parking_lot::Mutex;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::marker::PhantomData;
use std::mem;
use std::path::Path;

use crate::DynError;
use crate::block::Arena as BlockArena;
pub use crate::block::{BasicBlock, BlockId, BlockKind, BlockRelation};
use crate::block::{
    BlockCall, BlockClass, BlockConst, BlockEnum, BlockField, BlockFunc, BlockImpl, BlockMethod,
    BlockRoot, BlockStmt,
};
use crate::block_rel::BlockRelationMap;
use crate::context::{CompileCtxt, CompileUnit};
use crate::graph_render::{CompactNode, GraphRenderer};
use crate::ir::HirNode;
use crate::lang_def::LanguageTrait;
use crate::module_path::{module_group_from_location, module_group_from_path};
use crate::pagerank::PageRanker;
use crate::symbol::{SymId, Symbol};
use crate::visit::HirVisitor;
use rayon::prelude::*;

const COMPACT_INTERESTING_KINDS: [BlockKind; 3] = [
    BlockKind::Class,
    BlockKind::Enum,
    BlockKind::Func,
    // BlockKind::Method,
];

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

#[derive(Debug, Clone, Copy, Default)]
pub struct GraphBuildConfig;

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
    top_k: Option<usize>,
    pagerank_enabled: bool,
}

impl<'tcx> ProjectGraph<'tcx> {
    pub fn new(cc: &'tcx CompileCtxt<'tcx>) -> Self {
        Self {
            cc,
            units: Vec::new(),
            top_k: None,
            pagerank_enabled: false,
        }
    }

    pub fn add_child(&mut self, graph: UnitGraph) {
        self.units.push(graph);
    }

    /// Configure the number of PageRank-filtered nodes retained when rendering compact graphs.
    pub fn set_compact_rank_limit(&mut self, limit: Option<usize>) {
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

        let cross_edges = Mutex::new(Vec::new());

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

            for &dependent_id in dependents_guard.iter() {
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
        let nodes = self.collect_sorted_compact_nodes(top_k);

        if nodes.is_empty() {
            return "digraph DesignGraph {\n}\n".to_string();
        }

        let renderer = GraphRenderer::new(&nodes);
        let node_index = renderer.build_node_index();
        let edges = self.collect_compact_edges(renderer.nodes(), &node_index);

        renderer.render(&edges)
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

    fn collect_compact_nodes(
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

                if let Some(ids) = ranked_filter {
                    if !ids.contains(&block_id) {
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

                let raw_path = unit
                    .file_path()
                    .or_else(|| unit.file().path())
                    .unwrap_or("<unknown>");

                let path = std::fs::canonicalize(raw_path)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| raw_path.to_string());

                let file_bytes = unit.file().content();
                let location = block
                    .opt_node()
                    .map(|node| {
                        let line = compact_byte_to_line(file_bytes, node.start_byte());
                        format!("{path}:{line}")
                    })
                    .or(Some(path.clone()));

                let group = location
                    .as_ref()
                    .map(|loc| module_group_from_location(loc))
                    .unwrap_or_else(|| module_group_from_path(Path::new(&path)));

                Some(CompactNode {
                    block_id,
                    unit_index: *unit_index,
                    name: display_name,
                    location,
                    group,
                })
            })
            .collect()
    }

    fn collect_sorted_compact_nodes(&self, top_k: Option<usize>) -> Vec<CompactNode> {
        let ranked_filter = if self.pagerank_enabled {
            self.ranked_block_filter(top_k, &COMPACT_INTERESTING_KINDS)
        } else {
            None
        };
        let mut nodes =
            self.collect_compact_nodes(&COMPACT_INTERESTING_KINDS, ranked_filter.as_ref());
        nodes.sort_by(|a, b| a.name.cmp(&b.name));
        nodes
    }

    fn collect_compact_edges(
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
                    // Get all blocks that this block depends on
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

                    // Direct dependents tracked on this unit (covers cross-unit edges too)
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

#[derive(Debug)]
struct GraphBuilder<'tcx, Language> {
    unit: CompileUnit<'tcx>,
    root: Option<BlockId>,
    children_stack: Vec<Vec<BlockId>>,
    _config: GraphBuildConfig,
    _marker: PhantomData<Language>,
}

impl<'tcx, Language: LanguageTrait> GraphBuilder<'tcx, Language> {
    fn new(unit: CompileUnit<'tcx>, _config: GraphBuildConfig) -> Self {
        Self {
            unit,
            root: None,
            children_stack: Vec::new(),
            _config,
            _marker: PhantomData,
        }
    }

    fn alloc_from_block_arena<T, F>(&self, alloc: F) -> &'tcx T
    where
        F: for<'a> FnOnce(&'a BlockArena<'tcx>) -> &'a mut T,
    {
        let arena = self.unit.cc.block_arena.lock();
        let ptr = alloc(&arena);
        let reference: &T = &*ptr;
        unsafe { mem::transmute::<&T, &'tcx T>(reference) }
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
        match kind {
            BlockKind::Root => {
                let file_name = node.as_file().map(|file| file.file_path.clone());
                let block = BlockRoot::from_hir(id, node, parent, children, file_name);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_root.alloc(block));
                BasicBlock::Root(block_ref)
            }
            BlockKind::Func => {
                let block = BlockFunc::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_func.alloc(block));
                BasicBlock::Func(block_ref)
            }
            BlockKind::Method => {
                let block = BlockMethod::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_method.alloc(block));
                BasicBlock::Method(block_ref)
            }
            BlockKind::Class => {
                let block = BlockClass::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_class.alloc(block));
                BasicBlock::Class(block_ref)
            }
            BlockKind::Stmt => {
                let stmt = BlockStmt::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_stmt.alloc(stmt));
                BasicBlock::Stmt(block_ref)
            }
            BlockKind::Call => {
                let stmt = BlockCall::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_call.alloc(stmt));
                BasicBlock::Call(block_ref)
            }
            BlockKind::Enum => {
                let enum_ty = BlockEnum::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_enum.alloc(enum_ty));
                BasicBlock::Enum(block_ref)
            }
            BlockKind::Const => {
                let stmt = BlockConst::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_const.alloc(stmt));
                BasicBlock::Const(block_ref)
            }
            BlockKind::Impl => {
                let block = BlockImpl::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_impl.alloc(block));
                BasicBlock::Impl(block_ref)
            }
            BlockKind::Field => {
                let block = BlockField::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_field.alloc(block));
                BasicBlock::Field(block_ref)
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
        if let Some(scope) = self.unit.opt_get_scope(node.id()) {
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

        let dependencies = symbol.depends.read().clone();
        for dep_id in dependencies {
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
        let mut block_kind = Language::block_kind(node.kind_id());
        if block_kind == BlockKind::Func {
            let mut current_parent = node.parent();
            while let Some(parent_id) = current_parent {
                let parent_node = self.unit.hir_node(parent_id);
                let parent_kind = Language::block_kind(parent_node.kind_id());
                if matches!(parent_kind, BlockKind::Class | BlockKind::Impl) {
                    block_kind = BlockKind::Method;
                    break;
                }
                if parent_kind == BlockKind::Root {
                    break;
                }
                current_parent = parent_node.parent();
            }
        }

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
        if let Some(scope) = self.unit.opt_get_scope(node.id()) {
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
        match kind {
            BlockKind::Func
            | BlockKind::Method
            | BlockKind::Class
            | BlockKind::Enum
            | BlockKind::Const
            | BlockKind::Impl
            | BlockKind::Field
            | BlockKind::Call => self.build_block(node, parent, false),
            _ => self.visit_children(node, parent),
        }
    }

    fn visit_scope(&mut self, node: HirNode<'tcx>, parent: BlockId) {
        let kind = Language::block_kind(node.kind_id());
        match kind {
            BlockKind::Func
            | BlockKind::Method
            | BlockKind::Class
            | BlockKind::Enum
            | BlockKind::Const
            | BlockKind::Impl
            | BlockKind::Field => self.build_block(node, parent, true),
            _ => self.visit_children(node, parent),
        }
    }
}

pub fn build_llmcc_graph<L: LanguageTrait>(
    unit: CompileUnit<'_>,
    unit_index: usize,
    config: GraphBuildConfig,
) -> Result<UnitGraph, DynError> {
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

fn compact_byte_to_line(content: &[u8], byte_pos: usize) -> usize {
    let clamped = byte_pos.min(content.len());
    content[..clamped].iter().filter(|&&ch| ch == b'\n').count() + 1
}
