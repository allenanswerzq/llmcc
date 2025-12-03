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

use crate::graph_builder::BlockId;
use crate::interner::InternedStr;
use crate::ir::HirId;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
use crate::interner::InternPool;

/// Global atomic counter for assigning unique symbol IDs.
/// Incremented on each new symbol creation to ensure uniqueness.
static NEXT_SYMBOL_ID: AtomicUsize = AtomicUsize::new(0);

/// Resets the global symbol ID counter to 0.
/// Use this only during testing or when resetting the entire symbol table.
#[inline]
pub fn reset_symbol_id_counter() {
    NEXT_SYMBOL_ID.store(0, Ordering::SeqCst);
}

/// Global atomic counter for assigning unique scope IDs.
/// Incremented on each new scope creation to ensure uniqueness.
pub(crate) static NEXT_SCOPE_ID: AtomicUsize = AtomicUsize::new(0);

/// Resets the global scope ID counter to 0.
/// Use this only during testing or when resetting the entire scope table.
#[inline]
pub fn reset_scope_id_counter() {
    NEXT_SCOPE_ID.store(0, Ordering::SeqCst);
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymKind {
    Unknown,
    // logical grouping for mutliple modules
    Crate,
    // logical grouping for mutiple files
    Module,
    // logaical grouping for mutliple source code blocks
    File,
    // logical grouping for multiple entities
    Namespace,
    Struct,
    Enum,
    Function,
    Method,
    Closure,
    Macro,
    Variable,
    Field,
    Const,
    Static,
    Trait,
    Impl,
    EnumVariant,
    Primitive,
    TypeAlias,
    TypeParameter,
    GenericType,
    ConstParameter,
    UnresolvedType,
}

/// Classification of dependency relationship between symbols.
/// Used to build different graph representations:
/// - Dependency graph: A depends on B (A uses B)
/// - Architecture graph: Shows data flow direction (input → func → output)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum DepKind {
    /// General dependency (A uses/references B)
    #[default]
    Uses,
    /// General dependency (A used by B)
    Used,
    /// Function parameter type (param_type → func)
    ParamType,
    /// Function return type (func → return_type)
    ReturnType,
    /// Struct/Enum implements trait (trait → struct)
    Implements,
    /// Struct field type (field_type → struct)
    FieldType,
    /// Function/method call (caller → callee)
    Calls,
    /// Type instantiation (type → user)
    Instantiates,
    /// Generic type bound (trait_bound → struct)
    TypeBound,
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
    /// Symbols that this symbol depends on with their dependency kind.
    /// Stores (target_sym_id, dep_kind) pairs for different graph representations.
    pub depends: RwLock<Vec<(SymId, DepKind)>>,
    /// Symbols that depend on this symbol with their dependency kind (reverse relation).
    /// Used for reverse lookups and impact analysis.
    pub depended: RwLock<Vec<(SymId, DepKind)>>,
    /// Previous version/definition of this symbol (for shadowing and multi-definition tracking).
    /// Used to chain multiple definitions of the same symbol in different scopes or contexts.
    /// Example: inner definition shadows outer definition in nested scope.
    /// Forms a linked list of definitions traversable via following `previous` pointers.
    pub previous: RwLock<Option<SymId>>,
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
            depends: RwLock::new(self.depends.read().clone()),
            depended: RwLock::new(self.depended.read().clone()),
            previous: RwLock::new(*self.previous.read()),
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
        let id = NEXT_SYMBOL_ID.fetch_add(1, Ordering::SeqCst);
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
            depends: RwLock::new(Vec::new()),
            depended: RwLock::new(Vec::new()),
            previous: RwLock::new(None),
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

    /// Formats the symbol
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

    /// Adds a symbol that this symbol depends on with a specific dependency kind.
    /// Ignores self-dependencies. Allows tracking multiple dependency kinds between the same symbols.
    pub fn add_depends_on(&self, sym_id: SymId, dep_kind: DepKind) {
        if sym_id == self.id {
            return;
        }
        let mut deps = self.depends.write();
        // Check if we already recorded this relationship with the same kind
        if deps
            .iter()
            .any(|(id, kind)| *id == sym_id && *kind == dep_kind)
        {
            return;
        }
        deps.push((sym_id, dep_kind));
    }

    /// Adds a symbol that depends on this symbol (reverse dependency) with a specific kind.
    /// Ignores self-dependencies. Allows multiple dependency kinds from the same source symbol.
    pub fn add_depended_by(&self, sym_id: SymId, dep_kind: DepKind) {
        if sym_id == self.id {
            return;
        }
        let mut deps = self.depended.write();
        // Check if this relationship with the same kind already exists
        if deps
            .iter()
            .any(|(id, kind)| *id == sym_id && *kind == dep_kind)
        {
            return;
        }
        deps.push((sym_id, dep_kind));
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

    /// Adds a bidirectional dependency between this symbol and another.
    pub fn add_depends(&self, other: &Symbol, ignore_kinds: Option<&[SymKind]>) {
        self.add_depends_with(other, DepKind::Uses, ignore_kinds);
    }

    /// Adds a bidirectional dependency with a specific dependency kind.
    pub fn add_depends_with(
        &self,
        other: &Symbol,
        dep_kind: DepKind,
        ignore_kinds: Option<&[SymKind]>,
    ) {
        if self.id == other.id {
            tracing::trace!("skip_dep: {} -> {} (self-depends)", self.id, other.id);
            return;
        }
        // Skip if target is in the ignore list
        if let Some(kinds) = ignore_kinds
            && kinds.iter().any(|kind| other.kind() == *kind)
        {
            tracing::trace!("skip_dep: {} -> {} (ignored kind)", self.id, other.id);
            return;
        }

        // Skip if dependency already exists with same kind
        let deps = self.depends.read();
        if deps
            .iter()
            .any(|(id, kind)| *id == other.id && *kind == dep_kind)
        {
            tracing::trace!(
                "skip_dep: {} -> {} (duplicate {:?})",
                self.id,
                other.id,
                dep_kind
            );
            return;
        }
        drop(deps);

        // Skip if circular dependency would be created
        let other_deps = other.depends.read();
        if other_deps
            .iter()
            .any(|(id, kind)| *id == self.id && *kind == dep_kind)
        {
            tracing::trace!(
                "skip_dep: {} -> {} (circular {:?})",
                self.id,
                other.id,
                dep_kind
            );
            return;
        }
        drop(other_deps);

        tracing::trace!("add_depends: {} -> {} ({:?})", self.id, other.id, dep_kind);
        self.add_depends_on(other.id, dep_kind);
        other.add_depended_by(self.id, dep_kind);
    }

    /// Gets all dependency target IDs (ignoring DepKind).
    /// For backward compatibility with code that only needs the target symbols.
    pub fn depends_ids(&self) -> Vec<SymId> {
        self.depends.read().iter().map(|(id, _)| *id).collect()
    }

    /// Gets all reverse dependency source IDs (ignoring DepKind).
    pub fn depended_ids(&self) -> Vec<SymId> {
        self.depended.read().iter().map(|(id, _)| *id).collect()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn create_test_hir_id(index: u32) -> HirId {
        HirId(index as usize)
    }

    fn create_test_intern_pool() -> InternPool {
        InternPool::default()
    }

    #[test]
    #[serial(counter_tests)]
    fn test_sym_id_creation() {
        reset_symbol_id_counter();
        let id1 = SymId(NEXT_SYMBOL_ID.fetch_add(1, Ordering::SeqCst));
        let id2 = SymId(NEXT_SYMBOL_ID.fetch_add(1, Ordering::SeqCst));
        assert_eq!(id1.0, 0);
        assert_eq!(id2.0, 1);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_sym_id_display() {
        let id = SymId(42);
        assert_eq!(id.to_string(), "42");
    }

    #[test]
    #[serial(counter_tests)]
    fn test_sym_id_equality() {
        let id1 = SymId(42);
        let id2 = SymId(42);
        let id3 = SymId(43);
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_id_creation() {
        let id = ScopeId(10);
        assert_eq!(id.0, 10);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_id_display() {
        let id = ScopeId(99);
        assert_eq!(id.to_string(), "99");
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_kind_equality() {
        assert_eq!(SymKind::Function, SymKind::Function);
        assert_ne!(SymKind::Function, SymKind::Struct);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_creation() {
        reset_symbol_id_counter();
        reset_scope_id_counter();
        let pool = create_test_intern_pool();
        let id = create_test_hir_id(1);
        let name = pool.intern("test_symbol");

        let symbol = Symbol::new(id, name);

        // Verify basic properties regardless of counter state
        assert_eq!(symbol.owner(), id);
        assert_eq!(symbol.kind(), SymKind::Unknown);
        assert!(!symbol.is_global());
        assert_eq!(symbol.unit_index(), None);
        assert_eq!(symbol.type_of(), None);
        assert_eq!(symbol.block_id(), None);
        assert_eq!(symbol.previous(), None);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_monotonic_ids() {
        // Note: This test verifies monotonic IDs but is order-dependent.
        // IDs should only increase: if this test runs after others that create symbols,
        // the ID will be higher than 1.
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let id = create_test_hir_id(1);
        let name = pool.intern("symbol");

        let sym1 = Symbol::new(id, name);
        let sym2 = Symbol::new(id, name);

        // Verify they have different IDs
        assert_ne!(sym1.id, sym2.id);
        // Verify sym2 is greater than sym1
        assert!(sym2.id.0 > sym1.id.0);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_kind_setter_getter() {
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let symbol = Symbol::new(create_test_hir_id(1), pool.intern("func"));

        symbol.set_kind(SymKind::Function);
        assert_eq!(symbol.kind(), SymKind::Function);

        symbol.set_kind(SymKind::Struct);
        assert_eq!(symbol.kind(), SymKind::Struct);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_global_flag() {
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let symbol = Symbol::new(create_test_hir_id(1), pool.intern("global_var"));

        assert!(!symbol.is_global());
        symbol.set_is_global(true);
        assert!(symbol.is_global());
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_unit_index_only_set_once() {
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let symbol = Symbol::new(create_test_hir_id(1), pool.intern("sym"));

        symbol.set_unit_index(1);
        assert_eq!(symbol.unit_index(), Some(1));

        // Second call should not change the value
        symbol.set_unit_index(2);
        assert_eq!(symbol.unit_index(), Some(1));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_type_of() {
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let symbol = Symbol::new(create_test_hir_id(1), pool.intern("var"));
        let type_id = SymId(42);

        symbol.set_type_of(type_id);
        assert_eq!(symbol.type_of(), Some(type_id));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_scope_hierarchy() {
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let symbol = Symbol::new(create_test_hir_id(1), pool.intern("sym"));
        let scope_id = ScopeId(10);
        let parent_scope_id = ScopeId(5);

        symbol.set_scope(scope_id);
        symbol.set_parent_scope(parent_scope_id);

        assert_eq!(symbol.opt_scope(), Some(scope_id));
        assert_eq!(symbol.parent_scope(), Some(parent_scope_id));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_add_dependency() {
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let sym1 = Symbol::new(create_test_hir_id(1), pool.intern("func1"));
        let sym2 = Symbol::new(create_test_hir_id(2), pool.intern("func2"));

        sym1.add_depends(&sym2, None);

        assert!(sym1.depends.read().iter().any(|(id, _)| *id == sym2.id));
        assert!(sym2.depended.read().iter().any(|(id, _)| *id == sym1.id));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_ignore_self_dependency() {
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let symbol = Symbol::new(create_test_hir_id(1), pool.intern("sym"));

        symbol.add_depends_on(symbol.id, DepKind::Uses);
        assert!(!symbol.depends.read().iter().any(|(id, _)| *id == symbol.id));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_duplicate_dependency() {
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let sym1 = Symbol::new(create_test_hir_id(1), pool.intern("sym1"));
        let sym2 = Symbol::new(create_test_hir_id(2), pool.intern("sym2"));

        sym1.add_depends_on(sym2.id, DepKind::Uses);
        sym1.add_depends_on(sym2.id, DepKind::Uses);

        // Should only have one entry with same DepKind
        assert_eq!(sym1.depends.read().len(), 1);

        // Adding with different DepKind should create a new entry
        sym1.add_depends_on(sym2.id, DepKind::Calls);
        assert_eq!(sym1.depends.read().len(), 2);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_add_defining_locations() {
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let symbol = Symbol::new(create_test_hir_id(1), pool.intern("struct"));
        let hir_id_1 = create_test_hir_id(10);
        let hir_id_2 = create_test_hir_id(20);

        symbol.add_defining(hir_id_1);
        symbol.add_defining(hir_id_2);
        symbol.add_defining(hir_id_1); // Duplicate

        let defs = symbol.defining_hir_nodes();
        assert_eq!(defs.len(), 2);
        assert!(defs.contains(&hir_id_1));
        assert!(defs.contains(&hir_id_2));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_previous_chain() {
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let sym1 = Symbol::new(create_test_hir_id(1), pool.intern("var"));
        let sym2 = Symbol::new(create_test_hir_id(2), pool.intern("var"));
        let sym3 = Symbol::new(create_test_hir_id(3), pool.intern("var"));

        sym2.set_previous(sym1.id);
        sym3.set_previous(sym2.id);

        assert_eq!(sym2.previous(), Some(sym1.id));
        assert_eq!(sym3.previous(), Some(sym2.id));
        assert_eq!(sym1.previous(), None);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_symbol_clone() {
        reset_symbol_id_counter();
        let pool = create_test_intern_pool();
        let symbol = Symbol::new(create_test_hir_id(1), pool.intern("sym"));

        symbol.set_kind(SymKind::Function);
        symbol.set_is_global(true);
        symbol.set_unit_index(0);

        let cloned = symbol.clone();

        assert_eq!(cloned.id, symbol.id);
        assert_eq!(cloned.kind(), symbol.kind());
        assert_eq!(cloned.is_global(), symbol.is_global());
        assert_eq!(cloned.unit_index(), symbol.unit_index());
    }
}
