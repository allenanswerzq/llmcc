use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

pub use crate::block::{BasicBlock, BlockId, BlockKind, BlockRelation};
use crate::block::{BlockCall, BlockClass, BlockFunc, BlockRoot, BlockStmt};
use crate::block_rel::BlockRelationMap;
use crate::context::{CompileCtxt, CompileUnit};
use crate::ir::HirNode;
use crate::lang_def::LanguageTrait;
use crate::symbol::{Id, Symbol};
use crate::visit::HirVisitor;

#[derive(Debug, Clone)]
pub struct GraphUnit {
    /// Compile unit this graph belongs to
    unit_index: usize,
    /// Root block ID of this unit
    root: BlockId,
    /// Edges of this graph unit
    edges: BlockRelationMap,
    /// All blocks that belong to this unit (used for linking)
    blocks: HashSet<BlockId>,
}

impl GraphUnit {
    pub fn new(
        unit_index: usize,
        root: BlockId,
        edges: BlockRelationMap,
        blocks: Vec<BlockId>,
    ) -> Self {
        Self {
            unit_index,
            root,
            edges,
            blocks: blocks.into_iter().collect(),
        }
    }

    pub fn unit_index(&self) -> usize {
        self.unit_index
    }

    pub fn root(&self) -> BlockId {
        self.root
    }

    pub fn contains_block(&self, id: BlockId) -> bool {
        self.blocks.contains(&id)
    }

    pub fn blocks(&self) -> impl Iterator<Item = &BlockId> {
        self.blocks.iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphNode {
    pub unit_index: usize,
    pub block_id: BlockId,
}

pub type UnitGraph = GraphUnit;

pub struct ProjectGraph {
    units: Vec<GraphUnit>,
}

impl ProjectGraph {
    pub fn new() -> Self {
        Self { units: Vec::new() }
    }

    pub fn add_child(&mut self, graph: GraphUnit) {
        self.units.push(graph);
    }

    pub fn link_units<'tcx>(&mut self, cc: &'tcx CompileCtxt<'tcx>) {
        if self.units.is_empty() {
            return;
        }

        let mut resolver = SymbolResolver::new(cc);

        let mut block_to_unit = HashMap::new();
        for (idx, unit) in self.units.iter().enumerate() {
            for block_id in unit.blocks() {
                block_to_unit.insert(*block_id, idx);
            }
        }

        let mut unresolved = cc.unresolve_symbols.borrow_mut();
        unresolved.retain(|symbol_ref| {
            let target = *symbol_ref;
            let Some(target_block) = target.block_id() else {
                return false;
            };

            let dependents: Vec<Id> = target.depended.borrow().clone();
            for dependent_id in dependents {
                let Some(source_symbol) = resolver.resolve(dependent_id) else {
                    continue;
                };
                let Some(from_block) = source_symbol.block_id() else {
                    continue;
                };
                self.add_dependency_edge(&block_to_unit, from_block, target_block);
            }

            false
        });
    }

    pub fn units(&self) -> &[GraphUnit] {
        &self.units
    }
}

#[derive(Debug)]
struct GraphBuilder<'tcx, Language> {
    unit: CompileUnit<'tcx>,
    root: Option<BlockId>,
    children_stack: Vec<Vec<BlockId>>,
    blocks: Vec<BlockId>,
    _marker: PhantomData<Language>,
}

impl<'tcx, Language: LanguageTrait> GraphBuilder<'tcx, Language> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            unit,
            root: None,
            children_stack: Vec::new(),
            blocks: Vec::new(),
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
                let block = BlockRoot::from_hir(id, node, parent, children);
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
            BlockKind::Impl => {
                todo!()
                // let block = BlockImpl::from_hir(unit, id, node, parent, children);
                // BasicBlock::Impl(arena.alloc(block))
            }
            _ => {
                panic!("unknown block kind: {}", kind)
            }
        }
    }

    fn block_ids(&self) -> &[BlockId] {
        &self.blocks
    }

    fn build_edges(&self, node: HirNode<'tcx>) -> BlockRelationMap {
        let edges = BlockRelationMap::default();
        let mut resolver = SymbolResolver::new(self.unit.cc);
        let mut visited = HashSet::new();
        let mut unresolved = HashSet::new();
        self.collect_edges(node, &edges, &mut resolver, &mut visited, &mut unresolved);
        edges
    }

    fn collect_edges(
        &self,
        node: HirNode<'tcx>,
        edges: &BlockRelationMap,
        resolver: &mut SymbolResolver<'tcx>,
        visited: &mut HashSet<Id>,
        unresolved: &mut HashSet<Id>,
    ) {
        if let Some(scope) = self.unit.opt_get_scope(node.hir_id()) {
            if let Some(symbol) = scope.symbol() {
                let symbol_id = symbol.id;
                if visited.insert(symbol_id) {
                    if let Some(from_block) = symbol.block_id() {
                        let deps = symbol.depends.borrow();
                        for dep_id in deps.iter().copied() {
                            if let Some(target_symbol) = resolver.resolve(dep_id) {
                                if let Some(to_block) = target_symbol.block_id() {
                                    if !edges.has_relation(
                                        from_block,
                                        BlockRelation::DependsOn,
                                        to_block,
                                    ) {
                                        edges.add_call_relationship(from_block, to_block);
                                    }
                                } else if unresolved.insert(dep_id) {
                                    self.unit.add_unresolved_symbol(target_symbol);
                                }
                            } else if unresolved.insert(dep_id) {
                                // Without a symbol reference we cannot link now.
                            }
                        }
                    }
                }
            }
        }

        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            self.collect_edges(child, edges, resolver, visited, unresolved);
        }
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

            let children = self.children_stack.pop().unwrap();
            children
        } else {
            Vec::new()
        };

        let block = self.create_block(id, node, block_kind, Some(parent), children);
        if let Some(scope) = self.unit.opt_get_scope(node.hir_id()) {
            if let Some(symbol) = scope.symbol() {
                // Set the block ID for the symbol
                symbol.set_block_id(Some(id));
            }
        }
        self.unit.insert_block(id, block, parent);
        self.blocks.push(id);

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
        if Language::block_kind(node.kind_id()) != BlockKind::Undefined {
            self.build_block(node, parent, false);
        } else {
            self.visit_children(node, parent);
        }
    }

    fn visit_scope(&mut self, node: HirNode<'tcx>, parent: BlockId) {
        match Language::block_kind(node.kind_id()) {
            BlockKind::Func | BlockKind::Class => self.build_block(node, parent, true),
            _ => self.visit_children(node, parent),
        }
    }
}

pub fn build_llmcc_graph<'tcx, L: LanguageTrait>(
    unit: CompileUnit<'tcx>,
    unit_index: usize,
) -> Result<GraphUnit, Box<dyn std::error::Error>> {
    let root_hir = unit
        .file_start_hir_id()
        .ok_or_else(|| "missing file start HIR id")?;
    let mut builder = GraphBuilder::<L>::new(unit);
    let root_node = unit.hir_node(root_hir);
    builder.visit_node(root_node, BlockId::ROOT_PARENT);

    let root_block = builder.root;
    let root_block = root_block.ok_or_else(|| "graph builder produced no root")?;
    let edges = builder.build_edges(root_node);
    let blocks = builder.block_ids().to_vec();
    Ok(GraphUnit::new(unit_index, root_block, edges, blocks))
}

impl ProjectGraph {
    fn add_dependency_edge(
        &self,
        block_to_unit: &HashMap<BlockId, usize>,
        from_block: BlockId,
        to_block: BlockId,
    ) {
        let Some(&from_idx) = block_to_unit.get(&from_block) else {
            return;
        };
        let Some(&to_idx) = block_to_unit.get(&to_block) else {
            return;
        };

        if from_idx == to_idx {
            let unit = &self.units[from_idx];
            if !unit
                .edges
                .has_relation(from_block, BlockRelation::DependsOn, to_block)
            {
                unit.edges.add_call_relationship(from_block, to_block);
            }
            return;
        }

        let from_unit = &self.units[from_idx];
        if !from_unit
            .edges
            .has_relation(from_block, BlockRelation::DependsOn, to_block)
        {
            from_unit
                .edges
                .add_relation(from_block, BlockRelation::DependsOn, to_block);
        }

        let to_unit = &self.units[to_idx];
        if !to_unit
            .edges
            .has_relation(to_block, BlockRelation::DependedBy, from_block)
        {
            to_unit
                .edges
                .add_relation(to_block, BlockRelation::DependedBy, from_block);
        }
    }
}

struct SymbolResolver<'tcx> {
    cc: &'tcx CompileCtxt<'tcx>,
    cache: HashMap<Id, &'tcx Symbol>,
}

impl<'tcx> SymbolResolver<'tcx> {
    fn new(cc: &'tcx CompileCtxt<'tcx>) -> Self {
        Self {
            cc,
            cache: HashMap::new(),
        }
    }

    fn resolve(&mut self, id: Id) -> Option<&'tcx Symbol> {
        if let Some(symbol) = self.cache.get(&id) {
            return Some(*symbol);
        }

        let scope_map = self.cc.scope_map.borrow();
        for scope in scope_map.values() {
            if let Some(symbol) = scope.symbol() {
                if symbol.id == id {
                    self.cache.insert(id, symbol);
                    return Some(symbol);
                }
            }

            for symbol in scope.all_symbols() {
                if symbol.id == id {
                    self.cache.insert(id, symbol);
                    return Some(symbol);
                }
            }
        }

        None
    }
}
