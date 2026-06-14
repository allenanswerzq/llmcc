//! Basic block representation for code graph.

use parking_lot::RwLock;
use std::collections::HashSet;
use std::fmt;
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::CompileUnit;
use crate::declare_arena;
pub use crate::id::{BlockId, reset_block_id_counter};
use crate::ir::HirNode;
use crate::scope::Scope;
use crate::symbol::{SymId, SymKind, Symbol};

declare_arena!(BlockArena {
    bb: BasicBlock<'a>,
    blk_root: BlockRoot<'a>,
    blk_module: BlockModule<'a>,
    blk_func: BlockFunc<'a>,
    blk_class: BlockClass<'a>,
    blk_trait: BlockTrait<'a>,
    blk_interface: BlockInterface<'a>,
    blk_impl: BlockImpl<'a>,
    blk_call: BlockCall<'a>,
    blk_enum: BlockEnum<'a>,
    blk_field: BlockField<'a>,
    blk_const: BlockConst<'a>,
    blk_parameter: BlockParameter<'a>,
    blk_return: BlockReturn<'a>,
    blk_alias: BlockAlias<'a>,
});

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, FromRepr, Display, Default,
)]
#[strum(serialize_all = "snake_case")]
/// Normalized block categories used by the language-agnostic graph.
///
/// These names are shared graph vocabulary, not a checklist every language must
/// emit. A language should materialize only the kinds that fit its syntax and
/// semantics, and can add a new kind when an existing category would be
/// misleading. For example, `Class` is the current nominal/aggregate type bucket
/// used by classes, structs, and records; `Trait` and `Interface` are two
/// contract-like categories for languages that distinguish those concepts; and
/// `Impl` models an implementation/extension/conformance block.
pub enum BlockKind {
    #[default]
    Undefined,
    Root,
    Module,
    Func,
    Method,
    Closure,
    Call,
    Class,
    Trait,
    Interface,
    Enum,
    Const,
    Impl,
    Field,
    Scope,
    Parameter,
    Return,
    Alias,
}

impl BlockKind {
    /// Return the default PageRank teleport prior for this block kind.
    pub const fn pagerank_prior(self) -> f64 {
        match self {
            BlockKind::Root => 0.2,
            BlockKind::Module => 0.6,
            BlockKind::Class | BlockKind::Trait | BlockKind::Interface => 1.0,
            BlockKind::Func | BlockKind::Method => 1.0,
            BlockKind::Impl | BlockKind::Enum => 0.8,
            BlockKind::Alias => 0.6,
            BlockKind::Const => 0.4,
            BlockKind::Field => 0.3,
            BlockKind::Parameter | BlockKind::Return => 0.2,
            BlockKind::Call | BlockKind::Scope => 0.1,
            BlockKind::Undefined => 0.05,
            BlockKind::Closure => 1.0,
        }
    }

    /// Return all default PageRank teleport priors.
    pub fn pagerank_priors() -> Vec<(BlockKind, f64)> {
        Self::iter()
            .map(|kind| (kind, kind.pagerank_prior()))
            .collect()
    }

    /// Return true when this kind has a concrete [`BasicBlock`] representation.
    pub fn is_materialized(self) -> bool {
        matches!(
            self,
            BlockKind::Root
                | BlockKind::Module
                | BlockKind::Func
                | BlockKind::Method
                | BlockKind::Call
                | BlockKind::Class
                | BlockKind::Trait
                | BlockKind::Interface
                | BlockKind::Enum
                | BlockKind::Const
                | BlockKind::Impl
                | BlockKind::Field
                | BlockKind::Parameter
                | BlockKind::Return
                | BlockKind::Alias
        )
    }

    /// Return true when this block kind owns the primary symbol's block id.
    pub fn owns_symbol_block_id(self) -> bool {
        self.is_materialized() && !matches!(self, BlockKind::Impl | BlockKind::Return)
    }

    /// Return true when graph building should require a scope symbol for this kind.
    pub fn requires_scope_symbol(self) -> bool {
        matches!(self, BlockKind::Func | BlockKind::Method)
    }
}

/// Concrete block stored in the block graph.
///
/// Each variant wraps a block-specific payload, but every variant has a
/// [`BlockBase`] for common identity, source, hierarchy, and symbol metadata.
#[derive(Debug, Clone)]
pub enum BasicBlock<'blk> {
    Root(&'blk BlockRoot<'blk>),
    Module(&'blk BlockModule<'blk>),
    Func(&'blk BlockFunc<'blk>),
    Call(&'blk BlockCall<'blk>),
    Enum(&'blk BlockEnum<'blk>),
    Class(&'blk BlockClass<'blk>),
    Trait(&'blk BlockTrait<'blk>),
    Interface(&'blk BlockInterface<'blk>),
    Impl(&'blk BlockImpl<'blk>),
    Const(&'blk BlockConst<'blk>),
    Field(&'blk BlockField<'blk>),
    Parameter(&'blk BlockParameter<'blk>),
    Return(&'blk BlockReturn<'blk>),
    Alias(&'blk BlockAlias<'blk>),
}

impl<'blk> BasicBlock<'blk> {
    /// Dependency labels rendered as pseudo-children.
    pub fn dependency_labels(&self, unit: CompileUnit<'blk>) -> Vec<String> {
        match self {
            BasicBlock::Func(func) => func.dependency_labels(unit),
            BasicBlock::Class(class) => class.dependency_labels(unit),
            _ => Vec::new(),
        }
    }

    pub(crate) fn attach_child_blocks(&self, children: &[(BlockId, BlockKind)]) {
        for &(child_id, child_kind) in children {
            self.attach_child_block(child_id, child_kind);
        }
    }

    fn attach_child_block(&self, child_id: BlockId, child_kind: BlockKind) {
        match self {
            BasicBlock::Func(func) => match child_kind {
                BlockKind::Parameter => func.add_parameter(child_id),
                BlockKind::Return => func.set_return(child_id),
                _ => {}
            },
            BasicBlock::Class(class) => match child_kind {
                BlockKind::Field => class.add_field(child_id),
                BlockKind::Func | BlockKind::Method => class.add_method(child_id),
                _ => {}
            },
            BasicBlock::Enum(enum_block) => {
                if child_kind == BlockKind::Field {
                    enum_block.add_variant(child_id);
                }
            }
            BasicBlock::Trait(trait_block) => {
                if matches!(child_kind, BlockKind::Func | BlockKind::Method) {
                    trait_block.add_method(child_id);
                }
            }
            BasicBlock::Interface(iface_block) => match child_kind {
                BlockKind::Field => iface_block.add_field(child_id),
                BlockKind::Func | BlockKind::Method => iface_block.add_method(child_id),
                _ => {}
            },
            BasicBlock::Impl(impl_block) => {
                if matches!(child_kind, BlockKind::Func | BlockKind::Method) {
                    impl_block.add_method(child_id);
                }
            }
            _ => {}
        }
    }
}

impl<'blk> fmt::Display for BasicBlock<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BasicBlock::Root(block) => block.fmt(f),
            BasicBlock::Module(block) => block.fmt(f),
            BasicBlock::Func(block) => block.fmt(f),
            BasicBlock::Call(block) => block.fmt(f),
            BasicBlock::Enum(block) => block.fmt(f),
            BasicBlock::Class(block) => block.fmt(f),
            BasicBlock::Trait(block) => block.fmt(f),
            BasicBlock::Interface(block) => block.fmt(f),
            BasicBlock::Impl(block) => block.fmt(f),
            BasicBlock::Const(block) => block.fmt(f),
            BasicBlock::Field(block) => block.fmt(f),
            BasicBlock::Parameter(block) => block.fmt(f),
            BasicBlock::Return(block) => block.fmt(f),
            BasicBlock::Alias(block) => block.fmt(f),
        }
    }
}

impl<'blk> BasicBlock<'blk> {
    pub fn id(&self) -> BlockId {
        self.block_id()
    }

    /// Get the base block information regardless of variant
    pub fn base(&self) -> &BlockBase<'blk> {
        match self {
            BasicBlock::Root(block) => &block.base,
            BasicBlock::Module(block) => &block.base,
            BasicBlock::Func(block) => &block.base,
            BasicBlock::Class(block) => &block.base,
            BasicBlock::Trait(block) => &block.base,
            BasicBlock::Interface(block) => &block.base,
            BasicBlock::Impl(block) => &block.base,
            BasicBlock::Call(block) => &block.base,
            BasicBlock::Enum(block) => &block.base,
            BasicBlock::Const(block) => &block.base,
            BasicBlock::Field(block) => &block.base,
            BasicBlock::Parameter(block) => &block.base,
            BasicBlock::Return(block) => &block.base,
            BasicBlock::Alias(block) => &block.base,
        }
    }

    /// Get the block ID
    pub fn block_id(&self) -> BlockId {
        self.base().id
    }

    /// Get the block kind
    pub fn kind(&self) -> BlockKind {
        self.base().kind
    }

    /// Get the HIR node
    pub fn node(&self) -> &HirNode<'blk> {
        &self.base().node
    }

    /// Get the symbol that defines this block (if any)
    pub fn symbol(&self) -> Option<&'blk Symbol> {
        self.base().symbol()
    }

    /// Return the block's semantic name, if it has one.
    pub fn try_name(&self) -> Option<&str> {
        match self {
            BasicBlock::Root(block) => block.base.try_name(),
            BasicBlock::Module(block) => block.name(),
            BasicBlock::Func(block) => block.name(),
            BasicBlock::Call(block) => block.base.try_name(),
            BasicBlock::Enum(block) => block.name(),
            BasicBlock::Class(block) => block.name(),
            BasicBlock::Trait(block) => block.name(),
            BasicBlock::Interface(block) => block.name(),
            BasicBlock::Impl(block) => block.name(),
            BasicBlock::Const(block) => block.name(),
            BasicBlock::Field(block) => block.name(),
            BasicBlock::Parameter(block) => block.name(),
            BasicBlock::Return(block) => block.base.try_name(),
            BasicBlock::Alias(block) => block.name(),
        }
    }

    /// Get the children block IDs
    pub fn children(&self) -> Vec<BlockId> {
        self.base().children()
    }

    pub fn child_count(&self) -> usize {
        self.children().len()
    }

    /// Check if this is a specific kind of block
    pub fn is_kind(&self, kind: BlockKind) -> bool {
        self.kind() == kind
    }

    /// Get the inner BlockRoot if this is a Root block
    pub fn as_root(&self) -> Option<&'blk BlockRoot<'blk>> {
        match self {
            BasicBlock::Root(r) => Some(r),
            _ => None,
        }
    }

    /// Get the inner BlockModule if this is a Module block
    pub fn as_module(&self) -> Option<&'blk BlockModule<'blk>> {
        match self {
            BasicBlock::Module(m) => Some(m),
            _ => None,
        }
    }

    /// Get the inner BlockFunc if this is a Func or Method block
    pub fn as_func(&self) -> Option<&'blk BlockFunc<'blk>> {
        match self {
            BasicBlock::Func(f) => Some(f),
            _ => None,
        }
    }

    /// Get the inner BlockClass if this is a Class block
    pub fn as_class(&self) -> Option<&'blk BlockClass<'blk>> {
        match self {
            BasicBlock::Class(c) => Some(c),
            _ => None,
        }
    }

    /// Get the inner BlockTrait if this is a Trait block
    pub fn as_trait(&self) -> Option<&'blk BlockTrait<'blk>> {
        match self {
            BasicBlock::Trait(t) => Some(t),
            _ => None,
        }
    }

    /// Get the inner BlockInterface if this is an Interface block
    pub fn as_interface(&self) -> Option<&'blk BlockInterface<'blk>> {
        match self {
            BasicBlock::Interface(i) => Some(i),
            _ => None,
        }
    }

    /// Get the inner BlockImpl if this is an Impl block
    pub fn as_impl(&self) -> Option<&'blk BlockImpl<'blk>> {
        match self {
            BasicBlock::Impl(i) => Some(i),
            _ => None,
        }
    }

    /// Get the inner BlockEnum if this is an Enum block
    pub fn as_enum(&self) -> Option<&'blk BlockEnum<'blk>> {
        match self {
            BasicBlock::Enum(e) => Some(e),
            _ => None,
        }
    }

    /// Get the inner BlockField if this is a Field block
    pub fn as_field(&self) -> Option<&'blk BlockField<'blk>> {
        match self {
            BasicBlock::Field(f) => Some(f),
            _ => None,
        }
    }

    /// Get the inner BlockCall if this is a Call block
    pub fn as_call(&self) -> Option<&'blk BlockCall<'blk>> {
        match self {
            BasicBlock::Call(c) => Some(c),
            _ => None,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, FromRepr, Display, Default,
)]
#[strum(serialize_all = "snake_case")]
pub enum BlockRelation {
    #[default]
    Unknown,

    /// Parent block contains a child block.
    Contains,
    /// Child block is contained by a parent block.
    ContainedBy,

    /// Callable block owns a parameter block.
    HasParameters,
    /// Callable block owns a return block.
    HasReturn,
    /// Callable block calls another callable block.
    Calls,
    /// Callable block is called by another callable block.
    CalledBy,

    /// Aggregate or nominal type owns a member field block.
    HasField,
    /// Member field block belongs to an aggregate or nominal type.
    FieldOf,
    /// Typed block refers to the block that defines its type.
    TypeOf,
    /// Type-definition block is used as the type for another block.
    TypeFor,
    /// Implementation/extension block targets a type block.
    ImplFor,
    /// Type block has an implementation/extension block.
    HasImpl,
    /// Container block owns a member callable block.
    HasMethod,
    /// Member callable block belongs to a container block.
    MethodOf,
    /// Type or implementation block conforms to a contract block.
    Implements,
    /// Contract block is implemented by a type or implementation block.
    ImplementedBy,

    /// Block uses another type, constant, callable, or contract block.
    Uses,
    /// Block is used by another block.
    UsedBy,

    /// Type or contract block specializes another type or contract block.
    Extends,
    /// Type or contract block is specialized by another block.
    ExtendedBy,
}

impl BlockRelation {
    /// Return the reverse relation, when this relation has one.
    pub fn inverse(self) -> Option<Self> {
        match self {
            BlockRelation::Contains => Some(BlockRelation::ContainedBy),
            BlockRelation::ContainedBy => Some(BlockRelation::Contains),
            BlockRelation::Calls => Some(BlockRelation::CalledBy),
            BlockRelation::CalledBy => Some(BlockRelation::Calls),
            BlockRelation::HasField => Some(BlockRelation::FieldOf),
            BlockRelation::FieldOf => Some(BlockRelation::HasField),
            BlockRelation::TypeOf => Some(BlockRelation::TypeFor),
            BlockRelation::TypeFor => Some(BlockRelation::TypeOf),
            BlockRelation::ImplFor => Some(BlockRelation::HasImpl),
            BlockRelation::HasImpl => Some(BlockRelation::ImplFor),
            BlockRelation::HasMethod => Some(BlockRelation::MethodOf),
            BlockRelation::MethodOf => Some(BlockRelation::HasMethod),
            BlockRelation::Implements => Some(BlockRelation::ImplementedBy),
            BlockRelation::ImplementedBy => Some(BlockRelation::Implements),
            BlockRelation::Uses => Some(BlockRelation::UsedBy),
            BlockRelation::UsedBy => Some(BlockRelation::Uses),
            BlockRelation::Extends => Some(BlockRelation::ExtendedBy),
            BlockRelation::ExtendedBy => Some(BlockRelation::Extends),
            BlockRelation::Unknown | BlockRelation::HasParameters | BlockRelation::HasReturn => {
                None
            }
        }
    }
}

/// Shared metadata and graph relationships for every concrete block.
#[derive(Debug)]
pub struct BlockBase<'blk> {
    pub id: BlockId,
    pub node: HirNode<'blk>,
    pub kind: BlockKind,
    pub parent: RwLock<Option<BlockId>>,
    pub children: RwLock<Vec<BlockId>>,
    /// Direct reference to the symbol that defines this block.
    /// Set during block building. Enables: block.symbol().type_of().block_id()
    pub symbol: Option<&'blk Symbol>,
    /// Type-like blocks this block depends on for architecture graph edges.
    ///
    /// The collection is intentionally broad: languages use it for generic
    /// arguments, contract constraints, decorator/annotation symbols, and other
    /// type-shaped dependencies that should be rendered as graph edges but do
    /// not require their own dedicated relation variant.
    pub type_deps: RwLock<HashSet<BlockId>>,
}

impl<'blk> BlockBase<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        kind: BlockKind,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        Self {
            id,
            node,
            kind,
            parent: RwLock::new(parent),
            children: RwLock::new(children),
            symbol,
            type_deps: RwLock::new(HashSet::new()),
        }
    }

    /// Get the symbol that defines this block (if any)
    pub fn symbol(&self) -> Option<&'blk Symbol> {
        self.symbol
    }

    /// Return nested type ids recorded on this block's symbol.
    pub fn nested_types(&self) -> Option<Vec<SymId>> {
        self.symbol().and_then(Symbol::nested_types)
    }

    /// Return decorator symbol ids recorded on this block's symbol.
    pub fn decorators(&self) -> Option<Vec<SymId>> {
        self.symbol().and_then(Symbol::decorators)
    }

    pub fn try_name(&self) -> Option<&str> {
        self.node
            .as_scope()
            .and_then(|scope| *scope.ident.read())
            .map(|ident| ident.name)
    }

    fn inferred_name(&self) -> String {
        self.try_name().unwrap_or_default().to_string()
    }

    fn name_or_inferred(&self, name: Option<String>) -> String {
        name.filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.inferred_name())
    }

    pub fn add_child(&self, child_id: BlockId) {
        let mut children = self.children.write();
        if !children.contains(&child_id) {
            children.push(child_id);
        }
    }

    pub fn remove_child(&self, child_id: BlockId) {
        self.children.write().retain(|&id| id != child_id);
    }

    pub fn children(&self) -> Vec<BlockId> {
        self.children.read().clone()
    }

    pub fn set_parent(&self, parent_id: BlockId) {
        *self.parent.write() = Some(parent_id);
    }

    pub fn parent(&self) -> Option<BlockId> {
        *self.parent.read()
    }

    /// Add a type dependency to this block
    pub fn add_type_dep(&self, type_id: BlockId) {
        self.type_deps.write().insert(type_id);
    }

    /// Get all type dependencies for this block
    pub fn type_deps(&self) -> HashSet<BlockId> {
        self.type_deps.read().clone()
    }
}

fn fmt_typed_block(
    f: &mut fmt::Formatter<'_>,
    kind: BlockKind,
    id: BlockId,
    name: Option<&str>,
    type_name: &str,
    type_ref: Option<BlockId>,
) -> fmt::Result {
    match name.filter(|name| !name.is_empty()) {
        Some(name) => write!(f, "{kind}:{id} {name}")?,
        None => write!(f, "{kind}:{id}")?,
    }

    if type_name.is_empty() {
        return Ok(());
    }

    match type_ref {
        Some(type_id) => write!(f, " @type:{type_id} {type_name}"),
        None => write!(f, " @type {type_name}"),
    }
}

fn set_type_fields<'blk>(
    unit: CompileUnit<'blk>,
    type_symbol: &'blk Symbol,
    type_name: &mut String,
    type_ref: &RwLock<Option<BlockId>>,
) {
    *type_name = unit
        .resolve_interned_owned(type_symbol.name)
        .unwrap_or_default();
    *type_ref.write() = type_symbol.block_id();
}

#[derive(Debug)]
pub struct BlockRoot<'blk> {
    pub base: BlockBase<'blk>,
    pub file_name: Option<String>,
    /// Crate name from Cargo.toml [package] name
    pub crate_name: RwLock<Option<String>>,
    /// Crate/package root directory path
    pub crate_root: RwLock<Option<String>>,
    /// Module path relative to crate root (e.g., "utils::helpers")
    pub module_path: RwLock<Option<String>>,
    /// Module root directory path
    pub module_root: RwLock<Option<String>>,
}

impl<'blk> BlockRoot<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        file_name: Option<String>,
    ) -> Self {
        Self::new_with(id, node, parent, children, file_name, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        file_name: Option<String>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Root, parent, children, symbol);
        Self {
            base,
            file_name,
            crate_name: RwLock::new(None),
            crate_root: RwLock::new(None),
            module_path: RwLock::new(None),
            module_root: RwLock::new(None),
        }
    }

    pub fn set_crate_name(&self, name: String) {
        *self.crate_name.write() = Some(name);
    }

    pub fn set_meta(&self, unit: CompileUnit<'blk>, scope: Option<&Scope<'blk>>) {
        let meta = unit.unit_meta();
        let interner = unit.interner();

        if let Some(ref package_name) = meta.package_name {
            self.set_crate_name(package_name.clone());
        }
        if let Some(ref package_root) = meta.package_root {
            self.set_crate_root(package_root.display().to_string());
        }
        if let Some(ref module_name) = meta.module_name {
            self.set_module_path(module_name.clone());
        }
        if let Some(ref module_root) = meta.module_root {
            self.set_module_root(module_root.display().to_string());
        }

        if let Some(scope) = scope {
            if meta.package_name.is_none()
                && let Some(crate_sym) = scope.try_parent_symbol(SymKind::Crate)
                && let Some(name) = interner.try_resolve(crate_sym.name)
            {
                self.set_crate_name(name);
            }
            if meta.module_name.is_none()
                && let Some(module_sym) = scope.try_parent_symbol(SymKind::Module)
                && let Some(name) = interner.try_resolve(module_sym.name)
            {
                self.set_module_path(name);
            }
        }
    }

    pub fn crate_name(&self) -> Option<String> {
        self.crate_name.read().clone()
    }

    pub fn set_crate_root(&self, root: String) {
        *self.crate_root.write() = Some(root);
    }

    pub fn crate_root(&self) -> Option<String> {
        self.crate_root.read().clone()
    }

    pub fn set_module_path(&self, path: String) {
        *self.module_path.write() = Some(path);
    }

    pub fn module_path(&self) -> Option<String> {
        self.module_path.read().clone()
    }

    pub fn set_module_root(&self, root: String) {
        *self.module_root.write() = Some(root);
    }

    pub fn module_root(&self) -> Option<String> {
        self.module_root.read().clone()
    }
}

impl<'blk> fmt::Display for BlockRoot<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.base.try_name().unwrap_or("");
        if let Some(file_name) = &self.file_name {
            write!(
                f,
                "{}:{} {} ({})",
                self.base.kind, self.base.id, name, file_name
            )
        } else {
            write!(f, "{}:{} {}", self.base.kind, self.base.id, name)
        }
    }
}

/// Block representing a module declaration (`mod foo` or `mod foo { ... }`)
#[derive(Debug)]
pub struct BlockModule<'blk> {
    pub base: BlockBase<'blk>,
    /// Module name (e.g., "utils" for `mod utils;`)
    name: String,
    /// Whether this is an inline module (`mod foo { ... }`) vs file module (`mod foo;`)
    pub is_inline: bool,
}

impl<'blk> BlockModule<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        name: String,
        is_inline: bool,
    ) -> Self {
        Self::new_with(id, node, parent, children, name, is_inline, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        name: String,
        is_inline: bool,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Module, parent, children, symbol);
        Self {
            base,
            name,
            is_inline,
        }
    }

    pub fn name(&self) -> Option<&str> {
        (!self.name.is_empty()).then_some(&self.name)
    }
}

impl<'blk> fmt::Display for BlockModule<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inline_marker = if self.is_inline { " (inline)" } else { "" };
        write!(
            f,
            "{}:{} {}{}",
            self.base.kind, self.base.id, self.name, inline_marker
        )
    }
}

#[derive(Debug)]
pub struct BlockFunc<'blk> {
    pub base: BlockBase<'blk>,
    name: String,
    pub parameters: RwLock<Vec<BlockId>>,
    pub returns: RwLock<Option<BlockId>>,
    /// Functions/methods called by this function
    pub func_deps: RwLock<HashSet<BlockId>>,
}

impl<'blk> BlockFunc<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        kind: BlockKind,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, kind, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        kind: BlockKind,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, kind, parent, children, symbol);
        let name = base.inferred_name();
        Self {
            base,
            name,
            parameters: RwLock::new(Vec::new()),
            returns: RwLock::new(None),
            func_deps: RwLock::new(HashSet::new()),
        }
    }

    pub fn name(&self) -> Option<&str> {
        (!self.name.is_empty()).then_some(&self.name)
    }

    pub fn is_method(&self) -> bool {
        self.base.kind == BlockKind::Method
    }

    pub fn base(&self) -> &BlockBase<'blk> {
        &self.base
    }

    pub fn symbol(&self) -> Option<&'blk Symbol> {
        self.base.symbol()
    }

    pub fn nested_types(&self) -> Option<Vec<SymId>> {
        self.base.nested_types()
    }

    pub fn decorators(&self) -> Option<Vec<SymId>> {
        self.base.decorators()
    }

    pub fn add_parameter(&self, param: BlockId) {
        self.parameters.write().push(param);
    }

    pub fn parameters(&self) -> Vec<BlockId> {
        self.parameters.read().clone()
    }

    pub fn set_return(&self, ret: BlockId) {
        *self.returns.write() = Some(ret);
    }

    pub fn return_block(&self) -> Option<BlockId> {
        *self.returns.read()
    }

    pub fn children(&self) -> Vec<BlockId> {
        self.base.children()
    }

    pub fn add_type_dep(&self, type_id: BlockId) {
        self.base.add_type_dep(type_id);
    }

    pub fn type_deps(&self) -> HashSet<BlockId> {
        self.base.type_deps()
    }

    pub fn add_func_dep(&self, func_id: BlockId) {
        self.func_deps.write().insert(func_id);
    }

    pub fn func_deps(&self) -> HashSet<BlockId> {
        self.func_deps.read().clone()
    }

    /// Format dependency entries as pseudo-children (to be rendered after real children)
    /// Returns lines like "@tdep:3 Bar" and "@fdep:5 process"
    pub fn dependency_labels(&self, unit: CompileUnit<'blk>) -> Vec<String> {
        let mut deps = Vec::new();

        // Add type_deps
        let type_deps = self.type_deps();
        if !type_deps.is_empty() {
            let mut sorted: Vec<_> = type_deps.iter().collect();
            sorted.sort();
            for dep_id in sorted {
                let dep_block = unit.block(*dep_id);
                let dep_name = dep_block.try_name().unwrap_or("");
                deps.push(format!("@tdep:{dep_id} {dep_name}"));
            }
        }

        // Add func_deps
        let func_deps = self.func_deps();
        if !func_deps.is_empty() {
            let mut sorted: Vec<_> = func_deps.iter().collect();
            sorted.sort();
            for dep_id in sorted {
                let dep_block = unit.block(*dep_id);
                let dep_name = dep_block.try_name().unwrap_or("");
                deps.push(format!("@fdep:{dep_id} {dep_name}"));
            }
        }

        deps
    }
}

impl<'blk> fmt::Display for BlockFunc<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} {}", self.base.kind, self.base.id, self.name)
    }
}

#[derive(Debug)]
pub struct BlockCall<'blk> {
    pub base: BlockBase<'blk>,
    pub callee: RwLock<Option<BlockId>>,
    pub args: RwLock<Vec<BlockId>>,
}

impl<'blk> BlockCall<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Call, parent, children, symbol);
        Self {
            base,
            callee: RwLock::new(None),
            args: RwLock::new(Vec::new()),
        }
    }

    pub fn set_callee(&self, callee_id: BlockId) {
        *self.callee.write() = Some(callee_id);
    }

    pub fn add_arg(&self, arg_id: BlockId) {
        self.args.write().push(arg_id);
    }

    pub fn callee(&self) -> Option<BlockId> {
        *self.callee.read()
    }

    pub fn args(&self) -> Vec<BlockId> {
        self.args.read().clone()
    }
}

impl<'blk> fmt::Display for BlockCall<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.base.kind, self.base.id)
    }
}

#[derive(Debug)]
pub struct BlockClass<'blk> {
    pub base: BlockBase<'blk>,
    name: String,
    pub fields: RwLock<Vec<BlockId>>,
    pub methods: RwLock<Vec<BlockId>>,
    /// Optional base type for languages with single primary inheritance.
    ///
    /// Languages without this concept leave it empty. Languages with multiple
    /// base types should store additional generalization edges in graph
    /// relations rather than overloading this display field.
    pub extends: RwLock<Option<(String, Option<BlockId>)>>,
}

impl<'blk> BlockClass<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Class, parent, children, symbol);
        let name = base.inferred_name();
        Self {
            base,
            name,
            fields: RwLock::new(Vec::new()),
            methods: RwLock::new(Vec::new()),
            extends: RwLock::new(None),
        }
    }

    pub fn name(&self) -> Option<&str> {
        (!self.name.is_empty()).then_some(&self.name)
    }

    pub fn symbol(&self) -> Option<&'blk Symbol> {
        self.base.symbol()
    }

    pub fn nested_types(&self) -> Option<Vec<SymId>> {
        self.base.nested_types()
    }

    pub fn decorators(&self) -> Option<Vec<SymId>> {
        self.base.decorators()
    }

    pub fn add_type_dep(&self, type_id: BlockId) {
        self.base.add_type_dep(type_id);
    }

    pub fn type_deps(&self) -> HashSet<BlockId> {
        self.base.type_deps()
    }

    pub fn add_field(&self, field_id: BlockId) {
        self.fields.write().push(field_id);
    }

    pub fn add_method(&self, method_id: BlockId) {
        self.methods.write().push(method_id);
    }

    pub fn fields(&self) -> Vec<BlockId> {
        self.fields.read().clone()
    }

    pub fn methods(&self) -> Vec<BlockId> {
        self.methods.read().clone()
    }

    /// Set the displayed base type for this nominal type block.
    pub fn set_extends(&self, name: String, block_id: Option<BlockId>) {
        *self.extends.write() = Some((name, block_id));
    }

    /// Return the displayed base type for this nominal type block.
    pub fn extends(&self) -> Option<(String, Option<BlockId>)> {
        self.extends.read().clone()
    }
}

impl<'blk> fmt::Display for BlockClass<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let extends = self.extends.read();
        if let Some((name, id)) = extends.as_ref() {
            if let Some(block_id) = id {
                write!(
                    f,
                    "{}:{} {} @extends:{} {}",
                    self.base.kind, self.base.id, self.name, block_id, name
                )
            } else {
                write!(
                    f,
                    "{}:{} {} @extends {}",
                    self.base.kind, self.base.id, self.name, name
                )
            }
        } else {
            write!(f, "{}:{} {}", self.base.kind, self.base.id, self.name)
        }
    }
}

impl<'blk> BlockClass<'blk> {
    /// Format dependency entries as pseudo-children after real children.
    pub fn dependency_labels(&self, unit: CompileUnit<'blk>) -> Vec<String> {
        let mut deps = Vec::new();

        // Includes generic arguments, constraints, decorators, and contracts.
        let type_deps = self.type_deps();
        if !type_deps.is_empty() {
            let mut sorted: Vec<_> = type_deps.iter().collect();
            sorted.sort();
            for dep_id in sorted {
                let dep_block = unit.block(*dep_id);
                let dep_name = dep_block.try_name().unwrap_or("");
                deps.push(format!("@tdep:{dep_id} {dep_name}"));
            }
        }

        deps
    }
}

#[derive(Debug)]
pub struct BlockTrait<'blk> {
    pub base: BlockBase<'blk>,
    name: String,
    pub methods: RwLock<Vec<BlockId>>,
}

impl<'blk> BlockTrait<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Trait, parent, children, symbol);
        let name = base.inferred_name();
        Self {
            base,
            name,
            methods: RwLock::new(Vec::new()),
        }
    }

    pub fn name(&self) -> Option<&str> {
        (!self.name.is_empty()).then_some(&self.name)
    }

    pub fn symbol(&self) -> Option<&'blk Symbol> {
        self.base.symbol()
    }

    pub fn add_method(&self, method_id: BlockId) {
        self.methods.write().push(method_id);
    }

    pub fn methods(&self) -> Vec<BlockId> {
        self.methods.read().clone()
    }
}

impl<'blk> fmt::Display for BlockTrait<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} {}", self.base.kind, self.base.id, self.name)
    }
}

/// Block representing a structural or declared contract body.
#[derive(Debug)]
pub struct BlockInterface<'blk> {
    pub base: BlockBase<'blk>,
    name: String,
    pub methods: RwLock<Vec<BlockId>>,
    pub fields: RwLock<Vec<BlockId>>,
    /// Base contracts for languages with contract inheritance/refinement.
    pub extends: RwLock<Vec<(String, Option<BlockId>)>>,
}

impl<'blk> BlockInterface<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Interface, parent, children, symbol);
        let name = base.inferred_name();
        Self {
            base,
            name,
            methods: RwLock::new(Vec::new()),
            fields: RwLock::new(Vec::new()),
            extends: RwLock::new(Vec::new()),
        }
    }

    pub fn name(&self) -> Option<&str> {
        (!self.name.is_empty()).then_some(&self.name)
    }

    pub fn add_method(&self, method_id: BlockId) {
        self.methods.write().push(method_id);
    }

    pub fn methods(&self) -> Vec<BlockId> {
        self.methods.read().clone()
    }

    pub fn add_field(&self, field_id: BlockId) {
        self.fields.write().push(field_id);
    }

    pub fn fields(&self) -> Vec<BlockId> {
        self.fields.read().clone()
    }

    pub fn symbol(&self) -> Option<&'blk Symbol> {
        self.base.symbol()
    }

    pub fn nested_types(&self) -> Option<Vec<SymId>> {
        self.base.nested_types()
    }

    /// Add a displayed base contract.
    pub fn add_extends(&self, name: String, block_id: Option<BlockId>) {
        let mut extends = self.extends.write();
        if !extends
            .iter()
            .any(|(existing_name, existing_id)| existing_name == &name && *existing_id == block_id)
        {
            extends.push((name, block_id));
        }
    }

    /// Return displayed base contracts.
    pub fn extends(&self) -> Vec<(String, Option<BlockId>)> {
        self.extends.read().clone()
    }
}

impl<'blk> fmt::Display for BlockInterface<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let extends = self.extends.read();
        if extends.is_empty() {
            write!(f, "{}:{} {}", self.base.kind, self.base.id, self.name)
        } else {
            let extends_list: Vec<_> = extends
                .iter()
                .map(|(name, id)| {
                    if let Some(block_id) = id {
                        format!("@extends:{block_id} {name}")
                    } else {
                        format!("@extends {name}")
                    }
                })
                .collect();
            write!(
                f,
                "{}:{} {} {}",
                self.base.kind,
                self.base.id,
                self.name,
                extends_list.join(" ")
            )
        }
    }
}

#[derive(Debug)]
pub struct BlockImpl<'blk> {
    pub base: BlockBase<'blk>,
    name: String,
    /// Target type block ID (resolved during link_blocks if needed)
    target: RwLock<Option<BlockId>>,
    /// Target type symbol (for deferred block_id resolution)
    target_sym: Option<&'blk Symbol>,
    /// Contract block ID (resolved during link_blocks if needed).
    trait_ref: RwLock<Option<BlockId>>,
    /// Contract symbol (for deferred block_id resolution).
    trait_sym: Option<&'blk Symbol>,
    methods: RwLock<Vec<BlockId>>,
}

impl<'blk> BlockImpl<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Impl, parent, children, symbol);
        let name = base.inferred_name();
        Self {
            base,
            name,
            target: RwLock::new(None),
            target_sym: None,
            trait_ref: RwLock::new(None),
            trait_sym: None,
            methods: RwLock::new(Vec::new()),
        }
    }

    pub fn name(&self) -> Option<&str> {
        (!self.name.is_empty()).then_some(&self.name)
    }

    /// Set the impl target type from its bound symbol.
    pub fn set_target(&mut self, unit: CompileUnit<'blk>, symbol: &'blk Symbol) {
        let resolved = unit.try_type(symbol).unwrap_or(symbol);
        *self.target.write() = resolved.block_id();
        self.target_sym = Some(symbol);
    }

    /// Set the implemented contract from its bound symbol.
    pub fn set_trait(&mut self, unit: CompileUnit<'blk>, symbol: &'blk Symbol) {
        let resolved = unit.try_type(symbol).unwrap_or(symbol);
        *self.trait_ref.write() = resolved.block_id();
        self.trait_sym = Some(resolved);
    }

    pub fn set_target_ref(&self, target_id: BlockId) {
        *self.target.write() = Some(target_id);
    }

    pub fn set_trait_ref(&self, trait_id: BlockId) {
        *self.trait_ref.write() = Some(trait_id);
    }

    pub fn target_symbol(&self) -> Option<&'blk Symbol> {
        self.target_sym
    }

    pub fn trait_symbol(&self) -> Option<&'blk Symbol> {
        self.trait_sym
    }

    pub fn resolved_target(&self) -> Option<BlockId> {
        self.target()
            .or_else(|| self.target_symbol().and_then(Symbol::block_id))
    }

    pub fn resolved_trait(&self) -> Option<BlockId> {
        self.trait_ref()
            .or_else(|| self.trait_symbol().and_then(Symbol::block_id))
    }

    pub fn target_nested_types(&self) -> Option<Vec<SymId>> {
        self.target_symbol().and_then(Symbol::nested_types)
    }

    pub fn add_method(&self, method_id: BlockId) {
        self.methods.write().push(method_id);
    }

    pub fn target(&self) -> Option<BlockId> {
        *self.target.read()
    }

    pub fn trait_ref(&self) -> Option<BlockId> {
        *self.trait_ref.read()
    }

    pub fn methods(&self) -> Vec<BlockId> {
        self.methods.read().clone()
    }
}

impl<'blk> fmt::Display for BlockImpl<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = vec![format!("{}:{} {}", self.base.kind, self.base.id, self.name)];
        if let Some(target_id) = self.target() {
            parts.push(format!("@type:{target_id}"));
        }
        if let Some(trait_id) = self.trait_ref() {
            parts.push(format!("@trait:{trait_id}"));
        }
        write!(f, "{}", parts.join(" "))
    }
}

#[derive(Debug)]
pub struct BlockEnum<'blk> {
    pub base: BlockBase<'blk>,
    name: String,
    pub variants: RwLock<Vec<BlockId>>,
}

impl<'blk> BlockEnum<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Enum, parent, children, symbol);
        let name = base.inferred_name();
        Self {
            base,
            name,
            variants: RwLock::new(Vec::new()),
        }
    }

    pub fn name(&self) -> Option<&str> {
        (!self.name.is_empty()).then_some(&self.name)
    }

    pub fn add_variant(&self, variant_id: BlockId) {
        self.variants.write().push(variant_id);
    }

    pub fn variants(&self) -> Vec<BlockId> {
        self.variants.read().clone()
    }
}

impl<'blk> fmt::Display for BlockEnum<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} {}", self.base.kind, self.base.id, self.name)
    }
}

#[derive(Debug)]
pub struct BlockConst<'blk> {
    pub base: BlockBase<'blk>,
    name: String,
    /// Type name for display (e.g., "i32", "String")
    pub type_name: String,
    /// Block ID of the type definition (for user-defined types)
    pub type_ref: RwLock<Option<BlockId>>,
}

impl<'blk> BlockConst<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        Self::new_with_name(id, node, parent, children, None, symbol)
    }

    pub(crate) fn new_with_name(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        name: Option<String>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Const, parent, children, symbol);
        let name = base.name_or_inferred(name);
        Self {
            base,
            name,
            type_name: String::new(),
            type_ref: RwLock::new(None),
        }
    }

    pub fn name(&self) -> Option<&str> {
        (!self.name.is_empty()).then_some(&self.name)
    }

    pub fn base(&self) -> &BlockBase<'blk> {
        &self.base
    }

    /// Set the displayed type for this const block.
    pub fn set_type(&mut self, unit: CompileUnit<'blk>, type_symbol: &'blk Symbol) {
        set_type_fields(unit, type_symbol, &mut self.type_name, &self.type_ref);
    }

    /// Set type reference (used during link_blocks for cross-file resolution)
    pub fn set_type_ref(&self, type_ref: BlockId) {
        *self.type_ref.write() = Some(type_ref);
    }

    /// Get the type reference
    pub fn type_ref(&self) -> Option<BlockId> {
        *self.type_ref.read()
    }
}

impl<'blk> fmt::Display for BlockConst<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_typed_block(
            f,
            self.base.kind,
            self.base.id,
            Some(&self.name),
            &self.type_name,
            *self.type_ref.read(),
        )
    }
}

#[derive(Debug)]
pub struct BlockField<'blk> {
    pub base: BlockBase<'blk>,
    name: String,
    /// Type name for display (e.g., "i32", "String")
    pub type_name: String,
    /// Block ID of the type definition (for user-defined types)
    pub type_ref: RwLock<Option<BlockId>>,
}

impl<'blk> BlockField<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        Self::new_with_name(id, node, parent, children, None, symbol)
    }

    pub(crate) fn new_with_name(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        name: Option<String>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Field, parent, children, symbol);
        let name = base.name_or_inferred(name);
        Self {
            base,
            name,
            type_name: String::new(),
            type_ref: RwLock::new(None),
        }
    }

    pub fn name(&self) -> Option<&str> {
        (!self.name.is_empty()).then_some(&self.name)
    }

    pub fn base(&self) -> &BlockBase<'blk> {
        &self.base
    }

    pub fn children(&self) -> Vec<BlockId> {
        self.base.children()
    }

    /// Set the displayed type for this field block.
    pub fn set_type(&mut self, unit: CompileUnit<'blk>, type_symbol: &'blk Symbol) {
        set_type_fields(unit, type_symbol, &mut self.type_name, &self.type_ref);
    }

    /// Set type reference (used during link_blocks for cross-file resolution)
    pub fn set_type_ref(&self, type_ref: BlockId) {
        *self.type_ref.write() = Some(type_ref);
    }

    /// Get the type reference
    pub fn type_ref(&self) -> Option<BlockId> {
        *self.type_ref.read()
    }
}

impl<'blk> fmt::Display for BlockField<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_typed_block(
            f,
            self.base.kind,
            self.base.id,
            Some(&self.name),
            &self.type_name,
            *self.type_ref.read(),
        )
    }
}

/// Represents a single function/method parameter as its own block
#[derive(Debug)]
pub struct BlockParameter<'blk> {
    pub base: BlockBase<'blk>,
    /// Parameter name (e.g., "x", "self")
    name: String,
    /// Type name for display (e.g., "i32", "String")
    pub type_name: String,
    /// Block ID of the type definition (for user-defined types)
    pub type_ref: RwLock<Option<BlockId>>,
}

impl<'blk> BlockParameter<'blk> {
    /// Create a new BlockParameter for function/method parameters
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        Self::new_with_name(id, node, parent, children, None, symbol)
    }

    pub(crate) fn new_with_name(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        name: Option<String>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Parameter, parent, children, symbol);
        let name = base.name_or_inferred(name);
        Self {
            base,
            name,
            type_name: String::new(),
            type_ref: RwLock::new(None),
        }
    }

    pub fn name(&self) -> Option<&str> {
        (!self.name.is_empty()).then_some(&self.name)
    }

    pub fn base(&self) -> &BlockBase<'blk> {
        &self.base
    }

    /// Set the displayed type for this parameter block.
    pub fn set_type(&mut self, unit: CompileUnit<'blk>, type_symbol: &'blk Symbol) {
        set_type_fields(unit, type_symbol, &mut self.type_name, &self.type_ref);
    }

    /// Set type reference (used during link_blocks for cross-file resolution)
    pub fn set_type_ref(&self, type_ref: BlockId) {
        *self.type_ref.write() = Some(type_ref);
    }

    /// Get the type reference
    pub fn type_ref(&self) -> Option<BlockId> {
        *self.type_ref.read()
    }
}

impl<'blk> fmt::Display for BlockParameter<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_typed_block(
            f,
            self.base.kind,
            self.base.id,
            Some(&self.name),
            &self.type_name,
            *self.type_ref.read(),
        )
    }
}

/// Represents a function/method return type as its own block
#[derive(Debug)]
pub struct BlockReturn<'blk> {
    pub base: BlockBase<'blk>,
    /// Type name for display (e.g., "i32", "String")
    pub type_name: String,
    /// Block ID of the type definition (for user-defined types)
    pub type_ref: RwLock<Option<BlockId>>,
}

impl<'blk> BlockReturn<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Return, parent, children, symbol);
        Self {
            base,
            type_name: String::new(),
            type_ref: RwLock::new(None),
        }
    }

    /// Set the displayed type for this return block.
    pub fn set_type(&mut self, unit: CompileUnit<'blk>, type_symbol: &'blk Symbol) {
        set_type_fields(unit, type_symbol, &mut self.type_name, &self.type_ref);
    }

    pub fn base(&self) -> &BlockBase<'blk> {
        &self.base
    }

    /// Set type reference (used during link_blocks for cross-file resolution)
    pub fn set_type_ref(&self, type_ref: BlockId) {
        *self.type_ref.write() = Some(type_ref);
    }

    /// Get the type reference
    pub fn type_ref(&self) -> Option<BlockId> {
        *self.type_ref.read()
    }
}

impl<'blk> fmt::Display for BlockReturn<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_typed_block(
            f,
            self.base.kind,
            self.base.id,
            None,
            &self.type_name,
            *self.type_ref.read(),
        )
    }
}

#[derive(Debug)]
pub struct BlockAlias<'blk> {
    pub base: BlockBase<'blk>,
    name: String,
}

impl<'blk> BlockAlias<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with(id, node, parent, children, None)
    }

    pub fn new_with(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        Self::new_with_name(id, node, parent, children, None, symbol)
    }

    pub(crate) fn new_with_name(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        name: Option<String>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Alias, parent, children, symbol);
        let name = base.name_or_inferred(name);
        Self { base, name }
    }

    pub fn name(&self) -> Option<&str> {
        (!self.name.is_empty()).then_some(&self.name)
    }

    pub fn base(&self) -> &BlockBase<'blk> {
        &self.base
    }
}

impl<'blk> fmt::Display for BlockAlias<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} {}", self.base.kind, self.base.id, self.name)
    }
}
