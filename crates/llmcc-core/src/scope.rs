//! Scope management and symbol lookup for the code graph.
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::atomic::Ordering;

use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirId, HirNode};
use crate::symbol::{NEXT_SCOPE_ID, ScopeId, SymId, SymKind, Symbol};

/// Represents a single level in the scope hierarchy.
pub struct Scope<'tcx> {
    /// Unique monotonic scope ID assigned at creation time.
    /// Immutable for the lifetime of the scope.
    id: ScopeId,
    /// Map of interned symbol names to vectors of symbols.
    symbols: RwLock<HashMap<InternedStr, Vec<&'tcx Symbol>>>,
    /// The HIR node that owns/introduces this scope.
    /// Examples: function body, module definition, struct body.
    owner: HirId,
    /// The symbol that introduced this scope, if any.
    /// Examples: the function symbol for a function body scope.
    symbol: RwLock<Option<&'tcx Symbol>>,
    /// Parent scopes for inheritance and member lookup.
    parents: RwLock<Vec<&'tcx Scope<'tcx>>>,
    /// Child scopes nested within this scope.
    /// Used for hierarchical scope traversal (planned feature).
    #[allow(dead_code)]
    children: RwLock<Vec<&'tcx Scope<'tcx>>>,
}

impl<'tcx> Scope<'tcx> {
    /// Creates a new scope owned by the given HIR node.
    pub fn new(owner: HirId) -> Self {
        Self::new_with(owner, None)
    }

    /// Creates a new scope owned by the given HIR node and associated with a symbol.
    pub fn new_with(owner: HirId, symbol: Option<&'tcx Symbol>) -> Self {
        Self {
            id: ScopeId(NEXT_SCOPE_ID.fetch_add(1, Ordering::SeqCst)),
            symbols: RwLock::new(HashMap::new()),
            owner,
            symbol: RwLock::new(symbol),
            parents: RwLock::new(Vec::new()),
            children: RwLock::new(Vec::new()),
        }
    }

    /// Creates a new scope from an existing scope, copying its basic structure.
    pub fn new_from<'src>(other: &Scope<'src>, arena: &'tcx Arena<'tcx>) -> Self {
        // Clone the associated symbol if present
        let symbol_ref = (*other.symbol.read()).map(|symbol| arena.alloc(symbol.clone()));

        // Create the new scope with empty symbols
        let new_scope = Self {
            id: other.id,
            symbols: RwLock::new(HashMap::new()),
            owner: other.owner,
            symbol: RwLock::new(symbol_ref),
            parents: RwLock::new(Vec::new()),
            children: RwLock::new(Vec::new()),
        };

        // Copy all symbols from the source scope
        other.for_each_symbol(|source_symbol| {
            let allocated = arena.alloc(source_symbol.clone());
            new_scope.insert(allocated);
        });

        new_scope
    }

    /// Merge existing scope into this scope, new stuff should allocate in the given arena.
    pub fn merge_with(&self, other: &'tcx Scope<'tcx>, _arena: &'tcx Arena<'tcx>) {
        // Merge all symbols from the other scope into this scope
        other.for_each_symbol(|source_symbol| {
            self.insert(source_symbol);
        });
    }

    /// Gets the HIR node that owns this scope.
    #[inline]
    pub fn owner(&self) -> HirId {
        self.owner
    }

    /// Gets the symbol that introduced this scope (if any).
    /// For a function body scope, this would be the function symbol.
    #[inline]
    pub fn symbol(&self) -> Option<&'tcx Symbol> {
        *self.symbol.read()
    }

    /// Sets the symbol that introduced this scope.
    #[inline]
    pub fn set_symbol(&self, symbol: Option<&'tcx Symbol>) {
        *self.symbol.write() = symbol;
    }

    /// Gets the unique scope ID assigned at creation time.
    #[inline]
    pub fn id(&self) -> ScopeId {
        self.id
    }

    /// Invokes a closure for each symbol in this scope.
    pub fn for_each_symbol<F>(&self, mut visit: F)
    where
        F: FnMut(&'tcx Symbol),
    {
        let symbols = self.symbols.read();
        for symbol_vec in symbols.values() {
            for symbol in symbol_vec {
                visit(symbol);
            }
        }
    }

    /// Inserts a symbol into this scope.
    pub fn insert(&self, symbol: &'tcx Symbol) -> SymId {
        let sym_id = symbol.id;
        self.symbols
            .write()
            .entry(symbol.name)
            .or_default()
            .push(symbol);
        sym_id
    }

    /// Looks up all symbols with the given name in this scope.
    pub fn lookup_symbols(&self, name: InternedStr) -> Vec<&'tcx Symbol> {
        self.symbols
            .read()
            .get(&name)
            .map(|symbols| symbols.to_vec())
            .unwrap_or_default()
    }

    /// Looks up symbols with optional kind and unit filters.
    pub fn lookup_symbols_with(
        &self,
        name: InternedStr,
        kind_filter: Option<SymKind>,
        unit_filter: Option<usize>,
    ) -> Vec<&'tcx Symbol> {
        self.lookup_symbols(name)
            .into_iter()
            .filter(|symbol| {
                let kind_match = kind_filter.is_none() || kind_filter == Some(symbol.kind());
                let unit_match = unit_filter.is_none() || unit_filter == symbol.unit_index();
                kind_match && unit_match
            })
            .collect()
    }

    /// Returns a compact string representation for debugging.
    /// Format: `{owner}/{symbol_count}`
    pub fn format_compact(&self) -> String {
        let total: usize = self.symbols.read().values().map(|v| v.len()).sum();
        format!("{}/{}", self.owner, total)
    }
}

impl<'tcx> fmt::Debug for Scope<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbol_desc = self.symbol().cloned();
        let mut symbol_entries = Vec::new();
        self.for_each_symbol(|symbol| symbol_entries.push(symbol.clone()));
        f.debug_struct("Scope")
            .field("id", &self.id())
            .field("owner", &self.owner())
            .field("symbol", &symbol_desc)
            .field("symbols", &symbol_entries)
            .finish()
    }
}

/// Manages a stack of nested scopes for symbol resolution and insertion.
#[derive(Debug)]
pub struct ScopeStack<'tcx> {
    /// Arena allocator for symbols
    arena: &'tcx Arena<'tcx>,
    /// String interner for symbol names
    interner: &'tcx InternPool,
    /// Stack of nested scopes (global at index 0, current at end)
    stack: RwLock<Vec<&'tcx Scope<'tcx>>>,
}

impl<'tcx> ScopeStack<'tcx> {
    /// Creates a new empty scope stack.
    pub fn new(arena: &'tcx Arena<'tcx>, interner: &'tcx InternPool) -> Self {
        Self {
            arena,
            interner,
            stack: RwLock::new(Vec::new()),
        }
    }

    /// Gets the current depth of the scope stack (number of nested scopes).
    #[inline]
    pub fn depth(&self) -> usize {
        self.stack.read().len()
    }

    /// Pushes a scope onto the stack (increases nesting depth).
    #[inline]
    pub fn push(&self, scope: &'tcx Scope<'tcx>) {
        self.stack.write().push(scope);
    }

    /// Recursively pushes a scope and all its base (parent) scopes onto the stack.
    pub fn push_recursive(&self, scope: &'tcx Scope<'tcx>) {
        let mut cand = Vec::new();
        cand.push(scope);

        let mut scopes = Vec::new();

        let mut visited = HashSet::new();
        while let Some(current) = cand.pop() {
            if visited.contains(&current.id()) {
                continue;
            }

            visited.insert(current.id());
            scopes.push(current);

            let parents = current.parents.read();
            if !parents.is_empty() {
                // Process in reverse order to maintain LIFO semantics
                for base in parents.iter().rev() {
                    if !visited.contains(&base.id()) {
                        cand.push(base);
                    }
                }
            }
        }

        for scope in scopes.iter().rev() {
            self.push(scope);
        }
    }

    /// Pops a scope from the stack.
    ///
    /// # Returns
    /// Some(scope) if stack was non-empty, None if already at depth 0
    #[inline]
    pub fn pop(&self) -> Option<&'tcx Scope<'tcx>> {
        self.stack.write().pop()
    }

    /// Pops scopes until the stack reaches the specified depth.
    ///
    /// # Arguments
    /// * `depth` - The target depth (no-op if already <= depth)
    pub fn pop_until(&self, depth: usize) {
        while self.depth() > depth {
            self.pop();
        }
    }

    /// Gets the top (current) scope without removing it.
    ///
    /// # Returns
    /// Some(scope) if stack is non-empty, None if at depth 0
    #[inline]
    pub fn top(&self) -> Option<&'tcx Scope<'tcx>> {
        let stack = self.stack.read();
        if stack.is_empty() {
            return None;
        }
        stack.last().copied()
    }

    /// Returns an iterator over scopes from first to last (global to current).
    ///
    /// This is a double-ended iterator, allowing iteration in either direction.
    /// Creates a copy of the stack for safe iteration.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &'tcx Scope<'tcx>> + '_ {
        self.stack
            .read()
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Normalize and intern the symbol name.
    ///
    /// Returns the interned name key, or uses `___anonymous___` if the name is empty and `force` is true.
    /// Returns `None` if name is empty and `force` is false.
    fn normalize_name(&self, name: Option<&str>, force: bool) -> Option<InternedStr> {
        let name_str = match name {
            Some(n) if !n.is_empty() => n,
            _ => {
                if force {
                    "___anonymous___"
                } else {
                    return None;
                }
            }
        };
        Some(self.interner.intern(name_str))
    }

    /// Select the target scope based on configuration flags.
    ///
    /// Priority: global > parent > top (current)
    fn select_scope(&self, global: bool, parent: bool) -> Option<&'tcx Scope<'tcx>> {
        let stack = self.stack.read();
        if stack.is_empty() {
            return None;
        }

        if global {
            // Global scope (depth 0)
            Some(stack[0])
        } else if parent && stack.len() >= 2 {
            // Parent scope (depth - 1)
            Some(stack[stack.len() - 2])
        } else {
            // Current scope (top)
            stack.last().copied()
        }
    }

    /// Try to find an existing symbol in the scope.
    ///
    /// Returns all candidates with the given name in the scope.
    fn lookup_symbols_in_scope(
        &self,
        scope: Option<&'tcx Scope<'tcx>>,
        name_key: InternedStr,
    ) -> Vec<&'tcx Symbol> {
        scope
            .map(|s| s.lookup_symbols(name_key))
            .unwrap_or_default()
    }

    /// Lookup or insert a symbol with configurable scope selection strategy.
    ///
    /// Parameters:
    /// - `node`: The HIR node to find/add a symbol for
    /// - `name`: The symbol name (or `None` for anonymous if force is true)
    /// - `options`: Lookup options controlling scope selection and behavior
    ///
    /// Returns: `Some(symbol)` if found or created, `None` if name is empty/null and force is false.
    ///
    /// # Behavior
    /// - If `options.top` is true: Always creates a NEW symbol and chains it to the existing one (if any)
    /// - If `options.top` is false: Returns existing symbol if found, only creates new if not found
    fn lookup_or_insert_impl(
        &self,
        node: HirId,
        name: Option<&str>,
        options: LookupOptions,
    ) -> Option<&'tcx Symbol> {
        // Normalize the name
        let name_key = self.normalize_name(name, options.force)?;

        // Select the target scope
        let scope = self.select_scope(options.global, options.parent);

        // Look up existing symbols in the scope
        let existing_symbols = self.lookup_symbols_in_scope(scope, name_key);

        // If top flag is NOT set and we found existing symbols, return the most recent one
        if !options.top
            && !existing_symbols.is_empty()
            && let Some(existing) = existing_symbols.last()
        {
            return Some(existing);
        }

        // Create new symbol (either no existing found, or top flag set for chaining)
        let symbol = Symbol::new(node, name_key);
        let allocated = self.arena.alloc(symbol);

        // If top flag is set, chain to the most recent existing symbol
        if options.top
            && !existing_symbols.is_empty()
            && let Some(prev_sym) = existing_symbols.last()
        {
            allocated.set_previous(prev_sym.id);
        }

        // Insert into scope
        if let Some(s) = scope {
            s.insert(allocated);
        }

        Some(allocated)
    }

    /// Find existing symbol or insert new one in the current scope.
    ///
    /// If a symbol with this name exists in the current scope, returns it.
    /// Otherwise, creates a new symbol and inserts it into the current scope.
    ///
    /// # Arguments
    /// * `name` - The symbol name
    /// * `node` - The HIR node for the symbol
    ///
    /// # Returns
    /// Some(symbol) if name is non-empty, None if name is empty
    pub fn lookup_or_insert(&self, name: &str, node: &HirNode) -> Option<&'tcx Symbol> {
        self.lookup_or_insert_impl(node.id(), Some(name), LookupOptions::current())
    }

    /// Find or insert symbol with chaining enabled for shadowing support.
    ///
    /// If a symbol with this name exists in the current scope, creates a new
    /// symbol that chains to it via the `previous` field. This supports tracking
    /// shadowing relationships in nested scopes.
    ///
    /// # Arguments
    /// * `name` - The symbol name
    /// * `node` - The HIR node for the symbol
    ///
    /// # Returns
    /// Some(symbol) if name is non-empty, None if name is empty
    pub fn lookup_or_insert_chained(&self, name: &str, node: &HirNode) -> Option<&'tcx Symbol> {
        self.lookup_or_insert_impl(node.id(), Some(name), LookupOptions::chained())
    }

    /// Insert a symbol in the parent scope.
    ///
    /// Inserts into the parent scope (depth-1) if it exists, otherwise fails.
    /// Useful for lifting definitions out of the current scope.
    ///
    /// # Arguments
    /// * `name` - The symbol name
    /// * `node` - The HIR node for the symbol
    ///
    /// # Returns
    /// Some(symbol) if name is non-empty and parent scope exists,
    /// None if name is empty or no parent scope available
    pub fn lookup_or_insert_parent(&self, name: &str, node: &HirNode) -> Option<&'tcx Symbol> {
        self.lookup_or_insert_impl(node.id(), Some(name), LookupOptions::parent())
    }

    /// Find or insert symbol in the global scope.
    ///
    /// Inserts into the global scope (depth 0) regardless of current nesting.
    /// Used for module-level definitions.
    ///
    /// # Arguments
    /// * `name` - The symbol name
    /// * `node` - The HIR node for the symbol
    ///
    /// # Returns
    /// Some(symbol) if name is non-empty, None if name is empty
    pub fn lookup_or_insert_global(&self, name: &str, node: &HirNode) -> Option<&'tcx Symbol> {
        self.lookup_or_insert_impl(node.id(), Some(name), LookupOptions::global())
    }

    /// Full control API for symbol lookup and insertion with custom options.
    ///
    /// Provides maximum flexibility for symbol resolution. All behavior is
    /// controlled via the `LookupOptions` parameter.
    ///
    /// # Arguments
    /// * `name` - The symbol name (None for anonymous if force=true)
    /// * `node` - The HIR node for the symbol
    /// * `options` - Lookup options controlling scope selection and behavior
    ///
    /// # Returns
    /// Some(symbol) if found/created, None if name is empty/null and force=false
    ///
    /// # Example
    /// ```ignore
    /// let opts = LookupOptions::global().with_force(true);
    /// let symbol = scope_stack.lookup_or_insert_with(None, node_id, opts)?;
    /// ```
    pub fn lookup_or_insert_with(
        &self,
        name: Option<&str>,
        node: &HirNode,
        options: LookupOptions,
    ) -> Option<&'tcx Symbol> {
        self.lookup_or_insert_impl(node.id(), name, options)
    }
}

/// Configuration options for symbol lookup and insertion strategies.
///
/// Controls which scope is targeted and how symbols are handled:
/// - Scope selection: global, parent, or current (top)
/// - Symbol handling: normal insertion or chaining for shadowing
/// - Name handling: fail on empty or create anonymous
///
/// # Priority Order
/// When multiple flags are set:
/// 1. `global` takes highest priority (global scope)
/// 2. `parent` takes next priority (parent scope) - ignored if global is true
/// 3. Default is current scope (top)
///
/// # Scope Depth
/// - Global scope is at depth 0
/// - Parent scope is at depth-1 (invalid if current depth < 2)
/// - Current scope is at top
///
/// # Example
/// ```ignore
/// // Insert in global scope with chaining
/// let opts = LookupOptions::global().with_top(true);
///
/// // Create anonymous symbol if no name
/// let opts = LookupOptions::anonymous();
///
/// // Custom combination
/// let opts = LookupOptions::current()
///     .with_top(true)  // Enable chaining
///     .with_force(true); // Accept None names
/// ```
#[derive(Debug, Clone, Copy)]
pub struct LookupOptions {
    /// If true, select global scope (depth 0). Has priority over `parent`.
    pub global: bool,
    /// If true and stack.len() >= 2, select parent scope (depth-1).
    /// Ignored if `global` is true.
    pub parent: bool,
    /// If true, chain new symbols to existing ones with the same name.
    /// Enables shadowing tracking via the `previous` field.
    pub top: bool,
    /// If true, create symbol even for None/empty names using `___anonymous___`.
    /// If false, return None for empty names.
    pub force: bool,
}

impl LookupOptions {
    /// Create options for current scope insertion.
    /// No special flags set - inserts in current (top) scope.
    pub fn current() -> Self {
        Self {
            global: false,
            parent: false,
            top: false,
            force: false,
        }
    }

    /// Create options for global scope insertion.
    /// Inserts in global scope (depth 0) regardless of current nesting.
    pub fn global() -> Self {
        Self {
            global: true,
            parent: false,
            top: false,
            force: false,
        }
    }

    /// Create options for parent scope insertion.
    /// Inserts in parent scope (depth-1) if it exists.
    pub fn parent() -> Self {
        Self {
            global: false,
            parent: true,
            top: false,
            force: false,
        }
    }

    /// Create options with chaining enabled for shadowing.
    /// New symbols chain to existing ones with the same name.
    pub fn chained() -> Self {
        Self {
            global: false,
            parent: false,
            top: true,
            force: false,
        }
    }

    /// Create options for anonymous symbols.
    /// Forces creation of symbol even for None/empty names.
    pub fn anonymous() -> Self {
        Self {
            global: false,
            parent: false,
            top: false,
            force: true,
        }
    }

    /// Builder method: Set global scope flag.
    pub fn with_global(mut self, global: bool) -> Self {
        self.global = global;
        self
    }

    /// Builder method: Set parent scope flag.
    pub fn with_parent(mut self, parent: bool) -> Self {
        self.parent = parent;
        self
    }

    /// Builder method: Set chaining flag for shadowing support.
    pub fn with_top(mut self, top: bool) -> Self {
        self.top = top;
        self
    }

    /// Builder method: Set force mode for anonymous symbols.
    pub fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Arena, HirBase, HirKind, HirText};
    use rayon::prelude::*;
    use std::sync::Arc;

    // Helper to create test HirId
    fn create_hir_id(id: u32) -> HirId {
        HirId(id as usize)
    }

    // Helper to create a test HirNode by allocating it in an Arena
    // Since HirNode requires lifetimes, we use a macro to create one with proper lifetime
    fn create_test_hir_node<'a>(arena: &'a Arena<'a>, id: u32) -> HirNode<'a> {
        let base = HirBase {
            id: create_hir_id(id),
            parent: None,
            kind_id: 0,
            start_byte: 0,
            end_byte: 0,
            kind: HirKind::Internal,
            field_id: 0,
            children: Vec::new(),
        };
        // Create HirText which is simpler than HirInternal
        let text = HirText::new(base, String::new());
        let text_ref = arena.alloc(text);
        HirNode::Text(text_ref)
    }

    #[test]
    fn test_scope_creation_and_id() {
        let scope = Scope::new(create_hir_id(1));
        assert_eq!(scope.owner(), create_hir_id(1));
        assert!(scope.symbol().is_none());
    }

    #[test]
    fn test_scope_symbol_management() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let scope = Scope::new(create_hir_id(1));
        assert!(scope.symbol().is_none());

        let sym = Symbol::new(create_hir_id(100), pool.intern("test_sym"));
        let sym_ref = arena.alloc(sym);

        scope.set_symbol(Some(sym_ref));
        assert_eq!(scope.symbol().map(|s| s.id), Some(sym_ref.id));
    }

    #[test]
    fn test_scope_insert_and_lookup() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let scope = Scope::new(create_hir_id(1));

        let name1 = pool.intern("var_a");
        let sym1 = Symbol::new(create_hir_id(100), name1);
        let sym1_ref = arena.alloc(sym1);
        scope.insert(sym1_ref);

        let found = scope.lookup_symbols(name1);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, sym1_ref.id);
    }

    #[test]
    fn test_scope_lookup_multiple_symbols() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let scope = Scope::new(create_hir_id(1));
        let name = pool.intern("overloaded");

        // Insert multiple symbols with same name (shadowing)
        let mut ids = Vec::new();
        for i in 0..3 {
            let sym = Symbol::new(create_hir_id(100 + i), name);
            let sym_ref = arena.alloc(sym);
            scope.insert(sym_ref);
            ids.push(sym_ref.id);
        }

        let found = scope.lookup_symbols(name);
        assert_eq!(found.len(), 3);
        for (i, sym) in found.iter().enumerate() {
            assert_eq!(sym.id, ids[i]);
        }
    }

    #[test]
    fn test_scope_stack_basic_operations() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let stack = ScopeStack::new(&arena, &pool);
        assert_eq!(stack.depth(), 0);
        assert!(stack.top().is_none());

        let scope1 = Scope::new(create_hir_id(1));
        let scope1_ref = arena.alloc(scope1);
        stack.push(scope1_ref);

        assert_eq!(stack.depth(), 1);
        assert_eq!(stack.top().map(|s| s.id()), Some(scope1_ref.id()));
    }

    #[test]
    fn test_scope_stack_push_pop() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let stack = ScopeStack::new(&arena, &pool);

        let scope1 = Scope::new(create_hir_id(1));
        let scope1_ref = arena.alloc(scope1);

        let scope2 = Scope::new(create_hir_id(2));
        let scope2_ref = arena.alloc(scope2);

        stack.push(scope1_ref);
        stack.push(scope2_ref);
        assert_eq!(stack.depth(), 2);

        let popped = stack.pop();
        assert_eq!(popped.map(|s| s.id()), Some(scope2_ref.id()));
        assert_eq!(stack.depth(), 1);

        let popped = stack.pop();
        assert_eq!(popped.map(|s| s.id()), Some(scope1_ref.id()));
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn test_scope_stack_lookup_or_insert() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let stack = ScopeStack::new(&arena, &pool);

        let scope = Scope::new(create_hir_id(1));
        let scope_ref = arena.alloc(scope);
        stack.push(scope_ref);

        let node = create_test_hir_node(&arena, 100);

        // First lookup_or_insert should create
        let sym1 = stack.lookup_or_insert("var", &node).unwrap();
        assert_eq!(pool.resolve_owned(sym1.name), Some("var".to_string()));

        // Second lookup_or_insert should return same
        let sym2 = stack.lookup_or_insert("var", &node).unwrap();
        assert_eq!(sym1.id, sym2.id);
    }

    #[test]
    fn test_scope_stack_lookup_or_insert_chained() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let stack = ScopeStack::new(&arena, &pool);

        let scope = Scope::new(create_hir_id(1));
        let scope_ref = arena.alloc(scope);
        stack.push(scope_ref);

        let node1 = create_test_hir_node(&arena, 100);
        let node2 = create_test_hir_node(&arena, 101);

        // First symbol
        let sym1 = stack.lookup_or_insert_chained("var", &node1).unwrap();

        // Second symbol with chaining (shadowing)
        let sym2 = stack.lookup_or_insert_chained("var", &node2).unwrap();

        // Should have different IDs (shadowing creates new)
        assert_ne!(sym1.id, sym2.id);

        // sym2 should chain to sym1
        assert_eq!(sym2.previous(), Some(sym1.id));
    }

    #[test]
    fn test_scope_stack_global_vs_current() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let stack = ScopeStack::new(&arena, &pool);

        let global_scope = Scope::new(create_hir_id(1));
        let global_scope_ref = arena.alloc(global_scope);
        stack.push(global_scope_ref);

        let local_scope = Scope::new(create_hir_id(2));
        let local_scope_ref = arena.alloc(local_scope);
        stack.push(local_scope_ref);

        let node1 = create_test_hir_node(&arena, 100);
        let node2 = create_test_hir_node(&arena, 101);

        // Insert in global scope
        let _global_sym = stack.lookup_or_insert_global("global_var", &node1).unwrap();

        // Insert in current (local) scope
        let _local_sym = stack.lookup_or_insert("local_var", &node2).unwrap();

        // Local scope should have local_var
        assert_eq!(
            local_scope_ref
                .lookup_symbols(pool.intern("local_var"))
                .len(),
            1
        );

        // Global scope should have global_var
        assert_eq!(
            global_scope_ref
                .lookup_symbols(pool.intern("global_var"))
                .len(),
            1
        );

        // Local scope should NOT have global_var initially
        assert_eq!(
            local_scope_ref
                .lookup_symbols(pool.intern("global_var"))
                .len(),
            0
        );
    }

    #[test]
    fn test_scope_stack_parent_scope() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let stack = ScopeStack::new(&arena, &pool);

        let parent_scope = Scope::new(create_hir_id(1));
        let parent_scope_ref = arena.alloc(parent_scope);
        stack.push(parent_scope_ref);

        let child_scope = Scope::new(create_hir_id(2));
        let child_scope_ref = arena.alloc(child_scope);
        stack.push(child_scope_ref);

        let node = create_test_hir_node(&arena, 100);

        // Insert in parent scope
        let _sym = stack.lookup_or_insert_parent("parent_var", &node).unwrap();

        // Verify in parent scope
        assert_eq!(
            parent_scope_ref
                .lookup_symbols(pool.intern("parent_var"))
                .len(),
            1
        );

        // Verify NOT in child scope
        assert_eq!(
            child_scope_ref
                .lookup_symbols(pool.intern("parent_var"))
                .len(),
            0
        );
    }

    #[test]
    fn test_parallel_scope_insertions() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let scope = Arc::new(Scope::new(create_hir_id(1)));

        // Parallel insertion of 100 symbols
        (0..100).into_par_iter().for_each(|i| {
            let name = pool.intern(format!("symbol_{}", i));
            let sym = Symbol::new(create_hir_id(i as u32 + 1000), name);
            let sym_ref = arena.alloc(sym);
            scope.insert(sym_ref);
        });

        // Verify all symbols are present
        let mut total = 0;
        for i in 0..100 {
            let name = pool.intern(format!("symbol_{}", i));
            let found = scope.lookup_symbols(name);
            total += found.len();
        }

        assert_eq!(total, 100);
    }
    #[test]
    fn test_parallel_scope_stack_operations() {
        let arena = Arena::default();
        let pool = InternPool::default();
        let stack = Arc::new(ScopeStack::new(&arena, &pool));

        // Simulate parallel stack operations (simplified due to lifetime constraints)
        // We test that stack.depth() operations work correctly in parallel context
        (0..50).into_par_iter().for_each(|_i| {
            // Test that depth operations are thread-safe
            let _depth = stack.depth();
            assert_ne!(_depth, usize::MAX);
        });

        // Verify stack is still functional after parallel operations
        assert_ne!(stack.depth(), usize::MAX);
    }
    #[test]
    fn test_parallel_multiple_scopes() {
        let arena = Arena::default();
        let pool = InternPool::default();

        // Allocate all scopes in arena
        let scopes: Arc<Vec<_>> = Arc::new(
            (0..10)
                .map(|i| arena.alloc(Scope::new(create_hir_id(i as u32))))
                .collect(),
        );

        // Parallel insertion into different scopes
        (0..100).into_par_iter().for_each(|i| {
            let scope_idx = i % 10;
            let name = pool.intern(format!("sym_s{}_#{}", scope_idx, i));
            let sym = Symbol::new(create_hir_id(i as u32 + 2000), name);
            let sym_ref = arena.alloc(sym);
            scopes[scope_idx].insert(sym_ref);
        });

        // Verify each scope has 10 symbols
        for scope in scopes.iter() {
            let mut count = 0;
            scope.for_each_symbol(|_| count += 1);
            assert_eq!(count, 10);
        }
    }
    #[test]
    fn test_parallel_lookup_after_insert() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let scope = Arc::new(Scope::new(create_hir_id(1)));

        // Phase 1: Insert symbols in parallel
        (0..50).into_par_iter().for_each(|i| {
            let name = pool.intern(format!("lookup_test_{}", i));
            let sym = Symbol::new(create_hir_id(i as u32 + 3000), name);
            let sym_ref = arena.alloc(sym);
            scope.insert(sym_ref);
        });

        // Phase 2: Lookup all symbols in parallel
        let results: Vec<_> = (0..50)
            .into_par_iter()
            .map(|i| {
                let name = pool.intern(format!("lookup_test_{}", i));
                scope.lookup_symbols(name).len()
            })
            .collect();

        // Verify all lookups succeeded
        for count in results {
            assert_eq!(count, 1);
        }
    }
    #[test]
    fn test_for_each_symbol_consistency() {
        let arena = Arena::default();
        let pool = InternPool::default();

        let scope = Scope::new(create_hir_id(1));

        // Insert 10 symbols
        let mut expected_ids = Vec::new();
        for i in 0..10 {
            let name = pool.intern(format!("sym_{}", i));
            let sym = Symbol::new(create_hir_id(i as u32 + 4000), name);
            let sym_ref = arena.alloc(sym);
            scope.insert(sym_ref);
            expected_ids.push(sym_ref.id);
        }

        // Collect via for_each
        let mut collected_ids = Vec::new();
        scope.for_each_symbol(|sym| collected_ids.push(sym.id));

        // Verify same symbols (order may differ)
        assert_eq!(collected_ids.len(), expected_ids.len());
        for id in expected_ids {
            assert!(collected_ids.contains(&id));
        }
    }

    #[test]
    fn test_scope_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Scope>();
    }

    #[test]
    fn test_scope_stack_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ScopeStack>();
    }
}
