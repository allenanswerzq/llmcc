use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::{CompileCtxt, CompileUnit, ParentedBlock};
use crate::declare_arena;
use crate::ir::{HirId, HirNode};
use crate::lang_def::LanguageTrait;
use crate::symbol::Symbol;
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
    DependedBy,
    DependsOn,
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
    pub parent: Option<BlockId>,
    pub children: Vec<BlockId>,
}

impl<'blk> BlockBase<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        kind: BlockKind,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self {
            id,
            node,
            kind,
            parent,
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
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Root, parent, children);
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
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Func, parent, children);
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
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Stmt, parent, children);
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
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Call, parent, children);
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
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Class, parent, children);
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
        id: BlockId,
        node: HirNode<'blk>,
        children: Vec<BlockId>,
        parent: Option<BlockId>,
        target_class: BlockId,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Impl, parent, children);
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
