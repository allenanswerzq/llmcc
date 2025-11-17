use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::CompileUnit;
use crate::declare_arena;
use crate::ir::HirNode;

declare_arena!([
    blk_root: BlockRoot<'tcx>,
    blk_func: BlockFunc<'tcx>,
    blk_method: BlockMethod<'tcx>,
    blk_class: BlockClass<'tcx>,
    blk_impl: BlockImpl<'tcx>,
    blk_stmt: BlockStmt<'tcx>,
    blk_call: BlockCall<'tcx>,
    blk_enum: BlockEnum<'tcx>,
    blk_field: BlockField<'tcx>,
    blk_const: BlockConst<'tcx>,
]);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, FromRepr, Display, Default,
)]
#[strum(serialize_all = "snake_case")]
pub enum BlockKind {
    #[default]
    Undefined,
    Root,
    Func,
    Method,
    Stmt,
    Call,
    Class,
    Enum,
    Const,
    Impl,
    Field,
    Scope,
}

#[derive(Debug, Clone)]
pub enum BasicBlock<'blk> {
    Undefined,
    Root(&'blk BlockRoot<'blk>),
    Func(&'blk BlockFunc<'blk>),
    Method(&'blk BlockMethod<'blk>),
    Stmt(&'blk BlockStmt<'blk>),
    Call(&'blk BlockCall<'blk>),
    Enum(&'blk BlockEnum<'blk>),
    Class(&'blk BlockClass<'blk>),
    Impl(&'blk BlockImpl<'blk>),
    Const(&'blk BlockConst<'blk>),
    Field(&'blk BlockField<'blk>),
    Block,
}

impl<'blk> BasicBlock<'blk> {
    pub fn format_block(&self, _unit: CompileUnit<'blk>) -> String {
        let block_id = self.block_id();
        let kind = self.kind();
        let name = self
            .base()
            .and_then(|base| base.opt_get_name())
            .unwrap_or("");

        // Include file_name for Root blocks
        if let BasicBlock::Root(root) = self
            && let Some(file_name) = &root.file_name
        {
            return format!("{}:{} {} ({})", kind, block_id, name, file_name);
        }

        format!("{}:{} {}", kind, block_id, name)
    }

    /// Get the base block information regardless of variant
    pub fn base(&self) -> Option<&BlockBase<'blk>> {
        match self {
            BasicBlock::Undefined | BasicBlock::Block => None,
            BasicBlock::Root(block) => Some(&block.base),
            BasicBlock::Func(block) => Some(&block.base),
            BasicBlock::Method(block) => Some(&block.base),
            BasicBlock::Class(block) => Some(&block.base),
            BasicBlock::Impl(block) => Some(&block.base),
            BasicBlock::Stmt(block) => Some(&block.base),
            BasicBlock::Call(block) => Some(&block.base),
            BasicBlock::Enum(block) => Some(&block.base),
            BasicBlock::Const(block) => Some(&block.base),
            BasicBlock::Field(block) => Some(&block.base),
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, FromRepr, Display, Default,
)]
#[strum(serialize_all = "snake_case")]
pub enum BlockRelation {
    #[default]
    Unknown,
    DependedBy,
    DependsOn,
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

    pub fn opt_get_name(&self) -> Option<&str> {
        self.node
            .as_scope()
            .and_then(|scope| *scope.ident.read())
            .map(|ident| ident.name.as_str())
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
    pub file_name: Option<String>,
}

impl<'blk> BlockRoot<'blk> {
    pub fn new(base: BlockBase<'blk>, file_name: Option<String>) -> Self {
        Self { base, file_name }
    }

    pub fn from_hir(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        file_name: Option<String>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Root, parent, children);
        Self::new(base, file_name)
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
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self::new(base, name)
    }
}

#[derive(Debug, Clone)]
pub struct BlockMethod<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
}

impl<'blk> BlockMethod<'blk> {
    pub fn new(base: BlockBase<'blk>, name: String) -> Self {
        Self { base, name }
    }

    pub fn from_hir(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Method, parent, children);
        let name = base.opt_get_name().unwrap_or("").to_string();
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
}

impl<'blk> BlockClass<'blk> {
    pub fn new(base: BlockBase<'blk>, name: String) -> Self {
        Self { base, name }
    }

    pub fn from_hir(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Class, parent, children);
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self::new(base, name)
    }
}

#[derive(Debug, Clone)]
pub struct BlockImpl<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
}

impl<'blk> BlockImpl<'blk> {
    pub fn new(base: BlockBase<'blk>, name: String) -> Self {
        Self { base, name }
    }

    pub fn from_hir(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Impl, parent, children);
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self::new(base, name)
    }
}

#[derive(Debug, Clone)]
pub struct BlockEnum<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
    pub fields: Vec<BlockId>,
}

impl<'blk> BlockEnum<'blk> {
    pub fn new(base: BlockBase<'blk>, name: String) -> Self {
        Self {
            base,
            name,
            fields: Vec::new(),
        }
    }

    pub fn from_hir(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Enum, parent, children);
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self::new(base, name)
    }

    pub fn add_field(&mut self, field_id: BlockId) {
        self.fields.push(field_id);
    }
}

#[derive(Debug, Clone)]
pub struct BlockConst<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
}

impl<'blk> BlockConst<'blk> {
    pub fn new(base: BlockBase<'blk>, name: String) -> Self {
        Self { base, name }
    }

    pub fn from_hir(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Const, parent, children);
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self::new(base, name)
    }
}

#[derive(Debug, Clone)]
pub struct BlockField<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
}

impl<'blk> BlockField<'blk> {
    pub fn new(base: BlockBase<'blk>, name: String) -> Self {
        Self { base, name }
    }

    pub fn from_hir(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Field, parent, children);
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self::new(base, name)
    }
}
