use std::{collections::HashMap, marker::PhantomData};
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::{Context, ParentedBlock};
use crate::ir::HirNode;
use crate::lang_def::LanguageTrait;
use crate::visit::HirVisitor;
use crate::{declare_arena, HirId};

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
    pub fn format_block(&self, ctx: Context<'blk>) -> String {
        let block_id = self.block_id();
        let hir_id = self.node().hir_id();
        let kind = self.kind();
        let mut f = format!("{}:{}", kind, block_id);

        if let Some(def) = ctx.opt_defs(hir_id) {
            f.push_str(&format!("   d:{}", def.format_compact()));
        } else if let Some(sym) = ctx.opt_uses(hir_id) {
            f.push_str(&format!("   u:{}", sym.format_compact()));
        }

        f
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
        _ctx: Context<'blk>,
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
        ctx: Context<'blk>,
        id: BlockId,
        node: HirNode<'blk>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Func, children);
        let name = ctx.defs(node.hir_id()).name.clone();
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
        _ctx: Context<'blk>,
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
        _ctx: Context<'blk>,
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
        ctx: Context<'blk>,
        id: BlockId,
        node: HirNode<'blk>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Class, children);
        let name = ctx.defs(node.hir_id()).name.clone();
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
        _ctx: Context<'blk>,
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
    ctx: Context<'tcx>,
    id: u32,
    bb_map: HashMap<BlockId, ParentedBlock<'tcx>>,
    children_stack: Vec<Vec<BlockId>>,
    ph: PhantomData<Language>,
}

impl<'tcx, Language: LanguageTrait> GraphBuilder<'tcx, Language> {
    fn new(ctx: Context<'tcx>) -> Self {
        Self {
            ctx,
            id: 0,
            bb_map: HashMap::new(),
            children_stack: Vec::new(),
            ph: PhantomData,
        }
    }

    fn ctx(&self) -> Context<'tcx> {
        self.ctx
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
        let ctx = self.ctx();
        let arena = &self.ctx.gcx.block_arena;
        match kind {
            BlockKind::Root => {
                let block = BlockRoot::from_hir(ctx, id, node, children);
                BasicBlock::Root(arena.alloc(block))
            }
            BlockKind::Func => {
                let block = BlockFunc::from_hir(ctx, id, node, children);
                BasicBlock::Func(arena.alloc(block))
            }
            BlockKind::Class => {
                let block = BlockClass::from_hir(ctx, id, node, children);
                BasicBlock::Class(arena.alloc(block))
            }
            BlockKind::Stmt => {
                let stmt = BlockStmt::from_hir(ctx, id, node, children);
                BasicBlock::Stmt(arena.alloc(stmt))
            }
            BlockKind::Call => {
                let stmt = BlockCall::from_hir(ctx, id, node, children);
                BasicBlock::Call(arena.alloc(stmt))
            }
            BlockKind::Impl => {
                todo!()
                // let block = BlockImpl::from_hir(ctx, id, node);
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

        let children = if recursive {
            self.children_stack.push(Vec::new());
            self.visit_children(node, id);

            let children = self.children_stack.pop().unwrap();
            children
        } else {
            Vec::new()
        };

        let block = self.create_block(id, node, block_kind, children);
        self.bb_map.insert(id, ParentedBlock::new(parent, block));
        self.children_stack.last_mut().unwrap().push(id);
    }
}

impl<'tcx, Language: LanguageTrait> HirVisitor<'tcx> for GraphBuilder<'tcx, Language> {
    fn ctx(&self) -> Context<'tcx> {
        self.ctx()
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
    root: HirId,
    ctx: Context<'tcx>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = GraphBuilder::<L>::new(ctx);
    let root = ctx.hir_node(root);
    builder.visit_node(root, BlockId(0));
    *ctx.bb_map.borrow_mut() = builder.bb_map;
    Ok(())
}
