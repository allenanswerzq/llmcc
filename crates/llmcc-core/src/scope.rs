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
    /// Multiple symbols with the same name are supported via shadowing chains.
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
    ///
    /// Assigns a unique monotonic scope ID and initializes an empty symbol map.
    /// The scope starts with no associated symbol (`symbol` is None).
    ///
    /// # Arguments
    /// * `owner` - The HIR node that owns this scope
    ///
    /// # Example
    /// ```ignore
    /// let scope = Scope::new(function_hir_id);
    /// assert!(scope.lookup_symbols(name).is_empty());
    /// ```
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
    ///
    /// This is useful for transferring a scope from one arena to another while
    /// preserving its owner and symbol association. If the source scope has an
    /// associated symbol, it will be cloned and allocated in the provided arena.
    /// All symbols in the source scope are also copied to the new scope.
    ///
    /// # Arguments
    /// * `other` - The scope to copy from
    /// * `arena` - The arena to allocate symbols in
    ///
    /// # Returns
    /// A new scope with the same owner, symbol association, and all symbols
    /// from the source scope copied over.
    ///
    /// # Example
    /// ```ignore
    /// // Copy a scope from per-unit arena to global arena
    /// let global_scope = Scope::new_from(&local_scope, &global_arena);
    /// // Scope is now fully populated with symbols
    /// ```
    pub fn new_from<'src>(other: &Scope<'src>, arena: &'tcx Arena<'tcx>) -> Self {
        // Clone the associated symbol if present
        let symbol_ref = if let Some(symbol) = *other.symbol.read() {
            Some(arena.alloc(symbol.clone()))
        } else {
            None
        };

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
    pub fn merge_with<'src>(&self, other: &Scope<'src>, arena: &'tcx Arena<'tcx>) {
        // Merge all symbols from the other scope into this scope
        other.for_each_symbol(|source_symbol| {
            let allocated = arena.alloc(source_symbol.clone());
            self.insert(allocated);
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
    ///
    /// Calls the visitor function for every symbol stored in this scope.
    /// Useful for iteration without collecting all symbols into a vector.
    ///
    /// # Arguments
    /// * `visit` - A closure that accepts a reference to each symbol
    ///
    /// # Example
    /// ```ignore
    /// scope.for_each_symbol(|symbol| {
    ///     println!("Symbol: {:?}", symbol.name);
    /// });
    /// ```
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
    ///
    /// If multiple symbols have the same name, they are stored in a vector
    /// to support overloading and shadowing. Later symbols can reference
    /// earlier ones via their `previous` field.
    ///
    /// # Arguments
    /// * `symbol` - The symbol to insert
    ///
    /// # Returns
    /// The symbol's ID
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
    ///
    /// Returns a vector of all matching symbols. Use the `previous` field
    /// to traverse the shadowing chain if needed.
    ///
    /// # Arguments
    /// * `name` - The interned symbol name to look up
    ///
    /// # Returns
    /// Vector of symbols (may be empty if name not found)
    ///
    /// # Example
    /// ```ignore
    /// let symbols = scope.lookup_symbols(name_key);
    /// // symbols[0] is the first definition
    /// // symbols[last] is the most recent definition (shadows earlier ones)
    /// ```
    pub fn lookup_symbols(&self, name: InternedStr) -> Vec<&'tcx Symbol> {
        self.symbols
            .read()
            .get(&name)
            .map(|symbols| symbols.to_vec())
            .unwrap_or_default()
    }

    /// Looks up symbols with optional kind and unit filters.
    ///
    /// Filters symbols by their kind (e.g., Function, Struct) and compile unit.
    /// None for a filter means "no filter" (matches anything).
    ///
    /// # Arguments
    /// * `name` - The interned symbol name
    /// * `kind_filter` - Optional kind to filter by (None matches all)
    /// * `unit_filter` - Optional unit index to filter by (None matches all)
    ///
    /// # Returns
    /// Filtered vector of matching symbols
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
///
/// The scope stack handles hierarchical scope traversal and symbol lookup across
/// the scope hierarchy. It supports multiple lookup strategies via `LookupOptions`:
/// - Global scope (depth 0) - module level
/// - Parent scope (depth-1) - enclosing scope
/// - Current scope (top) - innermost scope
///
/// Symbols are created and stored in scopes, with support for shadowing via chains.
///
/// # Arena Allocation
/// Symbols are allocated in an arena for efficient memory management and stable pointers.
///
/// # Example
/// ```ignore
/// let mut scope_stack = ScopeStack::new(arena, interner);
/// let global_scope = Scope::new(global_hir_id);
/// scope_stack.push(&global_scope);
///
/// // Insert in current scope
/// let symbol = scope_stack.lookup_or_insert(node_id, "my_var");
///
/// // Insert in global scope
/// let global_sym = scope_stack.lookup_or_insert_global(node_id, "MODULE");
/// ```
#[derive(Debug)]
pub struct ScopeStack<'tcx> {
    /// Arena allocator for symbols
    arena: &'tcx Arena<'tcx>,
    /// String interner for symbol names
    interner: &'tcx InternPool,
    /// Stack of nested scopes (global at index 0, current at end)
    stack: Vec<&'tcx Scope<'tcx>>,
}

impl<'tcx> ScopeStack<'tcx> {
    /// Creates a new empty scope stack.
    ///
    /// # Arguments
    /// * `arena` - The arena allocator for symbols
    /// * `interner` - The string interner for symbol names
    ///
    /// # Example
    /// ```ignore
    /// let scope_stack = ScopeStack::new(arena, interner);
    /// assert_eq!(scope_stack.depth(), 0);
    /// ```
    pub fn new(arena: &'tcx Arena<'tcx>, interner: &'tcx InternPool) -> Self {
        Self {
            arena,
            interner,
            stack: Vec::new(),
        }
    }

    /// Gets the current depth of the scope stack (number of nested scopes).
    #[inline]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Pushes a scope onto the stack (increases nesting depth).
    #[inline]
    pub fn push(&mut self, scope: &'tcx Scope<'tcx>) {
        self.stack.push(scope);
    }

    /// Recursively pushes a scope and all its base (parent) scopes onto the stack.
    pub fn push_recursive(&mut self, scope: &'tcx Scope<'tcx>) {
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
    pub fn pop(&mut self) -> Option<&'tcx Scope<'tcx>> {
        self.stack.pop()
    }

    /// Pops scopes until the stack reaches the specified depth.
    ///
    /// # Arguments
    /// * `depth` - The target depth (no-op if already <= depth)
    pub fn pop_until(&mut self, depth: usize) {
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
        if self.stack.is_empty() {
            return None;
        }
        self.stack.last().copied()
    }

    /// Returns an iterator over scopes from first to last (global to current).
    ///
    /// This is a double-ended iterator, allowing iteration in either direction.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &'tcx Scope<'tcx>> + '_ {
        self.stack.iter().copied()
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
        if self.stack.is_empty() {
            return None;
        }

        if global {
            // Global scope (depth 0)
            Some(self.stack[0])
        } else if parent && self.stack.len() >= 2 {
            // Parent scope (depth - 1)
            Some(self.stack[self.stack.len() - 2])
        } else {
            // Current scope (top)
            self.top()
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
        if !options.top && !existing_symbols.is_empty() {
            if let Some(existing) = existing_symbols.last() {
                return Some(existing);
            }
        }

        // Create new symbol (either no existing found, or top flag set for chaining)
        let symbol = Symbol::new(node, name_key);
        let allocated = self.arena.alloc(symbol);

        // If top flag is set, chain to the most recent existing symbol
        if options.top && !existing_symbols.is_empty() {
            if let Some(prev_sym) = existing_symbols.last() {
                allocated.set_previous(prev_sym.id);
            }
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

    fn create_test_hir_id(index: u32) -> HirId {
        HirId(index)
    }

    fn create_test_intern_pool() -> InternPool {
        InternPool::default()
    }

    #[test]
    fn test_lookup_options_current() {
        let opts = LookupOptions::current();
        assert!(!opts.global);
        assert!(!opts.parent);
        assert!(!opts.top);
        assert!(!opts.force);
    }

    #[test]
    fn test_lookup_options_global() {
        let opts = LookupOptions::global();
        assert!(opts.global);
        assert!(!opts.parent);
        assert!(!opts.top);
        assert!(!opts.force);
    }

    #[test]
    fn test_lookup_options_parent() {
        let opts = LookupOptions::parent();
        assert!(!opts.global);
        assert!(opts.parent);
        assert!(!opts.top);
        assert!(!opts.force);
    }

    #[test]
    fn test_lookup_options_chained() {
        let opts = LookupOptions::chained();
        assert!(!opts.global);
        assert!(!opts.parent);
        assert!(opts.top);
        assert!(!opts.force);
    }

    #[test]
    fn test_lookup_options_anonymous() {
        let opts = LookupOptions::anonymous();
        assert!(!opts.global);
        assert!(!opts.parent);
        assert!(!opts.top);
        assert!(opts.force);
    }

    #[test]
    fn test_lookup_options_builder_pattern() {
        let opts = LookupOptions::current()
            .with_global(true)
            .with_top(true)
            .with_force(true);

        assert!(opts.global);
        assert!(!opts.parent);
        assert!(opts.top);
        assert!(opts.force);
    }

    #[test]
    fn test_scope_creation() {
        let id = create_test_hir_id(1);
        let scope = Scope::new(id);

        assert_eq!(scope.owner(), id);
        assert!(scope.symbol().is_none());
    }

    #[test]
    fn test_scope_insert_and_lookup() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let scope = Scope::new(create_test_hir_id(1));
        let name = pool.intern("test_var");

        let symbol = Symbol::new(create_test_hir_id(10), name);
        scope.insert(&symbol);

        let found = scope.lookup_symbols(name);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, symbol.id);
    }

    #[test]
    fn test_scope_lookup_multiple_symbols_same_name() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let scope = Scope::new(create_test_hir_id(1));
        let name = pool.intern("var");

        let sym1 = Symbol::new(create_test_hir_id(10), name);
        let sym2 = Symbol::new(create_test_hir_id(11), name);

        scope.insert(&sym1);
        scope.insert(&sym2);

        let found = scope.lookup_symbols(name);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn test_scope_lookup_nonexistent() {
        let pool = create_test_intern_pool();
        let scope = Scope::new(create_test_hir_id(1));
        let name = pool.intern("nonexistent");

        let found = scope.lookup_symbols(name);
        assert!(found.is_empty());
    }

    #[test]
    fn test_scope_format_compact() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let scope = Scope::new(create_test_hir_id(1));
        let name = pool.intern("var");

        let symbol = Symbol::new(create_test_hir_id(10), name);
        scope.insert(&symbol);

        let formatted = scope.format_compact();
        assert!(formatted.contains("/1")); // 1 symbol
    }

    #[test]
    fn test_scope_stack_creation() {
        crate::symbol::reset_symbol_id_counter();
        // ScopeStack requires proper Arena setup which is complex in tests
        // Tests for ScopeStack methods are better done in integration tests
    }

    #[test]
    fn test_lookup_options_priority_global_over_parent() {
        let opts = LookupOptions::global().with_parent(true);
        // When global is true, parent should be ignored in actual scope selection
        assert!(opts.global);
        assert!(opts.parent);
    }

    #[test]
    fn test_scope_symbol_relationship() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let scope = Scope::new(create_test_hir_id(1));
        let name = pool.intern("func");

        let symbol = Symbol::new(create_test_hir_id(10), name);
        scope.set_symbol(Some(&symbol));

        assert_eq!(scope.symbol().unwrap().id, symbol.id);
    }

    #[test]
    fn test_scope_lookup_with_filters() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let scope = Scope::new(create_test_hir_id(1));
        let name = pool.intern("item");

        let sym = Symbol::new(create_test_hir_id(10), name);
        sym.set_kind(SymKind::Function);
        sym.set_unit_index(0);
        scope.insert(&sym);

        // Lookup with matching filter
        let found = scope.lookup_symbols_with(name, Some(SymKind::Function), Some(0));
        assert_eq!(found.len(), 1);

        // Lookup with non-matching kind filter
        let found = scope.lookup_symbols_with(name, Some(SymKind::Struct), Some(0));
        assert!(found.is_empty());

        // Lookup with non-matching unit filter
        let found = scope.lookup_symbols_with(name, Some(SymKind::Function), Some(1));
        assert!(found.is_empty());
    }

    #[test]
    fn test_scope_unique_lookup() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let scope = Scope::new(create_test_hir_id(1));
        let name = pool.intern("unique_item");

        let sym = Symbol::new(create_test_hir_id(10), name);
        sym.set_kind(SymKind::Struct);
        sym.set_unit_index(0);
        scope.insert(&sym);

        let found = scope.lookup_symbols(name);
        assert!(!found.is_empty());
        assert_eq!(found[0].id, sym.id);
    }

    #[test]
    fn test_lookup_options_all_combinations() {
        let combinations = vec![
            (false, false, false, false),
            (true, false, false, false),
            (false, true, false, false),
            (false, false, true, false),
            (false, false, false, true),
            (true, true, true, true),
        ];

        for (global, parent, top, force) in combinations {
            let opts = LookupOptions {
                global,
                parent,
                top,
                force,
            };

            assert_eq!(opts.global, global);
            assert_eq!(opts.parent, parent);
            assert_eq!(opts.top, top);
            assert_eq!(opts.force, force);
        }
    }

    #[test]
    fn test_scope_new_from_basic() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let arena = crate::ir::Arena::default();

        // Create source scope with some symbols
        let source_scope = Scope::new(create_test_hir_id(1));
        let name = pool.intern("test_var");

        let symbol = Symbol::new(create_test_hir_id(10), name);
        source_scope.insert(&symbol);

        // Clone to new arena
        let new_scope = Scope::new_from(&source_scope, &arena);

        // Verify scope ID is preserved
        assert_eq!(new_scope.id(), source_scope.id());

        // Verify owner is the same
        assert_eq!(new_scope.owner(), source_scope.owner());

        // Verify symbols are copied
        let found = new_scope.lookup_symbols(name);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, symbol.id);

        // TODO:
        // Verify this new_scope is ineed allocated in the new arena
        // let mut found_in_arena = false;
        // for scope in arena.iter_mut_scope() {
        //     if scope.id() == new_scope.id() {
        //         found_in_arena = true;
        //         break;
        //     }
        // }
        // assert!(found_in_arena);
    }

    #[test]
    fn test_scope_new_from_multiple_symbols() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let arena = crate::ir::Arena::default();

        // Create source scope with multiple symbols
        let source_scope = Scope::new(create_test_hir_id(1));
        let name1 = pool.intern("var1");
        let name2 = pool.intern("var2");

        let symbol1 = Symbol::new(create_test_hir_id(10), name1);
        let symbol2 = Symbol::new(create_test_hir_id(11), name2);

        source_scope.insert(&symbol1);
        source_scope.insert(&symbol2);

        // Clone to new arena
        let new_scope = Scope::new_from(&source_scope, &arena);

        // Verify all symbols are copied
        let found1 = new_scope.lookup_symbols(name1);
        let found2 = new_scope.lookup_symbols(name2);

        assert_eq!(found1.len(), 1);
        assert_eq!(found2.len(), 1);
        assert_eq!(found1[0].id, symbol1.id);
        assert_eq!(found2[0].id, symbol2.id);
    }

    #[test]
    fn test_scope_new_from_with_associated_symbol() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let arena = crate::ir::Arena::default();

        // Create source scope with an associated symbol
        let source_scope = Scope::new(create_test_hir_id(1));
        let assoc_name = pool.intern("scope_symbol");
        let assoc_symbol = Symbol::new(create_test_hir_id(20), assoc_name);

        source_scope.set_symbol(Some(&assoc_symbol));

        // Add some regular symbols too
        let var_name = pool.intern("var");
        let var_symbol = Symbol::new(create_test_hir_id(30), var_name);
        source_scope.insert(&var_symbol);

        // Clone to new arena
        let new_scope = Scope::new_from(&source_scope, &arena);

        // Verify associated symbol is cloned
        assert!(new_scope.symbol().is_some());
        assert_eq!(new_scope.symbol().unwrap().id, assoc_symbol.id);

        // Verify regular symbols are also cloned
        let found_vars = new_scope.lookup_symbols(var_name);
        assert_eq!(found_vars.len(), 1);
        assert_eq!(found_vars[0].id, var_symbol.id);
    }

    #[test]
    fn test_scope_new_from_preserves_symbol_metadata() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let arena = crate::ir::Arena::default();

        // Create source scope with symbol having metadata
        let source_scope = Scope::new(create_test_hir_id(1));
        let name = pool.intern("typed_var");

        let symbol = Symbol::new(create_test_hir_id(10), name);
        symbol.set_kind(SymKind::Function);
        symbol.set_unit_index(5);
        symbol.set_is_global(true);

        source_scope.insert(&symbol);

        // Clone to new arena
        let new_scope = Scope::new_from(&source_scope, &arena);

        // Verify metadata is preserved
        let found = new_scope.lookup_symbols(name);
        assert_eq!(found.len(), 1);

        let cloned = found[0];
        assert_eq!(cloned.id, symbol.id);
        assert_eq!(cloned.kind(), SymKind::Function);
        assert_eq!(cloned.unit_index(), Some(5));
        assert!(cloned.is_global());
    }

    #[test]
    fn test_scope_new_from_empty_scope() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let arena = crate::ir::Arena::default();

        // Create empty source scope
        let source_scope = Scope::new(create_test_hir_id(1));

        // Clone to new arena
        let new_scope = Scope::new_from(&source_scope, &arena);

        // Verify basic properties are copied even when empty
        assert_eq!(new_scope.id(), source_scope.id());
        assert_eq!(new_scope.owner(), source_scope.owner());
        assert!(new_scope.symbol().is_none());
    }

    #[test]
    fn test_scope_merge_with_basic() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let arena = crate::ir::Arena::default();

        // Create target scope with some symbols
        let target_scope = Scope::new(create_test_hir_id(1));
        let name1 = pool.intern("target_var");
        let symbol1 = Symbol::new(create_test_hir_id(10), name1);
        target_scope.insert(&symbol1);

        // Create source scope to merge
        let source_scope = Scope::new(create_test_hir_id(2));
        let name2 = pool.intern("source_var");
        let symbol2 = Symbol::new(create_test_hir_id(20), name2);
        source_scope.insert(&symbol2);

        // Merge source into target
        target_scope.merge_with(&source_scope, &arena);

        // Verify both symbols are in target scope
        let found1 = target_scope.lookup_symbols(name1);
        let found2 = target_scope.lookup_symbols(name2);

        assert_eq!(found1.len(), 1);
        assert_eq!(found2.len(), 1);
        assert_eq!(found1[0].id, symbol1.id);
        assert_eq!(found2[0].id, symbol2.id);
    }

    #[test]
    fn test_scope_merge_with_multiple_symbols() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let arena = crate::ir::Arena::default();

        // Create target scope
        let target_scope = Scope::new(create_test_hir_id(1));

        // Create source scope with multiple symbols
        let source_scope = Scope::new(create_test_hir_id(2));
        let name1 = pool.intern("var1");
        let name2 = pool.intern("var2");
        let name3 = pool.intern("var3");

        let symbol1 = Symbol::new(create_test_hir_id(10), name1);
        let symbol2 = Symbol::new(create_test_hir_id(11), name2);
        let symbol3 = Symbol::new(create_test_hir_id(12), name3);

        source_scope.insert(&symbol1);
        source_scope.insert(&symbol2);
        source_scope.insert(&symbol3);

        // Merge source into target
        target_scope.merge_with(&source_scope, &arena);

        // Verify all symbols are in target scope
        assert_eq!(target_scope.lookup_symbols(name1).len(), 1);
        assert_eq!(target_scope.lookup_symbols(name2).len(), 1);
        assert_eq!(target_scope.lookup_symbols(name3).len(), 1);
    }

    #[test]
    fn test_scope_merge_with_same_name_symbols() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let arena = crate::ir::Arena::default();

        // Create target scope with a symbol
        let target_scope = Scope::new(create_test_hir_id(1));
        let name = pool.intern("shared_var");
        let symbol1 = Symbol::new(create_test_hir_id(10), name);
        target_scope.insert(&symbol1);

        // Create source scope with symbol of same name
        let source_scope = Scope::new(create_test_hir_id(2));
        let symbol2 = Symbol::new(create_test_hir_id(20), name);
        source_scope.insert(&symbol2);

        // Merge source into target
        target_scope.merge_with(&source_scope, &arena);

        // Verify both symbols are present (supporting shadowing)
        let found = target_scope.lookup_symbols(name);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, symbol1.id);
        assert_eq!(found[1].id, symbol2.id);
    }

    #[test]
    fn test_scope_merge_with_preserves_metadata() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let arena = crate::ir::Arena::default();

        // Create target scope
        let target_scope = Scope::new(create_test_hir_id(1));

        // Create source scope with symbol having metadata
        let source_scope = Scope::new(create_test_hir_id(2));
        let name = pool.intern("metadata_var");
        let symbol = Symbol::new(create_test_hir_id(20), name);
        symbol.set_kind(SymKind::Function);
        symbol.set_unit_index(5);
        symbol.set_is_global(true);
        source_scope.insert(&symbol);

        // Merge source into target
        target_scope.merge_with(&source_scope, &arena);

        // Verify metadata is preserved
        let found = target_scope.lookup_symbols(name);
        assert_eq!(found.len(), 1);

        let merged = found[0];
        assert_eq!(merged.id, symbol.id);
        assert_eq!(merged.kind(), SymKind::Function);
        assert_eq!(merged.unit_index(), Some(5));
        assert!(merged.is_global());
    }

    #[test]
    fn test_scope_merge_with_empty_source() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let arena = crate::ir::Arena::default();

        // Create target scope with a symbol
        let target_scope = Scope::new(create_test_hir_id(1));
        let name = pool.intern("target_var");
        let symbol = Symbol::new(create_test_hir_id(10), name);
        target_scope.insert(&symbol);

        // Create empty source scope
        let source_scope = Scope::new(create_test_hir_id(2));

        // Merge empty source into target
        target_scope.merge_with(&source_scope, &arena);

        // Verify target scope is unchanged
        let found = target_scope.lookup_symbols(name);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, symbol.id);
    }

    #[test]
    fn test_scope_merge_with_into_empty_target() {
        crate::symbol::reset_symbol_id_counter();
        crate::symbol::reset_scope_id_counter();

        let pool = create_test_intern_pool();
        let arena = crate::ir::Arena::default();

        // Create empty target scope
        let target_scope = Scope::new(create_test_hir_id(1));

        // Create source scope with symbols
        let source_scope = Scope::new(create_test_hir_id(2));
        let name1 = pool.intern("var1");
        let name2 = pool.intern("var2");

        let symbol1 = Symbol::new(create_test_hir_id(10), name1);
        let symbol2 = Symbol::new(create_test_hir_id(11), name2);

        source_scope.insert(&symbol1);
        source_scope.insert(&symbol2);

        // Merge source into empty target
        target_scope.merge_with(&source_scope, &arena);

        // Verify all symbols from source are now in target
        let found1 = target_scope.lookup_symbols(name1);
        let found2 = target_scope.lookup_symbols(name2);

        assert_eq!(found1.len(), 1);
        assert_eq!(found2.len(), 1);
        assert_eq!(found1[0].id, symbol1.id);
        assert_eq!(found2[0].id, symbol2.id);
    }
}
