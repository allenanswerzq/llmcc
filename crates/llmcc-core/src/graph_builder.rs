use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::marker::PhantomData;
use std::mem;
use std::path::Path;

use crate::block::Arena as BlockArena;
pub use crate::block::{BasicBlock, BlockId, BlockKind, BlockRelation};
use crate::block::{
    BlockCall, BlockClass, BlockConst, BlockEnum, BlockField, BlockFunc, BlockImpl, BlockRoot,
    BlockStmt,
};
use crate::block_rel::BlockRelationMap;
use crate::context::{CompileCtxt, CompileUnit};
use crate::ir::HirNode;
use crate::lang_def::LanguageTrait;
use crate::pagerank::PageRanker;
use crate::symbol::{SymId, Symbol};
use crate::visit::HirVisitor;
use crate::DynError;

const COMPACT_INTERESTING_KINDS: [BlockKind; 2] = [BlockKind::Class, BlockKind::Enum];

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
    compact_rank_limit: Option<usize>,
    pagerank_enabled: bool,
}

impl<'tcx> ProjectGraph<'tcx> {
    pub fn new(cc: &'tcx CompileCtxt<'tcx>) -> Self {
        Self {
            cc,
            units: Vec::new(),
            compact_rank_limit: None,
            pagerank_enabled: false,
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
        self.pagerank_enabled = self.compact_rank_limit.is_some();
    }

    pub fn link_units(&mut self) {
        if self.units.is_empty() {
            return;
        }

        let mut unresolved = self.cc.unresolve_symbols.write().unwrap();

        unresolved.retain(|symbol_ref| {
            let target = *symbol_ref;
            let Some(target_block) = target.block_id() else {
                return false;
            };

            let dependents: Vec<SymId> = target.depended.read().unwrap().clone();
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
        let block_indexes = self.cc.block_indexes.read().unwrap();
        let matches = block_indexes.find_by_name(name);

        matches.first().map(|(unit_index, _, block_id)| GraphNode {
            unit_index: *unit_index,
            block_id: *block_id,
        })
    }

    pub fn blocks_by_name(&self, name: &str) -> Vec<GraphNode> {
        let block_indexes = self.cc.block_indexes.read().unwrap();
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
        let block_indexes = self.cc.block_indexes.read().unwrap();
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
                    .map(|loc| extract_group_path(loc))
                    .unwrap_or_else(|| extract_group_path(&path));

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
        let block_indexes = self.cc.block_indexes.read().unwrap();
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
        let block_indexes = self.cc.block_indexes.read().unwrap();
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
        let block_indexes = self.cc.block_indexes.read().unwrap();
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
        let block_indexes = self.cc.block_indexes.read().unwrap();
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
        let block_indexes = self.cc.block_indexes.read().unwrap();
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
                    let block_indexes = self.cc.block_indexes.read().unwrap();
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
                        let indexes = self.cc.block_indexes.read().unwrap();
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
        let block_indexes = self.cc.block_indexes.read().unwrap();

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
        let block_indexes = self.cc.block_indexes.read().unwrap();

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

    fn render_compact_graph_inner(&self, top_k: Option<usize>) -> String {
        let nodes = self.collect_sorted_compact_nodes(top_k);

        if nodes.is_empty() {
            return "digraph DesignGraph {\n}\n".to_string();
        }

        let node_index = build_compact_node_index(&nodes);
        let edges = self.collect_compact_edges(&nodes, &node_index);

        let pruned = prune_compact_components(&nodes, &edges);
        if pruned.nodes.is_empty() {
            return "digraph DesignGraph {\n}\n".to_string();
        }

        let reduced_edges = reduce_transitive_edges(&pruned.nodes, &pruned.edges);

        render_compact_dot(&pruned.nodes, &reduced_edges)
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
        let arena = self.unit.cc.block_arena.lock().unwrap();
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

        let dependencies = symbol.depends.read().unwrap().clone();
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
        if kind != BlockKind::Undefined {
            self.build_block(node, parent, false);
        } else {
            self.visit_children(node, parent);
        }
    }

    fn visit_scope(&mut self, node: HirNode<'tcx>, parent: BlockId) {
        let kind = Language::block_kind(node.kind_id());
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

pub fn build_llmcc_graph<'tcx, L: LanguageTrait>(
    unit: CompileUnit<'tcx>,
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

#[derive(Clone)]
struct CompactNode {
    block_id: BlockId,
    unit_index: usize,
    name: String,
    location: Option<String>,
    group: String,
}

fn compact_byte_to_line(content: &[u8], byte_pos: usize) -> usize {
    let clamped = byte_pos.min(content.len());
    content[..clamped].iter().filter(|&&ch| ch == b'\n').count() + 1
}

fn escape_dot_label(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn escape_dot_attr(input: &str) -> String {
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

    let start = components.len().saturating_sub(3);
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
    let path = location.split(':').next().unwrap_or(location);
    let parts: Vec<&str> = path.split(['/', '\\']).collect();

    if let Some(src_idx) = parts.iter().position(|&p| p == "src") {
        if src_idx > 0 {
            return parts[src_idx - 1].to_string();
        }
    }

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
        if let Some(stem) = file_stem
            .as_ref()
            .filter(|stem| stem.as_str() != "__init__")
        {
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

fn render_compact_dot(nodes: &[CompactNode], edges: &BTreeSet<(usize, usize)>) -> String {
    let mut crate_groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, node) in nodes.iter().enumerate() {
        crate_groups
            .entry(node.group.clone())
            .or_default()
            .push(idx);
    }

    let mut output = String::from("digraph DesignGraph {\n");

    for (subgraph_counter, (crate_path, node_indices)) in crate_groups.iter().enumerate() {
        output.push_str(&format!("  subgraph cluster_{} {{\n", subgraph_counter));
        output.push_str(&format!(
            "    label=\"{}\";\n",
            escape_dot_label(crate_path)
        ));
        output.push_str("    style=filled;\n");
        output.push_str("    color=lightgrey;\n");

        for &idx in node_indices {
            let node = &nodes[idx];
            let label = escape_dot_label(&node.name);
            let mut attrs = vec![format!("label=\"{}\"", label)];

            if let Some(location) = &node.location {
                let (_display, full) = summarize_location(location);
                let escaped_full = escape_dot_attr(&full);
                attrs.push(format!("full_path=\"{}\"", escaped_full));
            }

            output.push_str(&format!("    n{} [{}];\n", idx, attrs.join(", ")));
        }

        output.push_str("  }\n");
    }

    for &(from, to) in edges {
        output.push_str(&format!("  n{} -> n{};\n", from, to));
    }

    output.push_str("}\n");
    output
}

fn build_compact_node_index(nodes: &[CompactNode]) -> HashMap<BlockId, usize> {
    let mut node_index = HashMap::with_capacity(nodes.len());
    for (idx, node) in nodes.iter().enumerate() {
        node_index.insert(node.block_id, idx);
    }
    node_index
}

struct PrunedGraph {
    nodes: Vec<CompactNode>,
    edges: BTreeSet<(usize, usize)>,
}

fn prune_compact_components(
    nodes: &[CompactNode],
    edges: &BTreeSet<(usize, usize)>,
) -> PrunedGraph {
    if nodes.is_empty() {
        return PrunedGraph {
            nodes: Vec::new(),
            edges: BTreeSet::new(),
        };
    }

    let components = find_connected_components(nodes.len(), edges);
    if components.is_empty() {
        return PrunedGraph {
            nodes: nodes.to_vec(),
            edges: edges.clone(),
        };
    }

    let mut retained_indices = HashSet::new();
    for component in components {
        if component.len() == 1 {
            let idx = component[0];
            let has_edges = edges.iter().any(|&(from, to)| from == idx || to == idx);
            if !has_edges {
                continue;
            }
        }
        retained_indices.extend(component);
    }

    if retained_indices.is_empty() {
        return PrunedGraph {
            nodes: Vec::new(),
            edges: BTreeSet::new(),
        };
    }

    let mut retained_nodes = Vec::new();
    let mut old_to_new = HashMap::new();
    for (new_idx, old_idx) in retained_indices.iter().enumerate() {
        retained_nodes.push(nodes[*old_idx].clone());
        old_to_new.insert(*old_idx, new_idx);
    }

    let mut retained_edges = BTreeSet::new();
    for &(from, to) in edges {
        if let (Some(&new_from), Some(&new_to)) = (old_to_new.get(&from), old_to_new.get(&to)) {
            retained_edges.insert((new_from, new_to));
        }
    }

    PrunedGraph {
        nodes: retained_nodes,
        edges: retained_edges,
    }
}

fn find_connected_components(
    node_count: usize,
    edges: &BTreeSet<(usize, usize)>,
) -> Vec<Vec<usize>> {
    if node_count == 0 {
        return Vec::new();
    }

    let mut graph: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(from, to) in edges.iter() {
        graph.entry(from).or_default().push(to);
        graph.entry(to).or_default().push(from);
    }

    let mut visited = HashSet::new();
    let mut components = Vec::new();

    for node in 0..node_count {
        if visited.contains(&node) {
            continue;
        }

        let mut component = Vec::new();
        let mut stack = vec![node];

        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }

            component.push(current);

            if let Some(neighbors) = graph.get(&current) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        stack.push(neighbor);
                    }
                }
            }
        }

        components.push(component);
    }

    components
}

fn reduce_transitive_edges(
    nodes: &[CompactNode],
    edges: &BTreeSet<(usize, usize)>,
) -> BTreeSet<(usize, usize)> {
    if nodes.is_empty() {
        return BTreeSet::new();
    }

    let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(from, to) in edges.iter() {
        adjacency.entry(from).or_default().push(to);
    }

    let mut minimal_edges = BTreeSet::new();

    for &(from, to) in edges.iter() {
        if !has_alternative_path(from, to, &adjacency, (from, to)) {
            minimal_edges.insert((from, to));
        }
    }

    minimal_edges
}

fn has_alternative_path(
    start: usize,
    target: usize,
    adjacency: &HashMap<usize, Vec<usize>>,
    edge_to_skip: (usize, usize),
) -> bool {
    let mut visited = HashSet::new();
    let mut stack: Vec<usize> = adjacency
        .get(&start)
        .into_iter()
        .flat_map(|neighbors| neighbors.iter())
        .filter_map(|&neighbor| {
            if (start, neighbor) == edge_to_skip {
                None
            } else {
                Some(neighbor)
            }
        })
        .collect();

    while let Some(current) = stack.pop() {
        if !visited.insert(current) {
            continue;
        }

        if current == target {
            return true;
        }

        if let Some(neighbors) = adjacency.get(&current) {
            for &neighbor in neighbors {
                if (current, neighbor) == edge_to_skip {
                    continue;
                }
                if !visited.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }
    }

    false
}
