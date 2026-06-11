//! Symbol metadata for names discovered in source code.
//!
//! A `Symbol` starts with a stable id, name, and owning HIR node. Later phases
//! attach classification, scope, type, and graph metadata as collection,
//! binding, inference, and graph building progress.
//!
//! Compact optional id fields use the same atomic encoding throughout this
//! file: `0` means `None`, and `n` means `Some(Id(n - 1))`.

use parking_lot::RwLock;
use std::fmt;
use strum_macros::{EnumIter, FromRepr};

use crate::id::BlockId;
pub use crate::id::{ScopeId, SymId, SymbolId, reset_scope_id_counter, reset_symbol_id_counter};
use crate::interner::InternedStr;
use crate::ir::HirId;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// Sentinel value for "not set" in unit/crate index.
pub const INDEX_NONE: u32 = u32::MAX;

/// Coarse classification for a named program entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, FromRepr, Default)]
#[repr(u8)]
pub enum SymKind {
    /// Kind has not been determined yet.
    #[default]
    Unknown = 0,
    /// Placeholder created for a referenced type before binding resolves it.
    UnresolvedType = 1,
    /// Package or crate-level root.
    Crate = 2,
    /// Language module or importable namespace unit.
    Module = 3,
    /// Source file symbol.
    File = 4,
    /// C++ namespace or equivalent named scope.
    Namespace = 5,
    /// Struct-like user-defined data type.
    Struct = 6,
    /// Enum type.
    Enum = 7,
    /// Free function or function-like declaration.
    Function = 8,
    /// Function owned by a type or impl/interface body.
    Method = 9,
    /// Anonymous function or closure expression.
    Closure = 10,
    /// Macro or macro-like compile-time callable.
    Macro = 11,
    /// Local, parameter, or binding variable.
    Variable = 12,
    /// Member field, property, or enum payload field.
    Field = 13,
    /// Constant item.
    Const = 14,
    /// Static item.
    Static = 15,
    /// Trait or protocol-like abstraction.
    Trait = 16,
    /// Interface abstraction.
    Interface = 17,
    /// Implementation block or equivalent extension body.
    Impl = 18,
    /// Named enum variant.
    EnumVariant = 19,
    /// Built-in primitive type symbol.
    Primitive = 20,
    /// Type alias symbol.
    TypeAlias = 21,
    /// Generic type parameter.
    TypeParameter = 22,
    /// Generic type expression, such as `Vec<T>` or `Promise<T>`.
    GenericType = 23,
    /// Synthetic compound type, such as tuple, array, union, or object shape.
    CompositeType = 24,
}

/// Small bitset for O(1) `SymKind` membership checks.
///
/// `SymKind` currently has fewer than 32 variants, so a single `u32` is enough.
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
        let bits = (1 << (SymKind::CompositeType as u32 + 1)) - 1;
        Self(bits)
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

/// All currently declared symbol kinds.
pub const SYM_KIND_ALL: SymKindSet = SymKindSet::all();

/// Kinds that can be selected while resolving a type reference.
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

/// Kinds that can be direct targets of an `impl` block.
pub const SYM_KIND_IMPL_TARGETS: SymKindSet = SymKindSet::empty()
    .with(SymKind::Struct)
    .with(SymKind::Enum);

/// Kinds that can be selected while resolving a call target.
pub const SYM_KIND_CALLABLE: SymKindSet = SymKindSet::empty()
    .with(SymKind::Struct)
    .with(SymKind::Enum)
    .with(SymKind::Trait)
    .with(SymKind::Function)
    .with(SymKind::Const);

impl SymKind {
    #[inline]
    pub fn from_u8(value: u8) -> Self {
        Self::from_repr(value).unwrap_or_default()
    }

    pub fn is_resolved(&self) -> bool {
        !matches!(self, SymKind::UnresolvedType)
    }

    pub fn is_const(&self) -> bool {
        matches!(self, SymKind::Const | SymKind::Static)
    }

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
}

/// Metadata record for one named source-code entity.
///
/// A symbol can represent declarations, references, inferred helper types, and
/// synthetic symbols used by the graph. Most relationships are stored as ids so
/// the arena can keep symbols stable and cheap to copy around.
///
/// # Thread Safety
/// `id` and `name` are immutable after construction. Other metadata is updated
/// by later phases through atomics or `RwLock` fields.
///
/// # Example
/// ```ignore
/// let symbol = Symbol::new(id, interned_name);
/// symbol.set_kind(SymKind::Function);
/// symbol.set_is_global(true);
/// ```
pub struct Symbol {
    /// Identity: monotonic id assigned when the symbol is created.
    pub id: SymId,
    /// Identity: interned symbol name used for fast lookup and comparison.
    pub name: InternedStr,
    /// Location: packed compile-unit and crate/package indexes.
    ///
    /// Encoding: low 32 bits store `unit_index`; high 32 bits store
    /// `crate_index`; `INDEX_NONE` means unset for either half.
    unit_crate_index: AtomicU64,
    /// Location: primary HIR node that introduces this symbol.
    pub owner: RwLock<HirId>,
    /// Location: additional HIR nodes that define or extend this symbol.
    ///
    /// Examples: impl blocks, declaration merging, overloads, or split type
    /// definitions. `owner` plus `defining` is the full definition set.
    pub defining: RwLock<Vec<HirId>>,
    /// Scope: scope that directly contains this symbol.
    ///
    /// Encoding: `0` means `None`, `n` means `Some(ScopeId(n - 1))`.
    pub scope: AtomicUsize,
    /// Scope: lexical or ownership parent used for upward traversal.
    pub parent_scope: RwLock<Option<ScopeId>>,
    /// Classification: compact `SymKind` tag.
    ///
    /// Encoding: stored as `u8` and decoded through `SymKind::from_u8`.
    kind: AtomicU8,
    /// Type: direct type, return type, alias target, or bound target.
    ///
    /// Encoding: `0` means `None`, `n` means `Some(SymId(n - 1))`.
    pub type_of: AtomicUsize,
    /// Graph: block corresponding to this symbol when graph building creates one.
    ///
    /// Encoding: `0` means `None`, `n` means `Some(BlockId(n - 1))`.
    pub block_id: AtomicU32,
    /// Visibility: true when this symbol can be found through global/export lookup.
    pub is_global: AtomicBool,
    /// Shadowing: previous symbol hidden or superseded by this one.
    ///
    /// Used to build a linked list for nested-scope shadowing and repeated
    /// definitions of the same name.
    pub previous: RwLock<Option<SymId>>,
    /// Type: component or related type ids owned by this symbol.
    ///
    /// Examples: tuple element types, array element type, struct field types,
    /// generic arguments, implemented interfaces, or union/object members.
    pub nested_types: RwLock<Vec<SymId>>,
    /// Ownership: symbol that owns this field/member symbol.
    ///
    /// Encoding: `0` means `None`, `n` means `Some(SymId(n - 1))`.
    pub field_of: AtomicUsize,
    /// Metadata: decorator symbols applied to this symbol.
    ///
    /// Used primarily for TypeScript/JavaScript decorators.
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
        let sym_id = crate::id::next_symbol_id();

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
        SymKind::from_u8(self.kind.load(Ordering::Relaxed))
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

            if self
                .unit_crate_index
                .compare_exchange(current, new_packed, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
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
        debug_assert!(
            crate_idx <= u32::MAX as usize,
            "crate_index exceeds u32::MAX"
        );
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

            if self
                .unit_crate_index
                .compare_exchange(current, new_packed, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
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

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn sym_kind_decode_rejects_invalid_bytes() {
        assert_eq!(SymKind::from_u8(0), SymKind::Unknown);
        assert_eq!(
            SymKind::from_u8(SymKind::CompositeType as u8),
            SymKind::CompositeType
        );
        assert_eq!(SymKind::from_u8(25), SymKind::Unknown);
        assert_eq!(SymKind::from_u8(u8::MAX), SymKind::Unknown);
    }

    #[test]
    fn sym_kind_all_contains_every_declared_kind() {
        for kind in SymKind::iter() {
            assert!(SYM_KIND_ALL.contains(kind), "SYM_KIND_ALL missing {kind:?}");
        }
    }
}
