use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::{CompileUnit, ParentedBlock};
use crate::declare_arena;
use crate::ir::{HirId, HirNode};
use crate::lang_def::LanguageTrait;
use crate::symbol::{Scope, Symbol};
use crate::visit::HirVisitor;

declare_arena!([
    blk_root: BlockRoot<'tcx>,
    blk_func: BlockFunc<'tcx>,
    blk_class: BlockClass<'tcx>,
    blk_impl: BlockImpl<'tcx>,
    blk_stmt: BlockStmt<'tcx>,
    blk_call: BlockCall<'tcx>,
]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, FromRepr, Display)]
#[strum(serialize_all = "snake_case")]
pub enum BlockKind {
    Undefined,
    Root,
    Func,
    Stmt,
    Call,
    Class,
    Impl,
    Scope,
}

impl Default for BlockKind {
    fn default() -> Self {
        BlockKind::Undefined
    }
}

#[derive(Debug, Clone)]
pub enum BasicBlock<'blk> {
    Undefined,
    Root(&'blk BlockRoot<'blk>),
    Func(&'blk BlockFunc<'blk>),
    Stmt(&'blk BlockStmt<'blk>),
    Call(&'blk BlockCall<'blk>),
    Class(&'blk BlockClass<'blk>),
    Impl(&'blk BlockImpl<'blk>),
    Block,
}

impl<'blk> BasicBlock<'blk> {
    pub fn format_block(&self, _unit: CompileUnit<'blk>) -> String {
        let block_id = self.block_id();
        let kind = self.kind();
        format!("{}:{}", kind, block_id)
    }

    /// Get the base block information regardless of variant
    pub fn base(&self) -> Option<&BlockBase<'blk>> {
        match self {
            BasicBlock::Undefined | BasicBlock::Block => None,
            BasicBlock::Root(block) => Some(&block.base),
            BasicBlock::Func(block) => Some(&block.base),
            BasicBlock::Class(block) => Some(&block.base),
            BasicBlock::Impl(block) => Some(&block.base),
            BasicBlock::Stmt(block) => Some(&block.base),
            BasicBlock::Call(block) => Some(&block.base),
        }
    }

    /// Get the block ID
    pub fn block_id(&self) -> BlockId {
        self.base().unwrap().id
    }

    /// Get the block kind
    pub fn kind(&self) -> BlockKind {
        self.base().map(|base| base.kind).unwrap_or_default()
    }

    /// Get the HIR node
    pub fn node(&self) -> &HirNode<'blk> {
        self.base().map(|base| &base.node).unwrap()
    }

    pub fn opt_node(&self) -> Option<&HirNode<'blk>> {
        self.base().map(|base| &base.node)
    }

    /// Get the children block IDs
    pub fn children(&self) -> &[BlockId] {
        self.base()
            .map(|base| base.children.as_slice())
            .unwrap_or(&[])
    }

    pub fn child_count(&self) -> usize {
        self.children().len()
    }

    /// Check if this is a specific kind of block
    pub fn is_kind(&self, kind: BlockKind) -> bool {
        self.kind() == kind
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, Default)]
pub struct BlockId(pub u32);

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl BlockId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }

    pub const ROOT_PARENT: BlockId = BlockId(u32::MAX);

    pub fn is_root_parent(self) -> bool {
        self.0 == u32::MAX
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, FromRepr, Display)]
#[strum(serialize_all = "snake_case")]
pub enum BlockRelation {
    Unknown,
    Calls,
    CalledBy,
    Contains,
    ContainedBy,
}

impl Default for BlockRelation {
    fn default() -> Self {
        BlockRelation::Unknown
    }
}

#[derive(Debug, Clone)]
pub struct UnitGraph<'tcx> {
    unit: CompileUnit<'tcx>,
    unit_index: usize,
    root: BlockId,
    hir_to_block: HashMap<HirId, BlockId>,
}

impl<'tcx> UnitGraph<'tcx> {
    pub fn new(
        unit: CompileUnit<'tcx>,
        root: BlockId,
        hir_to_block: HashMap<HirId, BlockId>,
    ) -> Self {
        Self {
            unit,
            unit_index: unit.index,
            root,
            hir_to_block,
        }
    }

    pub fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    pub fn unit_index(&self) -> usize {
        self.unit_index
    }

    pub fn root(&self) -> BlockId {
        self.root
    }

    pub fn block_for_hir(&self, hir_id: HirId) -> Option<BlockId> {
        self.hir_to_block.get(&hir_id).copied()
    }

    pub fn hir_mappings(&self) -> &HashMap<HirId, BlockId> {
        &self.hir_to_block
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphNode {
    pub unit_index: usize,
    pub block_id: BlockId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrossUnitEdge {
    pub from: GraphNode,
    pub to: GraphNode,
    pub relation: BlockRelation,
}

pub struct ProjectGraph<'tcx> {
    globals: &'tcx Scope<'tcx>,
    units: Vec<UnitGraph<'tcx>>,
    hir_index: HashMap<HirId, GraphNode>,
    cross_unit_edges: Vec<CrossUnitEdge>,
}

impl<'tcx> ProjectGraph<'tcx> {
    pub fn new(globals: &'tcx Scope<'tcx>) -> Self {
        Self {
            globals,
            units: Vec::new(),
            hir_index: HashMap::new(),
            cross_unit_edges: Vec::new(),
        }
    }

    pub fn add_child(&mut self, graph: UnitGraph<'tcx>) {
        self.register_unit(&graph);
        self.units.push(graph);
    }

    fn register_unit(&mut self, graph: &UnitGraph<'tcx>) {
        for (hir_id, block_id) in graph.hir_mappings() {
            self.hir_index.insert(
                *hir_id,
                GraphNode {
                    unit_index: graph.unit_index(),
                    block_id: *block_id,
                },
            );
        }
    }

    fn node_for_symbol(&self, symbol: &Symbol) -> Option<GraphNode> {
        let owner = symbol.owner();
        self.hir_index.get(&owner).copied()
    }

    pub fn link_units(&mut self) {
        let mut symbols_by_id = HashMap::new();
        for symbol in self.globals.all_symbols() {
            symbols_by_id.insert(symbol.id, symbol);
        }

        let mut edges = HashSet::new();
        for symbol in symbols_by_id.values() {
            let Some(from_node) = self.node_for_symbol(symbol) else {
                continue;
            };

            for dependency in symbol.depends.borrow().iter() {
                let Some(target_symbol) = symbols_by_id.get(dependency) else {
                    continue;
                };
                let Some(to_node) = self.node_for_symbol(target_symbol) else {
                    continue;
                };

                if from_node.unit_index == to_node.unit_index {
                    continue;
                }

                edges.insert((from_node, to_node, BlockRelation::Calls));
            }
        }

        self.cross_unit_edges = edges
            .into_iter()
            .map(|(from, to, relation)| CrossUnitEdge { from, to, relation })
            .collect();

        self.cross_unit_edges.sort_by(|lhs, rhs| {
            (
                lhs.from.unit_index,
                lhs.from.block_id.as_u32(),
                lhs.to.unit_index,
                lhs.to.block_id.as_u32(),
            )
                .cmp(&(
                    rhs.from.unit_index,
                    rhs.from.block_id.as_u32(),
                    rhs.to.unit_index,
                    rhs.to.block_id.as_u32(),
                ))
        });
    }

    pub fn cross_unit_edges(&self) -> &[CrossUnitEdge] {
        &self.cross_unit_edges
    }

    pub fn units(&self) -> &[UnitGraph<'tcx>] {
        &self.units
    }
}

#[derive(Debug, Clone)]
pub struct BlockBase<'blk> {
    pub id: BlockId,
    pub node: HirNode<'blk>,
    pub kind: BlockKind,
    pub children: Vec<BlockId>,
}

impl<'blk> BlockBase<'blk> {
    pub fn new(id: BlockId, node: HirNode<'blk>, kind: BlockKind, children: Vec<BlockId>) -> Self {
        Self {
            id,
            node,
            kind,
            children,
        }
    }

    pub fn add_child(&mut self, child_id: BlockId) {
        if !self.children.contains(&child_id) {
            self.children.push(child_id);
        }
    }

    pub fn remove_child(&mut self, child_id: BlockId) {
        self.children.retain(|&id| id != child_id);
    }
}

#[derive(Debug, Clone)]
pub struct BlockRoot<'blk> {
    pub base: BlockBase<'blk>,
}

impl<'blk> BlockRoot<'blk> {
    pub fn new(base: BlockBase<'blk>) -> Self {
        Self { base }
    }

    pub fn from_hir(
        _ctx: CompileUnit<'blk>,
        id: BlockId,
        node: HirNode<'blk>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Root, children);
        Self::new(base)
    }
}

#[derive(Debug, Clone)]
pub struct BlockFunc<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
    pub parameters: Option<BlockId>,
    pub returns: Option<BlockId>,
    pub stmts: Option<Vec<BlockId>>,
}

impl<'blk> BlockFunc<'blk> {
    pub fn new(base: BlockBase<'blk>, name: String) -> Self {
        Self {
            base,
            name,
            parameters: None,
            returns: None,
            stmts: None,
        }
    }

    pub fn from_hir(
        _unit: CompileUnit<'blk>,
        id: BlockId,
        node: HirNode<'blk>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Func, children);
        // let name = "aaa".to_string();
        let name = "aaaa".to_string();
        Self::new(base, name)
    }
}

#[derive(Debug, Clone)]
pub struct BlockStmt<'blk> {
    pub base: BlockBase<'blk>,
}

impl<'blk> BlockStmt<'blk> {
    pub fn new(base: BlockBase<'blk>) -> Self {
        Self { base }
    }

    pub fn from_hir(
        _ctx: CompileUnit<'blk>,
        id: BlockId,
        node: HirNode<'blk>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Stmt, children);
        Self::new(base)
    }
}

#[derive(Debug, Clone)]
pub struct BlockCall<'blk> {
    pub base: BlockBase<'blk>,
}

impl<'blk> BlockCall<'blk> {
    pub fn new(base: BlockBase<'blk>) -> Self {
        Self { base }
    }

    pub fn from_hir(
        _ctx: CompileUnit<'blk>,
        id: BlockId,
        node: HirNode<'blk>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Call, children);
        Self::new(base)
    }
}

#[derive(Debug, Clone)]
pub struct BlockClass<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
    pub fields: Vec<BlockId>,
    pub methods: Vec<BlockId>,
}

impl<'blk> BlockClass<'blk> {
    pub fn new(base: BlockBase<'blk>, name: String) -> Self {
        Self {
            base,
            name,
            fields: Vec::new(),
            methods: Vec::new(),
        }
    }

    pub fn from_hir(
        _unit: CompileUnit<'blk>,
        id: BlockId,
        node: HirNode<'blk>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Class, children);
        let name = "aaa".to_string();
        Self::new(base, name)
    }

    pub fn add_field(&mut self, field_id: BlockId) {
        self.fields.push(field_id);
    }

    pub fn add_method(&mut self, method_id: BlockId) {
        self.methods.push(method_id);
    }
}

#[derive(Debug, Clone)]
pub struct BlockImpl<'blk> {
    pub base: BlockBase<'blk>,
    pub target_class: BlockId,
    pub trait_ref: Option<BlockId>,
    pub methods: Vec<BlockId>,
}

impl<'blk> BlockImpl<'blk> {
    pub fn new(base: BlockBase<'blk>, target_class: BlockId) -> Self {
        Self {
            base,
            target_class,
            trait_ref: None,
            methods: Vec::new(),
        }
    }

    pub fn from_hir(
        _ctx: CompileUnit<'blk>,
        id: BlockId,
        node: HirNode<'blk>,
        children: Vec<BlockId>,
        target_class: BlockId,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Impl, children);
        Self::new(base, target_class)
    }

    pub fn with_trait(mut self, trait_id: BlockId) -> Self {
        self.trait_ref = Some(trait_id);
        self
    }

    pub fn add_method(&mut self, method_id: BlockId) {
        self.methods.push(method_id);
    }
}

#[derive(Debug)]
struct GraphBuilder<'tcx, Language> {
    unit: CompileUnit<'tcx>,
    id: u32,
    bb_map: HashMap<BlockId, ParentedBlock<'tcx>>,
    hir_to_block: HashMap<HirId, BlockId>,
    root: Option<BlockId>,
    children_stack: Vec<Vec<BlockId>>,
    ph: PhantomData<Language>,
}

impl<'tcx, Language: LanguageTrait> GraphBuilder<'tcx, Language> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            unit,
            id: 0,
            bb_map: HashMap::new(),
            hir_to_block: HashMap::new(),
            root: None,
            children_stack: Vec::new(),
            ph: PhantomData,
        }
    }

    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn next_id(&mut self) -> BlockId {
        let ans = BlockId(self.id);
        self.id += 1;
        ans
    }

    fn create_block(
        &self,
        id: BlockId,
        node: HirNode<'tcx>,
        kind: BlockKind,
        children: Vec<BlockId>,
    ) -> BasicBlock<'tcx> {
        let unit = self.unit();
        let arena = &self.unit.cc.block_arena;
        match kind {
            BlockKind::Root => {
                let block = BlockRoot::from_hir(unit, id, node, children);
                BasicBlock::Root(arena.alloc(block))
            }
            BlockKind::Func => {
                let block = BlockFunc::from_hir(unit, id, node, children);
                BasicBlock::Func(arena.alloc(block))
            }
            BlockKind::Class => {
                let block = BlockClass::from_hir(unit, id, node, children);
                BasicBlock::Class(arena.alloc(block))
            }
            BlockKind::Stmt => {
                let stmt = BlockStmt::from_hir(unit, id, node, children);
                BasicBlock::Stmt(arena.alloc(stmt))
            }
            BlockKind::Call => {
                let stmt = BlockCall::from_hir(unit, id, node, children);
                BasicBlock::Call(arena.alloc(stmt))
            }
            BlockKind::Impl => {
                todo!()
                // let block = BlockImpl::from_hir(unit, id, node);
                // BasicBlock::Impl(arena.alloc(block))
            }
            _ => {
                panic!("unknown block kind: {}", kind)
            }
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

        let block = self.create_block(id, node, block_kind, children);
        self.hir_to_block.insert(node.hir_id(), id);
        self.bb_map.insert(id, ParentedBlock::new(parent, block));
        if !parent.is_root_parent() {
            self.unit
                .cc
                .related_map
                .add_containment_relationship(parent, id);
        }
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
        if Language::block_kind(node.kind_id()) == BlockKind::Func {
            self.build_block(node, parent, true);
        } else {
            self.visit_children(node, parent);
        }
    }
}

pub fn build_llmcc_graph<'tcx, L: LanguageTrait>(
    unit: CompileUnit<'tcx>,
) -> Result<UnitGraph<'tcx>, Box<dyn std::error::Error>> {
    let root_hir = unit
        .file_start_hir_id()
        .ok_or_else(|| "missing file start HIR id")?;
    let mut builder = GraphBuilder::<L>::new(unit);
    let root_node = unit.hir_node(root_hir);
    builder.visit_node(root_node, BlockId::ROOT_PARENT);

    let GraphBuilder {
        bb_map,
        hir_to_block,
        root,
        ..
    } = builder;

    *unit.bb_map.borrow_mut() = bb_map;

    let root_block = root.ok_or_else(|| "graph builder produced no root")?;
    Ok(UnitGraph::new(unit, root_block, hir_to_block))
}
