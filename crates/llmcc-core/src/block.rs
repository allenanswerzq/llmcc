use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::CompileUnit;
use crate::declare_arena;
use crate::ir::HirNode;
use crate::symbol::Symbol;

declare_arena!(BlockArena {
    bb: BasicBlock<'a>,
    blk_root: BlockRoot<'a>,
    blk_module: BlockModule<'a>,
    blk_func: BlockFunc<'a>,
    blk_class: BlockClass<'a>,
    blk_trait: BlockTrait<'a>,
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
    Enum,
    Const,
    Impl,
    Field,
    Scope,
    Parameter,
    Return,
    Alias,
}

#[derive(Debug, Clone)]
pub enum BasicBlock<'blk> {
    Undefined,
    Root(&'blk BlockRoot<'blk>),
    Module(&'blk BlockModule<'blk>),
    Func(&'blk BlockFunc<'blk>),
    Call(&'blk BlockCall<'blk>),
    Enum(&'blk BlockEnum<'blk>),
    Class(&'blk BlockClass<'blk>),
    Trait(&'blk BlockTrait<'blk>),
    Impl(&'blk BlockImpl<'blk>),
    Const(&'blk BlockConst<'blk>),
    Field(&'blk BlockField<'blk>),
    Parameter(&'blk BlockParameter<'blk>),
    Return(&'blk BlockReturn<'blk>),
    Alias(&'blk BlockAlias<'blk>),
    Block,
}

impl<'blk> BasicBlock<'blk> {
    /// Format the block label (content inside the parentheses)
    /// Delegates to each block type's format method
    pub fn format_block(&self, unit: CompileUnit<'blk>) -> String {
        match self {
            BasicBlock::Root(root) => root.format(),
            BasicBlock::Module(module) => module.format(),
            BasicBlock::Func(func) => func.format(unit),
            BasicBlock::Class(class) => class.format(),
            BasicBlock::Trait(trait_blk) => trait_blk.format(),
            BasicBlock::Impl(impl_blk) => impl_blk.format(),
            BasicBlock::Call(call) => call.format(),
            BasicBlock::Enum(enum_blk) => enum_blk.format(),
            BasicBlock::Const(const_blk) => const_blk.format(),
            BasicBlock::Field(field) => field.format(),
            BasicBlock::Parameter(param) => param.format(),
            BasicBlock::Return(ret) => ret.format(),
            BasicBlock::Alias(alias) => alias.format(),
            BasicBlock::Undefined | BasicBlock::Block => "undefined".to_string(),
        }
    }

    /// Format extra entries for dependencies (rendered as pseudo-children)
    /// Only applicable to Func blocks, returns empty for others
    pub fn format_deps(&self, unit: CompileUnit<'blk>) -> Vec<String> {
        match self {
            BasicBlock::Func(func) => func.format_deps(unit),
            _ => Vec::new(),
        }
    }

    /// Format the suffix to appear after the closing parenthesis
    /// (Currently unused - type info is now inline in format_block)
    pub fn format_suffix(&self) -> Option<String> {
        None
    }

    pub fn id(&self) -> BlockId {
        self.block_id()
    }

    /// Get the base block information regardless of variant
    pub fn base(&self) -> Option<&BlockBase<'blk>> {
        match self {
            BasicBlock::Undefined | BasicBlock::Block => None,
            BasicBlock::Root(block) => Some(&block.base),
            BasicBlock::Module(block) => Some(&block.base),
            BasicBlock::Func(block) => Some(&block.base),
            BasicBlock::Class(block) => Some(&block.base),
            BasicBlock::Trait(block) => Some(&block.base),
            BasicBlock::Impl(block) => Some(&block.base),
            BasicBlock::Call(block) => Some(&block.base),
            BasicBlock::Enum(block) => Some(&block.base),
            BasicBlock::Const(block) => Some(&block.base),
            BasicBlock::Field(block) => Some(&block.base),
            BasicBlock::Parameter(block) => Some(&block.base),
            BasicBlock::Return(block) => Some(&block.base),
            BasicBlock::Alias(block) => Some(&block.base),
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

    /// Get the symbol that defines this block (if any)
    pub fn symbol(&self) -> Option<&'blk Symbol> {
        self.base().and_then(|base| base.symbol())
    }

    /// Get the children block IDs
    pub fn children(&self) -> Vec<BlockId> {
        self.base()
            .map(|base| base.get_children())
            .unwrap_or_default()
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

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, Default, PartialOrd, Ord)]
pub struct BlockId(pub u32);

/// Global counter for allocating unique Block IDs
static BLOCK_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

/// Reset global BlockId counter (primarily for deterministic tests)
pub fn reset_block_id_counter() {
    BLOCK_ID_COUNTER.store(1, Ordering::SeqCst);
}

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl BlockId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Allocate a new unique BlockId
    pub fn allocate() -> Self {
        let id = BLOCK_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        BlockId(id)
    }

    /// Get the next BlockId that will be allocated (useful for diagnostics)
    pub fn next() -> Self {
        BlockId(BLOCK_ID_COUNTER.load(Ordering::Relaxed))
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

    // ========== Structural Relations ==========
    /// Parent contains child (Root→Func, Class→Method, etc.)
    Contains,
    /// Child is contained by parent
    ContainedBy,

    // ========== Function/Method Relations ==========
    /// Func/Method → Parameters block
    HasParameters,
    /// Func/Method → Return block
    HasReturn,
    /// Func/Method → Func/Method it calls
    Calls,
    /// Func/Method is called by another Func/Method
    CalledBy,

    // ========== Type Relations ==========
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

    // ========== Generic Reference ==========
    /// Uses a type/const/function
    Uses,
    /// Is used by
    UsedBy,
}

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
    ) -> Self {
        Self {
            id,
            node,
            kind,
            parent: RwLock::new(parent),
            children: RwLock::new(children),
            symbol: None,
            type_deps: RwLock::new(HashSet::new()),
        }
    }

    /// Create a new BlockBase with symbol reference
    pub fn with_symbol(
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

    pub fn opt_get_name(&self) -> Option<&str> {
        self.node
            .as_scope()
            .and_then(|scope| *scope.ident.read())
            .map(|ident| ident.name.as_str())
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

    pub fn get_children(&self) -> Vec<BlockId> {
        self.children.read().clone()
    }

    pub fn set_parent(&self, parent_id: BlockId) {
        *self.parent.write() = Some(parent_id);
    }

    pub fn get_parent(&self) -> Option<BlockId> {
        *self.parent.read()
    }

    /// Add a type dependency to this block
    pub fn add_type_dep(&self, type_id: BlockId) {
        self.type_deps.write().insert(type_id);
    }

    /// Get all type dependencies for this block
    pub fn get_type_deps(&self) -> HashSet<BlockId> {
        self.type_deps.read().clone()
    }
}

#[derive(Debug)]
pub struct BlockRoot<'blk> {
    pub base: BlockBase<'blk>,
    pub file_name: Option<String>,
    /// Crate name from Cargo.toml [package] name
    pub crate_name: RwLock<Option<String>>,
    /// Module path relative to crate root (e.g., "utils::helpers")
    pub module_path: RwLock<Option<String>>,
}

impl<'blk> BlockRoot<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        file_name: Option<String>,
    ) -> Self {
        Self::new_with_symbol(id, node, parent, children, file_name, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        file_name: Option<String>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Root, parent, children, symbol);
        Self {
            base,
            file_name,
            crate_name: RwLock::new(None),
            module_path: RwLock::new(None),
        }
    }

    pub fn set_crate_name(&self, name: String) {
        *self.crate_name.write() = Some(name);
    }

    pub fn get_crate_name(&self) -> Option<String> {
        self.crate_name.read().clone()
    }

    pub fn set_module_path(&self, path: String) {
        *self.module_path.write() = Some(path);
    }

    pub fn get_module_path(&self) -> Option<String> {
        self.module_path.read().clone()
    }

    pub fn format(&self) -> String {
        let name = self.base.opt_get_name().unwrap_or("");
        if let Some(file_name) = &self.file_name {
            format!(
                "{}:{} {} ({})",
                self.base.kind, self.base.id, name, file_name
            )
        } else {
            format!("{}:{} {}", self.base.kind, self.base.id, name)
        }
    }
}

/// Block representing a module declaration (`mod foo` or `mod foo { ... }`)
#[derive(Debug)]
pub struct BlockModule<'blk> {
    pub base: BlockBase<'blk>,
    /// Module name (e.g., "utils" for `mod utils;`)
    pub name: String,
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
        Self::new_with_symbol(id, node, parent, children, name, is_inline, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        name: String,
        is_inline: bool,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Module, parent, children, symbol);
        Self {
            base,
            name,
            is_inline,
        }
    }

    pub fn format(&self) -> String {
        let inline_marker = if self.is_inline { " (inline)" } else { "" };
        format!(
            "{}:{} {}{}",
            self.base.kind, self.base.id, self.name, inline_marker
        )
    }
}

#[derive(Debug)]
pub struct BlockFunc<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
    pub parameters: RwLock<Vec<BlockId>>,
    pub returns: RwLock<Option<BlockId>>,
    /// Types used inside function body (excludes parameter/return types)
    /// Examples: local variable types, static method receivers (Foo::method)
    pub type_deps: RwLock<HashSet<BlockId>>,
    /// Functions/methods called by this function
    pub func_deps: RwLock<HashSet<BlockId>>,
    /// Whether this is a method (inside impl block) vs a free function
    pub is_method: AtomicBool,
}

impl<'blk> BlockFunc<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        kind: BlockKind,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with_symbol(id, node, kind, parent, children, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        kind: BlockKind,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, kind, parent, children, symbol);
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self {
            base,
            name,
            parameters: RwLock::new(Vec::new()),
            returns: RwLock::new(None),
            type_deps: RwLock::new(HashSet::new()),
            func_deps: RwLock::new(HashSet::new()),
            is_method: AtomicBool::new(false),
        }
    }

    pub fn set_is_method(&self, is_method: bool) {
        self.is_method.store(is_method, Ordering::Relaxed);
    }

    pub fn is_method(&self) -> bool {
        self.is_method.load(Ordering::Relaxed)
    }

    pub fn add_parameter(&self, param: BlockId) {
        self.parameters.write().push(param);
    }

    pub fn get_parameters(&self) -> Vec<BlockId> {
        self.parameters.read().clone()
    }

    pub fn set_returns(&self, ret: BlockId) {
        *self.returns.write() = Some(ret);
    }

    pub fn get_returns(&self) -> Option<BlockId> {
        *self.returns.read()
    }

    pub fn add_type_dep(&self, type_id: BlockId) {
        self.type_deps.write().insert(type_id);
    }

    pub fn get_type_deps(&self) -> HashSet<BlockId> {
        self.type_deps.read().clone()
    }

    pub fn add_func_dep(&self, func_id: BlockId) {
        self.func_deps.write().insert(func_id);
    }

    pub fn get_func_deps(&self) -> HashSet<BlockId> {
        self.func_deps.read().clone()
    }

    pub fn format(&self, _unit: CompileUnit<'blk>) -> String {
        format!("{}:{} {}", self.base.kind, self.base.id, self.name)
    }

    /// Format dependency entries as pseudo-children (to be rendered after real children)
    /// Returns lines like "@tdep:3 Bar" and "@fdep:5 process"
    pub fn format_deps(&self, unit: CompileUnit<'blk>) -> Vec<String> {
        let mut deps = Vec::new();

        // Add type_deps
        let type_deps = self.get_type_deps();
        if !type_deps.is_empty() {
            let mut sorted: Vec<_> = type_deps.iter().collect();
            sorted.sort();
            for dep_id in sorted {
                let dep_block = unit.bb(*dep_id);
                let dep_name = dep_block
                    .base()
                    .and_then(|b| b.opt_get_name())
                    .unwrap_or("");
                deps.push(format!("@tdep:{} {}", dep_id, dep_name));
            }
        }

        // Add func_deps
        let func_deps = self.get_func_deps();
        if !func_deps.is_empty() {
            let mut sorted: Vec<_> = func_deps.iter().collect();
            sorted.sort();
            for dep_id in sorted {
                let dep_block = unit.bb(*dep_id);
                let dep_name = dep_block
                    .base()
                    .and_then(|b| b.opt_get_name())
                    .unwrap_or("");
                deps.push(format!("@fdep:{} {}", dep_id, dep_name));
            }
        }

        deps
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
        Self::new_with_symbol(id, node, parent, children, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Call, parent, children, symbol);
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

    pub fn get_callee(&self) -> Option<BlockId> {
        *self.callee.read()
    }

    pub fn get_args(&self) -> Vec<BlockId> {
        self.args.read().clone()
    }

    pub fn format(&self) -> String {
        format!("{}:{}", self.base.kind, self.base.id)
    }
}

#[derive(Debug)]
pub struct BlockClass<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
    pub fields: RwLock<Vec<BlockId>>,
    pub methods: RwLock<Vec<BlockId>>,
}

impl<'blk> BlockClass<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with_symbol(id, node, parent, children, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Class, parent, children, symbol);
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self {
            base,
            name,
            fields: RwLock::new(Vec::new()),
            methods: RwLock::new(Vec::new()),
        }
    }

    pub fn add_field(&self, field_id: BlockId) {
        self.fields.write().push(field_id);
    }

    pub fn add_method(&self, method_id: BlockId) {
        self.methods.write().push(method_id);
    }

    pub fn get_fields(&self) -> Vec<BlockId> {
        self.fields.read().clone()
    }

    pub fn get_methods(&self) -> Vec<BlockId> {
        self.methods.read().clone()
    }

    pub fn format(&self) -> String {
        format!("{}:{} {}", self.base.kind, self.base.id, self.name)
    }
}

#[derive(Debug)]
pub struct BlockTrait<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
    pub methods: RwLock<Vec<BlockId>>,
}

impl<'blk> BlockTrait<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with_symbol(id, node, parent, children, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Trait, parent, children, symbol);
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self {
            base,
            name,
            methods: RwLock::new(Vec::new()),
        }
    }

    pub fn add_method(&self, method_id: BlockId) {
        self.methods.write().push(method_id);
    }

    pub fn get_methods(&self) -> Vec<BlockId> {
        self.methods.read().clone()
    }

    pub fn format(&self) -> String {
        format!("{}:{} {}", self.base.kind, self.base.id, self.name)
    }
}

#[derive(Debug)]
pub struct BlockImpl<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
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
        Self::new_with_symbol(id, node, parent, children, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Impl, parent, children, symbol);
        let name = base.opt_get_name().unwrap_or("").to_string();
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

    pub fn get_target(&self) -> Option<BlockId> {
        *self.target.read()
    }

    pub fn get_trait_ref(&self) -> Option<BlockId> {
        *self.trait_ref.read()
    }

    pub fn get_methods(&self) -> Vec<BlockId> {
        self.methods.read().clone()
    }

    pub fn format(&self) -> String {
        let mut parts = vec![format!("{}:{} {}", self.base.kind, self.base.id, self.name)];
        if let Some(target_id) = self.get_target() {
            parts.push(format!("@type:{}", target_id));
        }
        if let Some(trait_id) = self.get_trait_ref() {
            parts.push(format!("@trait:{}", trait_id));
        }
        parts.join(" ")
    }
}

#[derive(Debug)]
pub struct BlockEnum<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
    pub variants: RwLock<Vec<BlockId>>,
}

impl<'blk> BlockEnum<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with_symbol(id, node, parent, children, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Enum, parent, children, symbol);
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self {
            base,
            name,
            variants: RwLock::new(Vec::new()),
        }
    }

    pub fn add_variant(&self, variant_id: BlockId) {
        self.variants.write().push(variant_id);
    }

    pub fn get_variants(&self) -> Vec<BlockId> {
        self.variants.read().clone()
    }

    pub fn format(&self) -> String {
        format!("{}:{} {}", self.base.kind, self.base.id, self.name)
    }
}

#[derive(Debug)]
pub struct BlockConst<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
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
        Self::new_with_symbol(id, node, parent, children, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Const, parent, children, symbol);
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self {
            base,
            name,
            type_name: String::new(),
            type_ref: RwLock::new(None),
        }
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
    pub fn get_type_ref(&self) -> Option<BlockId> {
        *self.type_ref.read()
    }

    pub fn format(&self) -> String {
        if !self.type_name.is_empty() {
            if let Some(type_id) = *self.type_ref.read() {
                return format!(
                    "{}:{} {} @type:{} {}",
                    self.base.kind, self.base.id, self.name, type_id, self.type_name
                );
            } else {
                return format!(
                    "{}:{} {} @type {}",
                    self.base.kind, self.base.id, self.name, self.type_name
                );
            }
        }
        format!("{}:{} {}", self.base.kind, self.base.id, self.name)
    }
}

#[derive(Debug)]
pub struct BlockField<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
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
        Self::new_with_symbol(id, node, parent, children, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Field, parent, children, symbol);
        // Try to get name from the node's scope first, then from ident
        let name = base
            .opt_get_name()
            .map(|s| s.to_string())
            .or_else(|| {
                // For field nodes, the identifier child has the name
                node.as_scope()
                    .and_then(|scope| scope.opt_ident())
                    .map(|ident| ident.name.clone())
            })
            .unwrap_or_default();
        Self {
            base,
            name,
            type_name: String::new(),
            type_ref: RwLock::new(None),
        }
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
    pub fn get_type_ref(&self) -> Option<BlockId> {
        *self.type_ref.read()
    }

    pub fn format(&self) -> String {
        if !self.type_name.is_empty() {
            if let Some(type_id) = *self.type_ref.read() {
                return format!(
                    "{}:{} {} @type:{} {}",
                    self.base.kind, self.base.id, self.name, type_id, self.type_name
                );
            } else {
                return format!(
                    "{}:{} {} @type {}",
                    self.base.kind, self.base.id, self.name, self.type_name
                );
            }
        }
        format!("{}:{} {}", self.base.kind, self.base.id, self.name)
    }
}

/// Represents a single function/method parameter as its own block
#[derive(Debug)]
pub struct BlockParameter<'blk> {
    pub base: BlockBase<'blk>,
    /// Parameter name (e.g., "x", "self")
    pub name: String,
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
        Self::new_with_symbol(id, node, parent, children, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Parameter, parent, children, symbol);
        // Try to get name from the node's scope first, then from ident
        let name = base
            .opt_get_name()
            .map(|s| s.to_string())
            .or_else(|| {
                // For parameter nodes, the identifier child has the name
                node.as_scope()
                    .and_then(|scope| scope.opt_ident())
                    .map(|ident| ident.name.clone())
            })
            .unwrap_or_default();
        Self {
            base,
            name,
            type_name: String::new(),
            type_ref: RwLock::new(None),
        }
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
    pub fn get_type_ref(&self) -> Option<BlockId> {
        *self.type_ref.read()
    }

    pub fn format(&self) -> String {
        if !self.type_name.is_empty() {
            if let Some(type_id) = *self.type_ref.read() {
                return format!(
                    "{}:{} {} @type:{} {}",
                    self.base.kind, self.base.id, self.name, type_id, self.type_name
                );
            } else {
                return format!(
                    "{}:{} {} @type {}",
                    self.base.kind, self.base.id, self.name, self.type_name
                );
            }
        }
        format!("{}:{} {}", self.base.kind, self.base.id, self.name)
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
        Self::new_with_symbol(id, node, parent, children, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Return, parent, children, symbol);
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
    pub fn get_type_ref(&self) -> Option<BlockId> {
        *self.type_ref.read()
    }

    pub fn format(&self) -> String {
        if !self.type_name.is_empty() {
            if let Some(type_id) = *self.type_ref.read() {
                return format!(
                    "{}:{} @type:{} {}",
                    self.base.kind, self.base.id, type_id, self.type_name
                );
            } else {
                return format!(
                    "{}:{} @type {}",
                    self.base.kind, self.base.id, self.type_name
                );
            }
        }
        format!("{}:{}", self.base.kind, self.base.id)
    }
}

#[derive(Debug)]
pub struct BlockAlias<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
}

impl<'blk> BlockAlias<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> Self {
        Self::new_with_symbol(id, node, parent, children, None)
    }

    pub fn new_with_symbol(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        symbol: Option<&'blk Symbol>,
    ) -> Self {
        let base = BlockBase::with_symbol(id, node, BlockKind::Alias, parent, children, symbol);
        let name = base.opt_get_name().unwrap_or("").to_string();
        Self { base, name }
    }

    pub fn format(&self) -> String {
        format!("{}:{} {}", self.base.kind, self.base.id, self.name)
    }
}
