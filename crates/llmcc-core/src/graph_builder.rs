use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::{CompileCtxt, CompileUnit, ParentedBlock};
use crate::declare_arena;
use crate::ir::{HirId, HirNode};
use crate::lang_def::LanguageTrait;
use crate::symbol::Symbol;
use crate::visit::HirVisitor;
use crate::block::{BlockRoot, BlockFunc, BlockClass, BlockImpl, BlockStmt, BlockCall, BlockId, BlockRelation};


#[derive(Debug, Clone)]
pub struct GraphUnit {
    /// Compile unit this graph belongs to
    unit_index: usize,
    /// Root block ID of this unit
    root: BlockId,
    /// Edges of this graph unit
    edges: HashMap<BlockId, Vec<(BlockId, BlockRelation)>>,
}

impl GraphUnit {
    pub fn new(
        unit_index: usize,
        root: BlockId,
    ) -> Self {
        Self {
            unit_index,
            root,
            edges: HashMap::new(),
        }
    }

    pub fn unit_index(&self) -> usize {
        self.unit_index
    }

    pub fn root(&self) -> BlockId {
        self.root
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphNode {
    pub unit_index: usize,
    pub block_id: BlockId,
}

#[derive(Debug, Clone)]
pub struct PendingEdge<'tcx> {
    pub from: GraphNode,
    pub to_symbol: &'tcx Symbol,
    pub relation: BlockRelation,
}

pub struct ProjectGraph<'tcx> {
    units: Vec<GraphUnit<'tcx>>,
}

impl<'tcx> ProjectGraph<'tcx> {
    pub fn new() -> Self {
        Self {
            units: Vec::new(),
        }
    }

    pub fn add_child(&mut self, graph: GraphUnit<'tcx>) {
        self.units.push(graph);
    }

    pub fn link_units(&mut self) {
    }

    pub fn units(&self) -> &[GraphUnit<'tcx>] {
        &self.units
    }
}


#[derive(Debug)]
struct GraphBuilder<'tcx, Language> {
    unit: CompileUnit<'tcx>,
    root: Option<BlockId>,
    children_stack: Vec<Vec<BlockId>>,
    ph: PhantomData<Language>,
}

impl<'tcx, Language: LanguageTrait> GraphBuilder<'tcx, Language> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            unit,
            hir_to_block: HashMap::new(),
            root: None,
            children_stack: Vec::new(),
            ph: PhantomData,
        }
    }

    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
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

    fn build_edges(&self) -> Vec<PendingEdge<'tcx>> {
        // TODO: implement this function
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

        let block = self.create_block(id, node, block_kind, children);
        self.hir_to_block.insert(node.hir_id(), id);
        if let Some(scope) = self.unit.opt_get_scope(node.hir_id()) {
            if let Some(symbol) = scope.symbol() {
                symbol.set_block_id(Some(id));
            }
        }
        self
            .unit
            .cc
            .bb_map
            .borrow_mut()
            .insert(id, ParentedBlock::new(parent, block));
        if let Some(children) = self.children_stack.last_mut() {
            children.push(id);
        }
    }
}

impl<'tcx, Language: LanguageTrait> HirVisitor<'tcx> for GraphBuilder<'tcx, Language> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit()
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
) -> Result<GraphUnit<'tcx>, Box<dyn std::error::Error>> {
    let root_hir = unit
        .file_start_hir_id()
        .ok_or_else(|| "missing file start HIR id")?;
    let mut builder = GraphBuilder::<L>::new(unit);
    let root_node = unit.hir_node(root_hir);
    builder.visit_node(root_node, BlockId::ROOT_PARENT);

    let GraphBuilder {
        hir_to_block,
        root,
        ..
    } = builder;

    let root_block = root.ok_or_else(|| "graph builder produced no root")?;
    builder.build_edges();
    Ok(GraphUnit::new(unit, root_block, hir_to_block))
}
