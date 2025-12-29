//! Symbol and scope management for the code graph.
//!
//! This module defines the core data structures for tracking named entities (symbols) in source code:
//! - `Symbol`: Represents a named entity (function, struct, variable, etc.) with metadata
//! - `SymId`: Unique identifier for symbols
//! - `ScopeId`: Unique identifier for scopes
//!
//! Symbols are allocated in an arena for efficient memory management and are thread-safe via RwLock.
//! Names are interned for fast equality comparisons.

use parking_lot::RwLock;
use std::fmt;
use strum_macros::EnumIter;

use crate::graph_builder::BlockId;
use crate::interner::InternedStr;
use crate::ir::HirId;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Global atomic counter for assigning unique symbol IDs.
/// Incremented on each new symbol creation to ensure uniqueness.
static NEXT_SYMBOL_ID: AtomicUsize = AtomicUsize::new(0);

/// Resets the global symbol ID counter to 0.
/// Use this only during testing or when resetting the entire symbol table.
#[inline]
pub fn reset_symbol_id_counter() {
    NEXT_SYMBOL_ID.store(0, Ordering::Relaxed);
}

/// Global atomic counter for assigning unique scope IDs.
/// Incremented on each new scope creation to ensure uniqueness.
pub(crate) static NEXT_SCOPE_ID: AtomicUsize = AtomicUsize::new(0);

/// Resets the global scope ID counter to 0.
/// Use this only during testing or when resetting the entire scope table.
#[inline]
pub fn reset_scope_id_counter() {
    NEXT_SCOPE_ID.store(0, Ordering::Relaxed);
}

/// Unique identifier for symbols within a compilation unit.
/// Symbols are allocated sequentially, starting from ID 1.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct SymId(pub usize);

impl std::fmt::Display for SymId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for scopes within a compilation unit.
/// Scopes are allocated sequentially, starting from ID 1.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct ScopeId(pub usize);

impl std::fmt::Display for ScopeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Classification of what kind of named entity a symbol represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, Default)]
#[repr(u8)]
pub enum SymKind {
    #[default]
    Unknown = 0,
    UnresolvedType = 1,
    Crate = 2,
    Module = 3,
    File = 4,
    Namespace = 5,
    Struct = 6,
    Enum = 7,
    Function = 8,
    Method = 9,
    Closure = 10,
    Macro = 11,
    Variable = 12,
    Field = 13,
    Const = 14,
    Static = 15,
    Trait = 16,
    Impl = 17,
    EnumVariant = 18,
    Primitive = 19,
    TypeAlias = 20,
    TypeParameter = 21,
    GenericType = 22,
    CompositeType = 23,
}

/// A bitset representing a set of SymKind values for efficient O(1) containment checks.
/// Uses a u32 internally since we have < 32 SymKind variants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SymKindSet(u32);

impl SymKindSet {
    /// Create an empty set.
    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Create a set containing all kinds.
    #[inline]
    pub const fn all() -> Self {
        // Set bits 0-23 (all current SymKind values)
        Self(0x00FFFFFF)
    }

    /// Create a set from a single kind.
    #[inline]
    pub const fn from_kind(kind: SymKind) -> Self {
        Self(1 << (kind as u32))
    }

    /// Create a set from multiple kinds using const builder pattern.
    #[inline]
    pub const fn with(self, kind: SymKind) -> Self {
        Self(self.0 | (1 << (kind as u32)))
    }

    /// Check if the set contains a kind (O(1) operation).
    #[inline]
    pub const fn contains(&self, kind: SymKind) -> bool {
        (self.0 & (1 << (kind as u32))) != 0
    }

    /// Check if the set is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

/// Pre-computed constant for all symbol kinds (used when no filtering needed).
pub const SYM_KIND_ALL: SymKindSet = SymKindSet::all();

/// Pre-computed constant for type kinds (used in type resolution).
pub const SYM_KIND_TYPES: SymKindSet = SymKindSet::empty()
    .with(SymKind::Struct)
    .with(SymKind::Enum)
    .with(SymKind::Trait)
    .with(SymKind::Function)
    .with(SymKind::Const)
    .with(SymKind::Static)
    .with(SymKind::Primitive)
    .with(SymKind::GenericType)
    .with(SymKind::CompositeType)
    .with(SymKind::TypeAlias)
    .with(SymKind::Namespace)
    .with(SymKind::TypeParameter);

/// Pre-computed constant for impl target kinds.
pub const SYM_KIND_IMPL_TARGETS: SymKindSet = SymKindSet::empty()
    .with(SymKind::Struct)
    .with(SymKind::Enum);

/// Pre-computed constant for callable kinds.
pub const SYM_KIND_CALLABLE: SymKindSet = SymKindSet::empty()
    .with(SymKind::Struct)
    .with(SymKind::Enum)
    .with(SymKind::Trait)
    .with(SymKind::Function)
    .with(SymKind::Const);

impl SymKind {
    pub fn is_resolved(&self) -> bool {
        !matches!(self, SymKind::UnresolvedType)
    }

    pub fn is_const(&self) -> bool {
        matches!(self, SymKind::Const | SymKind::Static)
    }

    /// Checks if the symbol kind represents a user-defined type (struct, enum, trait, type alias).
    /// These are types that are explicitly defined in user code, not primitives or generics.
    /// Checks if the symbol kind represents a user-defined type.
    /// These are types that appear in type annotations and can have impl blocks.
    pub fn is_defined_type(&self) -> bool {
        matches!(
            self,
            SymKind::Struct | SymKind::Enum | SymKind::Trait | SymKind::TypeAlias
        )
    }

    /// Returns kinds that can be looked up as types in type annotations.
    /// Used for resolving type references in function signatures, fields, etc.
    #[deprecated(note = "Use SYM_KIND_TYPES constant instead")]
    pub fn type_kinds() -> Vec<SymKind> {
        vec![
            SymKind::Struct,
            SymKind::Enum,
            SymKind::Trait,
            SymKind::Function,
            SymKind::Const,
            SymKind::Static,
            SymKind::Primitive,
            SymKind::GenericType,
            SymKind::CompositeType,
            SymKind::TypeAlias,
            SymKind::Namespace,
            SymKind::TypeParameter,
        ]
    }

    /// Returns kinds that can be targets of impl blocks (impl X for Target).
    /// In Rust, you can impl traits for structs and enums.
    #[deprecated(note = "Use SYM_KIND_IMPL_TARGETS constant instead")]
    pub fn impl_target_kinds() -> Vec<SymKind> {
        vec![SymKind::Struct, SymKind::Enum]
    }

    /// Returns kinds that can be called like functions.
    #[deprecated(note = "Use SYM_KIND_CALLABLE constant instead")]
    pub fn callable_kinds() -> Vec<SymKind> {
        vec![
            SymKind::Struct,
            SymKind::Enum,
            SymKind::Trait,
            SymKind::Function,
            SymKind::Const,
        ]
    }
}

/// Represents a named entity in source code.
///
/// Symbols track metadata about functions, structs, variables, and other named elements.
/// Each symbol has:
/// - An immutable unique ID
/// - A name (interned for fast comparison)
/// - Metadata (kind, location, type, dependencies)
/// - Relationships to other symbols (dependencies, scope hierarchy)
///
/// Symbols support shadowing and multi-definition tracking via the `previous` field,
/// which forms a chain of symbol definitions in nested scopes.
///
/// # Thread Safety
/// Most fields use `RwLock` for thread-safe interior mutability.
/// The ID and name are immutable once created.
///
/// # Example
/// ```ignore
/// let symbol = Symbol::new(id, interned_name);
/// symbol.set_kind(SymKind::Function);
/// symbol.set_is_global(true);
/// ```
pub struct Symbol {
    /// Monotonic id assigned when the symbol is created.
    pub id: SymId,
    /// Interned key for the symbol name, used for fast lookup and comparison.
    /// Interned names allow O(1) equality checks.
    pub name: InternedStr,
    /// Which compile unit this symbol is defined in.
    /// May be updated during compilation if the symbol spans multiple files.
    /// NOTE: compile unit doesn't mean a single file, it can be multiple files combined.
    pub unit_index: RwLock<Option<usize>>,
    /// Owning HIR node that introduces the symbol (e.g. function def, struct def).
    /// Immutable once set; represents the primary definition location.
    pub owner: RwLock<HirId>,
    /// Additional defining locations for this symbol.
    /// For example, a struct can have multiple impl blocks in different files.
    /// owner + defining together represent all locations where this symbol is defined.
    pub defining: RwLock<Vec<HirId>>,
    /// The scope that this symbol belongs to.
    /// Used to quickly find the scope during binding and type resolution.
    pub scope: RwLock<Option<ScopeId>>,
    /// The parent scope of this symbol (for scope hierarchy).
    /// Enables upward traversal of the scope chain.
    pub parent_scope: RwLock<Option<ScopeId>>,
    /// The kind of symbol this represents (function, struct, variable, etc.).
    /// Initially Unknown, updated as the symbol is processed.
    pub kind: RwLock<SymKind>,
    /// Optional backing type for this symbol (e.g. variable type, alias target).
    /// Set during type analysis if applicable.
    pub type_of: RwLock<Option<SymId>>,
    /// Optional block id associated with this symbol (for graph building).
    /// Links the symbol to its corresponding block in the code graph.
    pub block_id: RwLock<Option<BlockId>>,
    /// Whether the symbol is globally visible/exported.
    /// Used to distinguish public symbols from private ones.
    pub is_global: RwLock<bool>,
    /// Previous version/definition of this symbol (for shadowing and multi-definition tracking).
    /// Used to chain multiple definitions of the same symbol in different scopes or contexts.
    /// Example: inner definition shadows outer definition in nested scope.
    /// Forms a linked list of definitions traversable via following `previous` pointers.
    pub previous: RwLock<Option<SymId>>,
    /// For compound types (tuple, array, struct, enum), tracks the types of nested components.
    /// For tuple types: element types in order.
    /// For struct/enum: field types in declaration order.
    /// For array types: single element type.
    pub nested_types: RwLock<Vec<SymId>>,
    /// For field symbols, tracks which symbol owns this field (parent struct, enum, or object).
    /// Set to the symbol that contains/defines this field.
    /// Examples: enum variant's FieldOf is the enum; struct field's FieldOf is the struct;
    /// tuple field (by index) FieldOf is the tuple/value being accessed.
    pub field_of: RwLock<Option<SymId>>,
}

impl Clone for Symbol {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            owner: RwLock::new(*self.owner.read()),
            name: self.name,
            unit_index: RwLock::new(*self.unit_index.read()),
            defining: RwLock::new(self.defining.read().clone()),
            scope: RwLock::new(*self.scope.read()),
            parent_scope: RwLock::new(*self.parent_scope.read()),
            kind: RwLock::new(*self.kind.read()),
            type_of: RwLock::new(*self.type_of.read()),
            block_id: RwLock::new(*self.block_id.read()),
            is_global: RwLock::new(*self.is_global.read()),
            previous: RwLock::new(*self.previous.read()),
            nested_types: RwLock::new(self.nested_types.read().clone()),
            field_of: RwLock::new(*self.field_of.read()),
        }
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format(None))
    }
}

impl Symbol {
    /// Creates a new symbol with the given HIR node owner and interned name.
    pub fn new(owner: HirId, name_key: InternedStr) -> Self {
        let id = NEXT_SYMBOL_ID.fetch_add(1, Ordering::Relaxed);
        let sym_id = SymId(id);

        Self {
            id: sym_id,
            owner: RwLock::new(owner),
            name: name_key,
            unit_index: RwLock::new(None),
            defining: RwLock::new(Vec::new()),
            scope: RwLock::new(None),
            parent_scope: RwLock::new(None),
            kind: RwLock::new(SymKind::Unknown),
            type_of: RwLock::new(None),
            block_id: RwLock::new(None),
            is_global: RwLock::new(false),
            previous: RwLock::new(None),
            nested_types: RwLock::new(Vec::new()),
            field_of: RwLock::new(None),
        }
    }

    /// Gets the owner HIR node of this symbol.
    #[inline]
    pub fn owner(&self) -> HirId {
        *self.owner.read()
    }

    #[inline]
    pub fn id(&self) -> SymId {
        self.id
    }

    /// Sets the owner HIR node of this symbol.
    #[inline]
    pub fn set_owner(&self, owner: HirId) {
        *self.owner.write() = owner;
    }

    /// Formats the symbol with basic information
    pub fn format(&self, interner: Option<&crate::interner::InternPool>) -> String {
        let kind = format!("{:?}", self.kind());
        if let Some(interner) = interner {
            if let Some(name) = interner.resolve_owned(self.name) {
                format!("[{}:{}] {}", self.id.0, kind, name)
            } else {
                format!("[{}:{}]?", self.id.0, kind)
            }
        } else {
            format!("[{}:{}]", self.id.0, kind)
        }
    }

    /// Gets the scope ID this symbol belongs to.
    #[inline]
    pub fn opt_scope(&self) -> Option<ScopeId> {
        *self.scope.read()
    }

    #[inline]
    pub fn scope(&self) -> ScopeId {
        self.scope.read().unwrap()
    }

    /// Sets the scope ID this symbol belongs to.
    #[inline]
    pub fn set_scope(&self, scope_id: ScopeId) {
        *self.scope.write() = Some(scope_id);
    }

    /// Gets the parent scope ID in the scope hierarchy.
    #[inline]
    pub fn parent_scope(&self) -> Option<ScopeId> {
        *self.parent_scope.read()
    }

    /// Sets the parent scope ID in the scope hierarchy.
    #[inline]
    pub fn set_parent_scope(&self, scope_id: ScopeId) {
        *self.parent_scope.write() = Some(scope_id);
    }

    /// Gets the symbol kind (function, struct, variable, etc.).
    #[inline]
    pub fn kind(&self) -> SymKind {
        *self.kind.read()
    }

    /// Sets the symbol kind after analysis.
    #[inline]
    pub fn set_kind(&self, kind: SymKind) {
        *self.kind.write() = kind;
    }

    /// Gets the type of this symbol (if it has one).
    /// For variables, this is their declared type.
    /// For type aliases, this is the target type.
    #[inline]
    pub fn type_of(&self) -> Option<SymId> {
        *self.type_of.read()
    }

    /// Sets the type of this symbol.
    #[inline]
    pub fn set_type_of(&self, ty: SymId) {
        tracing::trace!("setting type of symbol {} to symbol {}", self.id, ty,);
        *self.type_of.write() = Some(ty);
    }

    /// Gets the compile unit index this symbol is defined in.
    #[inline]
    pub fn unit_index(&self) -> Option<usize> {
        *self.unit_index.read()
    }

    /// Sets the compile unit index, but only if not already set.
    /// Prevents overwriting the original definition location.
    #[inline]
    pub fn set_unit_index(&self, file: usize) {
        let mut unit_index = self.unit_index.write();
        if unit_index.is_none() {
            *unit_index = Some(file);
        }
    }

    /// Checks if this symbol is globally visible/exported.
    #[inline]
    pub fn is_global(&self) -> bool {
        *self.is_global.read()
    }

    /// Sets the global visibility flag.
    #[inline]
    pub fn set_is_global(&self, value: bool) {
        *self.is_global.write() = value;
    }

    /// Adds a HIR node as an additional definition location.
    /// Prevents duplicate entries.
    pub fn add_defining(&self, id: HirId) {
        let mut defs = self.defining.write();
        if !defs.contains(&id) {
            defs.push(id);
        }
    }

    /// Gets all HIR nodes that define this symbol.
    pub fn defining_hir_nodes(&self) -> Vec<HirId> {
        self.defining.read().clone()
    }

    /// Gets the block ID associated with this symbol.
    #[inline]
    pub fn block_id(&self) -> Option<BlockId> {
        *self.block_id.read()
    }

    /// Sets the block ID associated with this symbol.
    #[inline]
    pub fn set_block_id(&self, block_id: BlockId) {
        *self.block_id.write() = Some(block_id);
    }

    /// Gets the previous definition of this symbol (for shadowing).
    /// Symbols with the same name in nested scopes form a chain via this field.
    #[inline]
    pub fn previous(&self) -> Option<SymId> {
        *self.previous.read()
    }

    /// Sets the previous definition of this symbol.
    /// Used to build shadowing chains when a symbol name is reused in a nested scope.
    #[inline]
    pub fn set_previous(&self, sym_id: SymId) {
        *self.previous.write() = Some(sym_id);
    }

    /// Gets the nested types for compound types (tuples, arrays, structs, enums).
    /// Returns None if no nested types have been set, Some(vec) otherwise.
    #[inline]
    pub fn nested_types(&self) -> Option<Vec<SymId>> {
        let types = self.nested_types.read();
        if types.is_empty() {
            None
        } else {
            Some(types.clone())
        }
    }

    /// Adds a type to the nested types list for compound types.
    /// For tuples/arrays, this is in element order. For structs/enums, in field order.
    #[inline]
    pub fn add_nested_type(&self, ty: SymId) {
        self.nested_types.write().push(ty);
    }

    /// Replaces all nested types with a new list.
    #[inline]
    pub fn set_nested_types(&self, types: Vec<SymId>) {
        *self.nested_types.write() = types;
    }

    /// Gets which symbol owns this field (parent struct, enum, or object being accessed).
    #[inline]
    pub fn field_of(&self) -> Option<SymId> {
        *self.field_of.read()
    }

    /// Sets which symbol owns this field.
    #[inline]
    pub fn set_field_of(&self, owner: SymId) {
        *self.field_of.write() = Some(owner);
    }
}
