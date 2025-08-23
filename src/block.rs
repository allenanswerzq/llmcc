use std::collections::HashMap;
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::Context;
use crate::declare_arena;
use crate::ir::HirNode;

declare_arena!([
    blk_root: BlockRoot<'tcx>,
    blk_func: BlockFunc<'tcx>,
    blk_class: BlockClass<'tcx>,
    blk_impl: BlockImpl<'tcx>,
]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, FromRepr, Display)]
#[strum(serialize_all = "snake_case")]
pub enum BlockKind {
    Undefined,
    Root,
    Func,
    Class,
    Impl,
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
    Class(&'blk BlockClass<'blk>),
    Impl(&'blk BlockImpl<'blk>),
}

impl<'blk> BasicBlock<'blk> {
    /// Get the base block information regardless of variant
    pub fn base(&self) -> Option<&BlockBase<'blk>> {
        match self {
            BasicBlock::Undefined => None,
            BasicBlock::Root(block) => Some(&block.base),
            BasicBlock::Func(block) => Some(&block.base),
            BasicBlock::Class(block) => Some(&block.base),
            BasicBlock::Impl(block) => Some(&block.base),
        }
    }

    /// Get the block ID
    pub fn id(&self) -> Option<BlockId> {
        self.base().map(|base| base.id)
    }

    /// Get the block kind
    pub fn kind(&self) -> BlockKind {
        self.base().map(|base| base.kind).unwrap_or_default()
    }

    /// Get the HIR node
    pub fn node(&self) -> Option<&HirNode<'blk>> {
        self.base().map(|base| &base.node)
    }

    /// Get the children block IDs
    pub fn children(&self) -> &[BlockId] {
        self.base()
            .map(|base| base.children.as_slice())
            .unwrap_or(&[])
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
        write!(f, "b{}", self.0)
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
    pub fn new(id: BlockId, node: HirNode<'blk>, kind: BlockKind) -> Self {
        Self {
            id,
            node,
            kind,
            children: Vec::new(),
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

    pub fn from_hir_node(id: BlockId, node: HirNode<'blk>) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Root);
        Self::new(base)
    }
}

#[derive(Debug, Clone)]
pub struct BlockFunc<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
}

impl<'blk> BlockFunc<'blk> {
    pub fn new(base: BlockBase<'blk>, name: String) -> Self {
        Self { base, name }
    }

    pub fn from_hir_node(id: BlockId, node: HirNode<'blk>, name: String) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Func);
        Self::new(base, name)
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

    pub fn from_hir_node(id: BlockId, node: HirNode<'blk>, name: String) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Class);
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

    pub fn from_hir_node(id: BlockId, node: HirNode<'blk>, target_class: BlockId) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Impl);
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
