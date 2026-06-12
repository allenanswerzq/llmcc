//! Basic block representation for code graph.

use parking_lot::RwLock;
use std::collections::HashSet;
use std::fmt;
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::CompileUnit;
use crate::declare_arena;
pub use crate::id::{BlockId, reset_block_id_counter};
use crate::ir::HirNode;
use crate::symbol::Symbol;

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
    /// Return true when this kind has a concrete [`BasicBlock`] representation.
    pub fn is_graph_block(self) -> bool {
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

    /// Parent contains child (Root→Func, Class→Method, etc.)
    Contains,
    /// Child is contained by parent
    ContainedBy,

    /// Func/Method → Parameters block
    HasParameters,
    /// Func/Method → Return block
    HasReturn,
    /// Func/Method → Func/Method it calls
    Calls,
    /// Func/Method is called by another Func/Method
    CalledBy,

    /// Class/Enum → Field blocks
    HasField,
    /// Field → Class/Enum that owns it
    FieldOf,
    /// Field/Parameter/Return → Type definition (the type of this element)
    TypeOf,
    /// Type definition → Field/Parameter/Return that uses this type
    TypeFor,
    /// Impl → Type it implements for
    ImplFor,
    /// Type → Impl blocks for this type
    HasImpl,
    /// Impl/Trait → Method blocks
    HasMethod,
    /// Method → Impl/Trait/Class that owns it
    MethodOf,
    /// Type → Trait it implements
    Implements,
    /// Trait → Types that implement it
    ImplementedBy,

    /// Uses a type/const/function
    Uses,
    /// Is used by
    UsedBy,

    /// Trait/Interface extends another (TypeScript extends, Rust supertraits)
    Extends,
    /// Trait/Interface is extended by another
    ExtendedBy,
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
    /// Types this block depends on (used for arch graph edges)
    /// For impl blocks: type arguments from trait reference (e.g., User in `impl Repository<User>`)
    /// For structs/enums: could include generic bounds or trait objects
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
    /// Extended class (for TypeScript/JavaScript class inheritance)
    /// A class can only extend one other class (single inheritance)
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

    /// Set the extended class
    pub fn set_extends(&self, name: String, block_id: Option<BlockId>) {
        *self.extends.write() = Some((name, block_id));
    }

    /// Get the extended class
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
    /// Format dependency entries as pseudo-children (to be rendered after real children)
    /// Returns lines like "@tdep:3 Bar" for implemented interfaces
    pub fn dependency_labels(&self, unit: CompileUnit<'blk>) -> Vec<String> {
        let mut deps = Vec::new();

        // Add type_deps (includes implemented interfaces)
        let type_deps = self.base.type_deps();
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

/// Block representing a TypeScript interface declaration
#[derive(Debug)]
pub struct BlockInterface<'blk> {
    pub base: BlockBase<'blk>,
    name: String,
    pub methods: RwLock<Vec<BlockId>>,
    pub fields: RwLock<Vec<BlockId>>,
    /// Extended interfaces (for TypeScript extends)
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

    /// Add an extended interface
    pub fn add_extends(&self, name: String, block_id: Option<BlockId>) {
        self.extends.write().push((name, block_id));
    }

    /// Get extended interfaces
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
    /// Target type block ID (resolved during connect_blocks if needed)
    pub target: RwLock<Option<BlockId>>,
    /// Target type symbol (for deferred block_id resolution)
    pub target_sym: Option<&'blk Symbol>,
    /// Trait block ID (resolved during connect_blocks if needed)
    pub trait_ref: RwLock<Option<BlockId>>,
    /// Trait symbol (for deferred block_id resolution)
    pub trait_sym: Option<&'blk Symbol>,
    pub methods: RwLock<Vec<BlockId>>,
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

    /// Set target with both block_id (if available) and symbol (for deferred resolution)
    pub fn set_target_info(&mut self, block_id: Option<BlockId>, sym: Option<&'blk Symbol>) {
        *self.target.write() = block_id;
        self.target_sym = sym;
    }

    /// Set trait with both block_id (if available) and symbol (for deferred resolution)
    pub fn set_trait_info(&mut self, block_id: Option<BlockId>, sym: Option<&'blk Symbol>) {
        *self.trait_ref.write() = block_id;
        self.trait_sym = sym;
    }

    pub fn set_target(&self, target_id: BlockId) {
        *self.target.write() = Some(target_id);
    }

    pub fn set_trait_ref(&self, trait_id: BlockId) {
        *self.trait_ref.write() = Some(trait_id);
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

    /// Set type info for this const block (used during block building)
    pub fn set_type_info(&mut self, type_name: String, type_ref: Option<BlockId>) {
        self.type_name = type_name;
        *self.type_ref.write() = type_ref;
    }

    /// Set type reference (used during connect_blocks for cross-file resolution)
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

    /// Set type info for this field block (used during block building)
    pub fn set_type_info(&mut self, type_name: String, type_ref: Option<BlockId>) {
        self.type_name = type_name;
        *self.type_ref.write() = type_ref;
    }

    /// Set type reference (used during connect_blocks for cross-file resolution)
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

    /// Set type info for this parameter block (used during block building)
    pub fn set_type_info(&mut self, type_name: String, type_ref: Option<BlockId>) {
        self.type_name = type_name;
        *self.type_ref.write() = type_ref;
    }

    /// Set type reference (used during connect_blocks for cross-file resolution)
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

    /// Set type info for this return block (used during block building)
    pub fn set_type_info(&mut self, type_name: String, type_ref: Option<BlockId>) {
        self.type_name = type_name;
        *self.type_ref.write() = type_ref;
    }

    /// Set type reference (used during connect_blocks for cross-file resolution)
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
}

impl<'blk> fmt::Display for BlockAlias<'blk> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} {}", self.base.kind, self.base.id, self.name)
    }
}
