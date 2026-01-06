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
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// Sentinel value for "not set" in unit/crate index.
pub const INDEX_NONE: u32 = u32::MAX;

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
    Interface = 17,
    Impl = 18,
    EnumVariant = 19,
    Primitive = 20,
    TypeAlias = 21,
    TypeParameter = 22,
    GenericType = 23,
    CompositeType = 24,
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
        // Set bits 0-24 (all current SymKind values)
        Self(0x01FFFFFF)
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
    .with(SymKind::Interface)
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
            SymKind::Struct
                | SymKind::Enum
                | SymKind::Trait
                | SymKind::TypeAlias
                | SymKind::Interface
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
    /// Packed unit_index (low 32 bits) and crate_index (high 32 bits).
    /// - unit_index: which compile unit (file) this symbol is defined in
    /// - crate_index: which crate/package this symbol belongs to
    /// Both use INDEX_NONE (u32::MAX) to indicate "not set".
    unit_crate_index: AtomicU64,
    /// Owning HIR node that introduces the symbol (e.g. function def, struct def).
    /// Immutable once set; represents the primary definition location.
    pub owner: RwLock<HirId>,
    /// Additional defining locations for this symbol.
    /// For example, a struct can have multiple impl blocks in different files.
    /// owner + defining together represent all locations where this symbol is defined.
    pub defining: RwLock<Vec<HirId>>,
    /// The scope that this symbol belongs to.
    /// Used to quickly find the scope during binding and type resolution.
    pub scope: AtomicUsize, // 0 = None, n = Some(ScopeId(n-1))
    /// The parent scope of this symbol (for scope hierarchy).
    /// Enables upward traversal of the scope chain.
    pub parent_scope: RwLock<Option<ScopeId>>,
    /// The kind of symbol this represents (function, struct, variable, etc.).
    /// Initially Unknown, updated as the symbol is processed.
    pub kind: AtomicU8,
    /// Optional backing type for this symbol (e.g. variable type, alias target).
    /// Set during type analysis if applicable.
    pub type_of: AtomicUsize, // 0 = None, n = Some(SymId(n-1))
    /// Optional block id associated with this symbol (for graph building).
    /// Links the symbol to its corresponding block in the code graph.
    pub block_id: AtomicU32, // 0 = None, n = Some(BlockId(n-1))
    /// Whether the symbol is globally visible/exported.
    /// Used to distinguish public symbols from private ones.
    pub is_global: AtomicBool,
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
    pub field_of: AtomicUsize, // 0 = None, n = Some(SymId(n-1))
    /// For classes/functions, tracks decorator function symbols applied to this symbol.
    /// Used primarily in TypeScript/JavaScript for @decorator syntax.
    pub decorators: RwLock<Vec<SymId>>,
}

impl Clone for Symbol {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            owner: RwLock::new(*self.owner.read()),
            name: self.name,
            unit_crate_index: AtomicU64::new(self.unit_crate_index.load(Ordering::Relaxed)),
            defining: RwLock::new(self.defining.read().clone()),
            scope: AtomicUsize::new(self.scope.load(Ordering::Relaxed)),
            parent_scope: RwLock::new(*self.parent_scope.read()),
            kind: AtomicU8::new(self.kind.load(Ordering::Relaxed)),
            type_of: AtomicUsize::new(self.type_of.load(Ordering::Relaxed)),
            block_id: AtomicU32::new(self.block_id.load(Ordering::Relaxed)),
            is_global: AtomicBool::new(self.is_global.load(Ordering::Relaxed)),
            previous: RwLock::new(*self.previous.read()),
            nested_types: RwLock::new(self.nested_types.read().clone()),
            field_of: AtomicUsize::new(self.field_of.load(Ordering::Relaxed)),
            decorators: RwLock::new(self.decorators.read().clone()),
        }
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format(None))
    }
}

impl Symbol {
    /// Pack unit_index and crate_index into a single u64.
    /// Layout: [crate_index: u32 (high)][unit_index: u32 (low)]
    #[inline]
    const fn pack_indices(unit_index: u32, crate_index: u32) -> u64 {
        ((crate_index as u64) << 32) | (unit_index as u64)
    }

    /// Unpack unit_index from the combined u64.
    #[inline]
    const fn unpack_unit_index(packed: u64) -> u32 {
        packed as u32
    }

    /// Unpack crate_index from the combined u64.
    #[inline]
    const fn unpack_crate_index(packed: u64) -> u32 {
        (packed >> 32) as u32
    }

    /// Creates a new symbol with the given HIR node owner and interned name.
    pub fn new(owner: HirId, name_key: InternedStr) -> Self {
        let id = NEXT_SYMBOL_ID.fetch_add(1, Ordering::Relaxed);
        let sym_id = SymId(id);

        Self {
            id: sym_id,
            owner: RwLock::new(owner),
            name: name_key,
            unit_crate_index: AtomicU64::new(Self::pack_indices(INDEX_NONE, INDEX_NONE)),
            defining: RwLock::new(Vec::new()),
            scope: AtomicUsize::new(0),
            parent_scope: RwLock::new(None),
            kind: AtomicU8::new(SymKind::Unknown as u8),
            type_of: AtomicUsize::new(0),
            block_id: AtomicU32::new(0),
            is_global: AtomicBool::new(false),
            previous: RwLock::new(None),
            nested_types: RwLock::new(Vec::new()),
            field_of: AtomicUsize::new(0),
            decorators: RwLock::new(Vec::new()),
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
        let v = self.scope.load(Ordering::Relaxed);
        if v == 0 { None } else { Some(ScopeId(v - 1)) }
    }

    #[inline]
    pub fn scope(&self) -> ScopeId {
        self.opt_scope().unwrap()
    }

    /// Sets the scope ID this symbol belongs to.
    #[inline]
    pub fn set_scope(&self, scope_id: ScopeId) {
        self.scope.store(scope_id.0 + 1, Ordering::Relaxed);
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
        // SAFETY: SymKind has repr(u8) implied by enum values 0-23
        unsafe { std::mem::transmute(self.kind.load(Ordering::Relaxed)) }
    }

    /// Sets the symbol kind after analysis.
    #[inline]
    pub fn set_kind(&self, kind: SymKind) {
        self.kind.store(kind as u8, Ordering::Relaxed);
    }

    /// Gets the type of this symbol (if it has one).
    /// For variables, this is their declared type.
    /// For type aliases, this is the target type.
    #[inline]
    pub fn type_of(&self) -> Option<SymId> {
        let v = self.type_of.load(Ordering::Relaxed);
        if v == 0 { None } else { Some(SymId(v - 1)) }
    }

    /// Sets the type of this symbol.
    #[inline]
    pub fn set_type_of(&self, ty: SymId) {
        tracing::trace!("setting type of symbol {} to symbol {}", self.id, ty,);
        self.type_of.store(ty.0 + 1, Ordering::Relaxed);
    }

    /// Gets the compile unit index this symbol is defined in.
    #[inline]
    pub fn unit_index(&self) -> Option<usize> {
        let packed = self.unit_crate_index.load(Ordering::Relaxed);
        match Self::unpack_unit_index(packed) {
            INDEX_NONE => None,
            v => Some(v as usize),
        }
    }

    /// Sets the compile unit index, but only if not already set.
    /// Prevents overwriting the original definition location.
    #[inline]
    pub fn set_unit_index(&self, unit_idx: usize) {
        debug_assert!(unit_idx <= u32::MAX as usize, "unit_index exceeds u32::MAX");
        let unit_idx = unit_idx as u32;
        
        loop {
            let current = self.unit_crate_index.load(Ordering::Relaxed);
            let current_unit = Self::unpack_unit_index(current);
            
            // Only set if not already set
            if current_unit != INDEX_NONE {
                return;
            }
            
            let crate_idx = Self::unpack_crate_index(current);
            let new_packed = Self::pack_indices(unit_idx, crate_idx);
            
            if self.unit_crate_index.compare_exchange(
                current,
                new_packed,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ).is_ok() {
                return;
            }
        }
    }

    /// Gets the crate/package index this symbol belongs to.
    #[inline]
    pub fn crate_index(&self) -> Option<usize> {
        let packed = self.unit_crate_index.load(Ordering::Relaxed);
        match Self::unpack_crate_index(packed) {
            INDEX_NONE => None,
            v => Some(v as usize),
        }
    }

    /// Sets the crate index, but only if not already set.
    #[inline]
    pub fn set_crate_index(&self, crate_idx: usize) {
        debug_assert!(crate_idx <= u32::MAX as usize, "crate_index exceeds u32::MAX");
        let crate_idx = crate_idx as u32;
        
        loop {
            let current = self.unit_crate_index.load(Ordering::Relaxed);
            let current_crate = Self::unpack_crate_index(current);
            
            // Only set if not already set
            if current_crate != INDEX_NONE {
                return;
            }
            
            let unit_idx = Self::unpack_unit_index(current);
            let new_packed = Self::pack_indices(unit_idx, crate_idx);
            
            if self.unit_crate_index.compare_exchange(
                current,
                new_packed,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ).is_ok() {
                return;
            }
        }
    }

    /// Checks if this symbol is globally visible/exported.
    #[inline]
    pub fn is_global(&self) -> bool {
        self.is_global.load(Ordering::Relaxed)
    }

    /// Sets the global visibility flag.
    #[inline]
    pub fn set_is_global(&self, value: bool) {
        self.is_global.store(value, Ordering::Relaxed);
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
        let v = self.block_id.load(Ordering::Relaxed);
        if v == 0 { None } else { Some(BlockId(v - 1)) }
    }

    /// Sets the block ID associated with this symbol.
    #[inline]
    pub fn set_block_id(&self, block_id: BlockId) {
        self.block_id.store(block_id.0 + 1, Ordering::Relaxed);
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
        let v = self.field_of.load(Ordering::Relaxed);
        if v == 0 { None } else { Some(SymId(v - 1)) }
    }

    /// Sets which symbol owns this field.
    #[inline]
    pub fn set_field_of(&self, owner: SymId) {
        self.field_of.store(owner.0 + 1, Ordering::Relaxed);
    }

    /// Gets the decorators applied to this symbol (for TypeScript/JavaScript @decorator syntax).
    #[inline]
    pub fn decorators(&self) -> Option<Vec<SymId>> {
        let decorators = self.decorators.read();
        if decorators.is_empty() {
            None
        } else {
            Some(decorators.clone())
        }
    }

    /// Adds a decorator to this symbol.
    #[inline]
    pub fn add_decorator(&self, decorator: SymId) {
        self.decorators.write().push(decorator);
    }
}
