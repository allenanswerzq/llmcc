use std::collections::HashSet;
use std::marker::PhantomData;

pub use crate::block::{BasicBlock, BlockId, BlockKind, BlockRelation};
use crate::block::{BlockCall, BlockClass, BlockConst, BlockEnum, BlockFunc, BlockRoot, BlockStmt};
use crate::block_rel::BlockRelationMap;
use crate::context::{CompileCtxt, CompileUnit};
use crate::ir::HirNode;
use crate::lang_def::LanguageTrait;
use crate::symbol::{SymId, Symbol};
use crate::visit::HirVisitor;

#[derive(Debug, Clone)]
pub struct GraphUnit {
    /// Compile unit this graph belongs to
    unit_index: usize,
    /// Root block ID of this unit
    root: BlockId,
    /// Edges of this graph unit
    edges: BlockRelationMap,
}

impl GraphUnit {
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

pub type UnitGraph = GraphUnit;

pub struct ProjectGraph<'tcx> {
    pub cc: &'tcx CompileCtxt<'tcx>,
    units: Vec<GraphUnit>,
}

impl<'tcx> ProjectGraph<'tcx> {
    pub fn new(cc: &'tcx CompileCtxt<'tcx>) -> Self {
        Self { cc, units: Vec::new() }
    }

    pub fn add_child(&mut self, graph: GraphUnit) {
        self.units.push(graph);
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

    pub fn units(&self) -> &[GraphUnit] {
        &self.units
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
        if !from_unit
            .edges
            .has_relation(from_block, BlockRelation::DependsOn, to_block)
        {
            from_unit.edges.add_relation(from_block, to_block);
        }

        let to_unit = &self.units[to_idx];
        if !to_unit
            .edges
            .has_relation(to_block, BlockRelation::DependedBy, from_block)
        {
            to_unit.edges.add_relation(to_block, from_block);
        }
    }
}

#[derive(Debug)]
struct GraphBuilder<'tcx, Language> {
    unit: CompileUnit<'tcx>,
    root: Option<BlockId>,
    children_stack: Vec<Vec<BlockId>>,
    _marker: PhantomData<Language>,
}

impl<'tcx, Language: LanguageTrait> GraphBuilder<'tcx, Language> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            unit,
            root: None,
            children_stack: Vec::new(),
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
            BlockKind::Enum => {
                let enum_ty = BlockEnum::from_hir(id, node, parent, children);
                BasicBlock::Enum(arena.alloc(enum_ty))
            }
            BlockKind::Const => {
                let stmt = BlockConst::from_hir(id, node, parent, children);
                BasicBlock::Const(arena.alloc(stmt))
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
        if let Some(symbol) = self
            .unit
            .opt_get_scope(node.hir_id())
            .and_then(|scope| scope.symbol())
        {
            self.process_symbol(symbol, edges, visited, unresolved);
        }

        // Recurse into children
        for &child_id in node.children() {
            let child = self.unit.hir_node(child_id);
            self.collect_edges(child, edges, visited, unresolved);
        }
    }

    fn process_symbol(
        &self,
        symbol: &Symbol,
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
                if target_unit.is_some() && target_unit != Some(self.unit.index) {
                    if unresolved.insert(dep_id) {
                        self.unit.add_unresolved_symbol(target_symbol);
                    }
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
    Ok(GraphUnit::new(unit_index, root_block, edges))
}
