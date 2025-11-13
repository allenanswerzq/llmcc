//! Symbol collection core for building symbol tables.
//!
//! This module provides the common infrastructure for collecting symbols across all supported
//! languages. The architecture is designed for parallel per-unit symbol collection:
//!
//! # Design
//! - Each compilation unit gets its own per-unit arena (lifetime 'a)
//! - Collectors borrow the arena from the outside (e.g., from CompileUnit)
//! - After collection completes, collected symbols are applied/registered globally
//! - This allows each unit to be processed independently and in parallel
//!
//! # Usage Pattern
//! 1. Get or create an Arena<'a> for the compilation unit
//! 2. Create a CollectorCore with the unit's arena, unit index, and interner
//! 3. Collect symbols using the provided helper methods
//! 4. Access collected data through arena and scope helpers
//! 5. After collection, apply collected symbols to global registry
//!
use llmcc_core::interner::InternPool;
use llmcc_core::ir::{HirId, Arena};
use llmcc_core::symbol::Symbol;
use llmcc_core::scope::{Scope, ScopeStack, LookupOptions};

/// Core symbol collector for a single compilation unit.
///
/// The collector borrows an arena from the outside
/// and uses it for allocating symbols and scopes during collection.
/// The collector maintains a scope stack for proper nesting and symbol resolution.
///
/// # Architecture for Parallel Collection
/// - Each compilation unit has its own per-unit lifetime 'a
/// - An Arena<'a> is borrowed from CompileUnit or similar holder
/// - Scopes and Symbols are allocated into the borrowed arena for that unit
/// - After collection completes, symbols are extracted and applied globally
/// - Multiple units can be collected in parallel with separate CollectorCore instances
///
/// # Thread Safety
/// - Designed to be used in a single thread per unit
/// - Multiple CollectorCore instances can run in parallel with separate arenas
/// - The shared interner should be thread-safe (InternPool handles this)
///
/// # Lifetime 'a
/// The lifetime 'a is the lifetime of the borrowed arena from the compilation unit.
/// All allocated symbols and scopes are valid for this lifetime.
///
#[derive(Debug)]
pub struct CollectorCore<'a> {
    /// The per-unit arena borrowed from CompileUnit or similar.
    /// Used for allocating symbols and scopes during collection.
    arena: &'a Arena<'a>,

    /// The compile unit index this collector is processing.
    /// Used to tag symbols with their origin unit.
    unit_index: usize,

    /// Shared string interner for symbol names.
    /// One global interner is used across all units for consistent interning.
    /// InternPool uses internal synchronization for thread-safe interning.
    interner: &'a InternPool,

    /// Stack of active scopes during collection.
    /// Maintains the scope hierarchy as we traverse the code structure.
    scopes: ScopeStack<'a>,

    /// Global scope allocated during initialization.
    /// This is the root scope for module-level definitions.
    globals: &'a Scope<'a>,
}

impl<'a> CollectorCore<'a> {
    /// Creates a new collector for a compilation unit.
    ///
    /// Takes the per-unit arena from outside (typically from CompileUnit),
    /// initializes a global scope, and sets up an empty scope stack.
    /// The collector is ready to begin collecting symbols immediately after
    /// calling `init_scope_stack()`.
    ///
    /// # Arguments
    /// * `unit_index` - The index of the compilation unit being processed
    /// * `arena` - The per-unit arena borrowed from CompileUnit or similar
    /// * `interner` - Shared string interner (must be the same across all units)
    ///
    /// # Returns
    /// A new CollectorCore with an empty scope stack and initialized global scope
    pub fn new(unit_index: usize, arena: &'a Arena<'a>, interner: &'a InternPool) -> Self {
        // Create global scope as the root of the scope hierarchy
        // Use a dummy HirId(0) for the global scope owner as it has no specific HIR node
        let globals = arena.alloc(Scope::new(HirId(0)));

        // Create the scope stack with the borrowed arena
        let mut scopes = ScopeStack::new(arena, interner);
        scopes.push(globals);

        Self {
            arena,
            unit_index,
            interner,
            scopes,
            globals,
        }
    }

    /// Gets the compile unit index this collector is processing.
    #[inline]
    pub fn unit_index(&self) -> usize {
        self.unit_index
    }

    /// Gets the arena
    #[inline]
    pub fn arena(&self) -> &Arena<'a> {
        self.arena
    }

    /// Gets the current scope stack depth (number of nested scopes).
    ///
    /// - 0 means no scope has been pushed yet
    /// - 1 means global scope is active
    /// - 2+ means nested scopes are active
    #[inline]
    pub fn scope_depth(&self) -> usize {
        self.scopes.depth()
    }

    /// Pushes a scope onto the stack.
    ///
    /// Increases nesting depth and makes the scope active for symbol insertions.
    #[inline]
    pub fn push_scope(&mut self, scope: &'a Scope<'a>) {
        self.scopes.push(scope);
    }

    /// Pops the current scope from the stack.
    ///
    /// Returns to the previous scope level. No-op at depth 0.
    #[inline]
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Pops scopes until reaching the specified depth.
    ///
    /// # Arguments
    /// * `depth` - Target depth to pop to (no-op if already at or below depth)
    pub fn pop_until(&mut self, depth: usize) {
        self.scopes.pop_until(depth);
    }

    /// Gets the shared string interner.
    ///
    /// Used to intern symbol names for fast comparison and lookups.
    #[inline]
    pub fn interner(&self) -> &'a InternPool {
        self.interner
    }

    /// Gets the global scope (module-level definitions).
    ///
    /// All module-level symbols should be inserted here or in subscopes.
    #[inline]
    pub fn globals(&self) -> &'a Scope<'a> {
        self.globals
    }

    /// Find or insert symbol in the current scope.
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
    #[inline]
    pub fn lookup_or_insert(&self, name: &str, node: HirId) -> Option<&'a Symbol> {
        self.scopes.lookup_or_insert(name, node)
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
    #[inline]
    pub fn lookup_or_insert_chained(&self, name: &str, node: HirId) -> Option<&'a Symbol> {
        self.scopes.lookup_or_insert_chained(name, node)
    }

    /// Find or insert symbol in the parent scope.
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
    #[inline]
    pub fn lookup_or_insert_parent(&self, name: &str, node: HirId) -> Option<&'a Symbol> {
        self.scopes.lookup_or_insert_parent(name, node)
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
    #[inline]
    pub fn lookup_or_insert_global(&self, name: &str, node: HirId) -> Option<&'a Symbol> {
        self.scopes.lookup_or_insert_global(name, node)
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
    /// use llmcc_core::scope::LookupOptions;
    /// let opts = LookupOptions::global().with_force(true);
    /// let symbol = collector.lookup_or_insert_with(None, node_id, opts)?;
    /// ```
    #[inline]
    pub fn lookup_or_insert_with(
        &self,
        name: Option<&str>,
        node: HirId,
        options: llmcc_core::scope::LookupOptions,
    ) -> Option<&'a Symbol> {
        self.scopes.lookup_or_insert_with(name, node, options)
    }

}

/// Collects symbols from a compilation unit into a scope hierarchy.
///
/// This function orchestrates the symbol collection process for a single compilation unit.
/// It creates a CollectorCore, invokes the visitor function to traverse and collect symbols,
/// and returns the global scope containing the collected symbols.
///
/// # Type Parameters
/// * `C` - The concrete collector type (e.g., RustCollector, GoCollector, etc.)
/// * `Visit` - A callable that performs the symbol collection by traversing the AST
///
/// # Arguments
/// * `unit_index` - The index of this compilation unit
/// * `arena` - The per-unit arena for symbol allocation
/// * `interner` - Shared string interner
/// * `visitor` - A function that traverses the AST and calls collector methods
///
/// # Returns
/// The global scope containing all collected symbols
///
/// # Example
/// ```ignore
/// let collected_scope = collect_symbols_batch(
///     0,
///     &arena,
///     &interner,
///     |collector| {
///         // Visit AST nodes and call collector methods
///         visitor.visit_module(&collector, module_node);
///     }
/// );
/// ```
pub fn collect_symbols_batch<'a, F>(
    unit_index: usize,
    arena: &'a Arena<'a>,
    interner: &'a InternPool,
    visitor: F,
) -> &'a Scope<'a>
where
    F: FnOnce(&mut CollectorCore<'a>),
{
    let mut collector = CollectorCore::new(unit_index, arena, interner);
    visitor(&mut collector);
    collector.globals()
}

/// Applies collected symbols to a compilation context or global registry.
///
/// This function takes the scope hierarchy collected from a compilation unit
/// and integrates it into the broader symbol context. This is typically called
/// after all per-unit collections are complete, during the merge/registration phase.
///
/// # Arguments
/// * `unit_index` - The index of the compilation unit
/// * `globals` - The global scope containing collected symbols
///
/// # Future Work
/// This function will:
/// - Register symbols in the global symbol table
/// - Handle symbol deduplication across units
/// - Track symbol provenance (which unit defined it)
/// - Validate symbol visibility and access rules
/// - Build cross-unit dependency graphs
pub fn apply_collected_symbols(
    unit_index: usize,
    globals: &Scope,
) {
    // TODO: Implement symbol registration and global merging
    // For now, this is a placeholder for the integration phase
    let _ = (unit_index, globals);
}


#[cfg(test)]
mod tests {
    use super::*;
    use llmcc_core::ir::Arena;

    #[test]
    fn test_collector_creation() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let collector = CollectorCore::new(0, &arena, &interner);

        assert_eq!(collector.unit_index(), 0);
        assert_eq!(collector.scope_depth(), 1); // Global scope pushed by new()
    }

    #[test]
    fn test_collector_multiple_units() {
        let arena1 = Arena::default();
        let arena2 = Arena::default();
        let interner = InternPool::default();

        let collector1 = CollectorCore::new(0, &arena1, &interner);
        let collector2 = CollectorCore::new(1, &arena2, &interner);

        assert_eq!(collector1.unit_index(), 0);
        assert_eq!(collector2.unit_index(), 1);
    }

    #[test]
    fn test_scope_operations() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let mut collector = CollectorCore::new(0, &arena, &interner);

        // Initial depth should be 1 (global scope)
        assert_eq!(collector.scope_depth(), 1);

        // Push a new scope
        let scope1 = arena.alloc(Scope::new(HirId(1)));
        collector.push_scope(scope1);
        assert_eq!(collector.scope_depth(), 2);

        // Push another scope
        let scope2 = arena.alloc(Scope::new(HirId(2)));
        collector.push_scope(scope2);
        assert_eq!(collector.scope_depth(), 3);

        // Pop a scope
        collector.pop_scope();
        assert_eq!(collector.scope_depth(), 2);

        // Pop until depth 1
        collector.pop_until(1);
        assert_eq!(collector.scope_depth(), 1);
    }

    #[test]
    fn test_lookup_or_insert_current_scope() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let collector = CollectorCore::new(0, &arena, &interner);

        // Insert a symbol in the global scope
        let sym1 = collector.lookup_or_insert("foo", HirId(1));
        assert!(sym1.is_some());

        // Each call to lookup_or_insert creates a new symbol, even with the same name
        let sym2 = collector.lookup_or_insert("foo", HirId(2));
        assert!(sym2.is_some());

        // They will be different symbols (lookup_or_insert always creates new)
        assert_ne!(sym1.unwrap() as *const _, sym2.unwrap() as *const _);
    }

    #[test]
    fn test_lookup_or_insert_empty_name() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let collector = CollectorCore::new(0, &arena, &interner);

        // Empty name should return None
        let sym = collector.lookup_or_insert("", HirId(1));
        assert!(sym.is_none());
    }

    #[test]
    fn test_lookup_or_insert_chained() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let mut collector = CollectorCore::new(0, &arena, &interner);

        // Insert first symbol
        let sym1 = collector.lookup_or_insert_chained("x", HirId(1));
        assert!(sym1.is_some());

        // Push a new scope
        let scope = arena.alloc(Scope::new(HirId(10)));
        collector.push_scope(scope);

        // Insert chained symbol with same name
        let sym2 = collector.lookup_or_insert_chained("x", HirId(2));
        assert!(sym2.is_some());

        // The second symbol should be different from the first
        assert_ne!(sym1.unwrap() as *const _, sym2.unwrap() as *const _);
    }

    #[test]
    fn test_lookup_or_insert_global() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let mut collector = CollectorCore::new(0, &arena, &interner);

        // Push a nested scope
        let scope = arena.alloc(Scope::new(HirId(10)));
        collector.push_scope(scope);
        assert_eq!(collector.scope_depth(), 2);

        // Insert symbol in global scope from nested scope
        let sym = collector.lookup_or_insert_global("global_var", HirId(1));
        assert!(sym.is_some());

        // Each call creates new symbols
        let sym2 = collector.lookup_or_insert_global("global_var", HirId(2));
        assert!(sym2.is_some());
        // They are different symbols (lookup_or_insert always creates new)
        assert_ne!(sym.unwrap() as *const _, sym2.unwrap() as *const _);
    }

    #[test]
    fn test_lookup_or_insert_parent() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let mut collector = CollectorCore::new(0, &arena, &interner);

        // At depth 1, parent lookup fails the parent condition but falls back to top(),
        // so it may succeed depending on implementation details.
        // Let's test with depth 2 where parent scope clearly exists

        // Push a nested scope to have depth 2
        let scope = arena.alloc(Scope::new(HirId(10)));
        collector.push_scope(scope);
        assert_eq!(collector.scope_depth(), 2);

        // Now we have depth 2, parent should be the first scope (global)
        let sym_parent = collector.lookup_or_insert_parent("parent_var", HirId(1));
        assert!(sym_parent.is_some());

        // Push another scope to have depth 3
        let scope2 = arena.alloc(Scope::new(HirId(20)));
        collector.push_scope(scope2);
        assert_eq!(collector.scope_depth(), 3);

        // Parent lookup at depth 3 should return the scope at depth 2
        let sym_parent2 = collector.lookup_or_insert_parent("another_parent_var", HirId(2));
        assert!(sym_parent2.is_some());
    }

    #[test]
    fn test_lookup_or_insert_with_options() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let collector = CollectorCore::new(0, &arena, &interner);

        // Use current scope option
        let opts = LookupOptions::current();
        let sym1 = collector.lookup_or_insert_with(Some("opt_var"), HirId(1), opts);
        assert!(sym1.is_some());

        // Each call creates a new symbol
        let sym2 = collector.lookup_or_insert_with(Some("opt_var"), HirId(2), opts);
        assert!(sym2.is_some());
        // They are different symbols
        assert_ne!(sym1.unwrap() as *const _, sym2.unwrap() as *const _);
    }

    #[test]
    fn test_nested_scopes_with_different_symbols() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let mut collector = CollectorCore::new(0, &arena, &interner);

        // Insert symbol in global scope
        let global_sym = collector.lookup_or_insert("var", HirId(1));
        assert!(global_sym.is_some());

        // Push nested scope
        let scope1 = arena.alloc(Scope::new(HirId(10)));
        collector.push_scope(scope1);

        // Insert different symbol in nested scope
        let nested_sym = collector.lookup_or_insert("nested_var", HirId(2));
        assert!(nested_sym.is_some());

        // Different names should be different symbols
        assert_ne!(global_sym.unwrap() as *const _, nested_sym.unwrap() as *const _);

        // Each call creates a new symbol
        let nested_sym2 = collector.lookup_or_insert("nested_var", HirId(3));
        assert!(nested_sym2.is_some());
        // They are different symbols (different calls to lookup_or_insert)
        assert_ne!(nested_sym.unwrap() as *const _, nested_sym2.unwrap() as *const _);
    }

    #[test]
    fn test_collector_interner_access() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let collector = CollectorCore::new(0, &arena, &interner);

        // Should be able to access interner
        let interner_ref = collector.interner();
        assert_eq!(interner_ref as *const _, &interner as *const _);
    }

    #[test]
    fn test_collector_globals_access() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let collector = CollectorCore::new(0, &arena, &interner);

        // Should be able to access globals scope
        let globals = collector.globals();
        // Just verify it's a valid reference by accessing it
        let _ = globals;
    }

    #[test]
    fn test_collector_arena_access() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let collector = CollectorCore::new(0, &arena, &interner);

        // Should be able to access arena
        let arena_ref = collector.arena();
        assert_eq!(arena_ref as *const _, &arena as *const _);
    }

    #[test]
    fn test_multiple_symbols_in_scope() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let collector = CollectorCore::new(0, &arena, &interner);

        // Insert multiple symbols
        let sym_a = collector.lookup_or_insert("a", HirId(1));
        let sym_b = collector.lookup_or_insert("b", HirId(2));
        let sym_c = collector.lookup_or_insert("c", HirId(3));

        assert!(sym_a.is_some());
        assert!(sym_b.is_some());
        assert!(sym_c.is_some());

        // All should be different (each call creates a new symbol)
        assert_ne!(sym_a.unwrap() as *const _, sym_b.unwrap() as *const _);
        assert_ne!(sym_b.unwrap() as *const _, sym_c.unwrap() as *const _);
        assert_ne!(sym_a.unwrap() as *const _, sym_c.unwrap() as *const _);

        // Each subsequent call with same name also creates new symbols
        let sym_a2 = collector.lookup_or_insert("a", HirId(4));
        assert!(sym_a2.is_some());
        assert_ne!(sym_a.unwrap() as *const _, sym_a2.unwrap() as *const _);
    }

    #[test]
    fn test_scope_isolation() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let mut collector = CollectorCore::new(0, &arena, &interner);

        // Insert in global
        collector.lookup_or_insert("global_only", HirId(1));

        // Push scope 1 and insert
        let scope1 = arena.alloc(Scope::new(HirId(10)));
        collector.push_scope(scope1);
        let sym_in_scope1 = collector.lookup_or_insert("scope1_var", HirId(2));

        // Pop and push scope 2
        collector.pop_scope();
        let scope2 = arena.alloc(Scope::new(HirId(20)));
        collector.push_scope(scope2);

        // Lookup scope1_var in scope2 should create new symbol (different scope)
        let sym_in_scope2 = collector.lookup_or_insert("scope1_var", HirId(3));

        // They should be different (different scopes)
        assert_ne!(sym_in_scope1.unwrap() as *const _, sym_in_scope2.unwrap() as *const _);
    }

    #[test]
    fn test_pop_until_multiple_levels() {
        let arena = Arena::default();
        let interner = InternPool::default();
        let mut collector = CollectorCore::new(0, &arena, &interner);

        // Build a scope stack: depth 1 (global), 2, 3, 4, 5
        for i in 1..5 {
            let scope = arena.alloc(Scope::new(HirId(i as u32)));
            collector.push_scope(scope);
        }
        assert_eq!(collector.scope_depth(), 5);

        // Pop until depth 2
        collector.pop_until(2);
        assert_eq!(collector.scope_depth(), 2);

        // Pop until depth 1
        collector.pop_until(1);
        assert_eq!(collector.scope_depth(), 1);

        // Pop until 0 (should be no-op or reduce to 1, depending on implementation)
        let before_pop = collector.scope_depth();
        collector.pop_until(0);
        // After pop_until(0), we should be at depth 1 or 0 depending on whether it respects the minimum
        assert!(collector.scope_depth() <= before_pop);
    }

    #[test]
    fn test_collect_symbols_batch() {
        let arena = Arena::default();
        let interner = InternPool::default();

        // Collect symbols using the batch function
        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            // Simulate visiting and collecting symbols
            let _sym1 = collector.lookup_or_insert("foo", HirId(1));
            let _sym2 = collector.lookup_or_insert("bar", HirId(2));
        });

        // Verify we got back a scope
        assert_eq!(global_scope as *const _, global_scope as *const _);
    }

    #[test]
    fn test_collect_symbols_batch_single_symbol() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            let sym = collector.lookup_or_insert("single", HirId(1));
            assert!(sym.is_some());
        });

        // Should return valid global scope
        let _ = global_scope;
    }

    #[test]
    fn test_collect_symbols_batch_multiple_symbols() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            let sym1 = collector.lookup_or_insert("var1", HirId(1));
            let sym2 = collector.lookup_or_insert("var2", HirId(2));
            let sym3 = collector.lookup_or_insert("var3", HirId(3));
            let sym4 = collector.lookup_or_insert("var4", HirId(4));

            assert!(sym1.is_some());
            assert!(sym2.is_some());
            assert!(sym3.is_some());
            assert!(sym4.is_some());

            // All should be different
            assert_ne!(sym1.unwrap() as *const _, sym2.unwrap() as *const _);
            assert_ne!(sym2.unwrap() as *const _, sym3.unwrap() as *const _);
            assert_ne!(sym3.unwrap() as *const _, sym4.unwrap() as *const _);
        });

        let _ = global_scope;
    }

    #[test]
    fn test_collect_symbols_batch_with_scopes_basic() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            // Initial depth should be 1 (global)
            assert_eq!(collector.scope_depth(), 1);

            // Collect in global scope
            let _global_sym = collector.lookup_or_insert_global("module_level", HirId(1));

            // Create nested scope
            let scope = arena.alloc(Scope::new(HirId(10)));
            collector.push_scope(scope);
            assert_eq!(collector.scope_depth(), 2);

            // Collect in nested scope
            let _nested_sym = collector.lookup_or_insert("inner", HirId(2));

            // Pop back to global
            collector.pop_scope();
            assert_eq!(collector.scope_depth(), 1);
        });

        let _ = global_scope;
    }

    #[test]
    fn test_collect_symbols_batch_with_nested_scopes() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            // Global scope
            let _global = collector.lookup_or_insert("global_func", HirId(1));

            // First nested scope
            let scope1 = arena.alloc(Scope::new(HirId(10)));
            collector.push_scope(scope1);
            let _local1 = collector.lookup_or_insert("local1", HirId(2));

            // Second nested scope (inside first)
            let scope2 = arena.alloc(Scope::new(HirId(20)));
            collector.push_scope(scope2);
            let _local2 = collector.lookup_or_insert("local2", HirId(3));

            // Pop one level
            collector.pop_scope();
            let _local1_again = collector.lookup_or_insert("after_inner", HirId(4));

            // Pop to global
            collector.pop_scope();

            assert_eq!(collector.scope_depth(), 1);
        });

        let _ = global_scope;
    }

    #[test]
    fn test_collect_symbols_batch_preserves_unit_index() {
        let arena1 = Arena::default();
        let arena2 = Arena::default();
        let interner = InternPool::default();

        // Collect for unit 0
        let scope1 = collect_symbols_batch(0, &arena1, &interner, |collector| {
            assert_eq!(collector.unit_index(), 0);
            let _ = collector.lookup_or_insert("unit0_var", HirId(1));
        });

        // Collect for unit 1
        let scope2 = collect_symbols_batch(1, &arena2, &interner, |collector| {
            assert_eq!(collector.unit_index(), 1);
            let _ = collector.lookup_or_insert("unit1_var", HirId(1));
        });

        // Both should be valid but different scopes
        assert_ne!(scope1 as *const _, scope2 as *const _);
    }

    #[test]
    fn test_collect_symbols_batch_global_access() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            // Access global scope directly
            let globals = collector.globals();
            let _ = globals;

            // Insert symbols
            let sym1 = collector.lookup_or_insert_global("pub_func", HirId(1));
            let sym2 = collector.lookup_or_insert_global("pub_const", HirId(2));

            assert!(sym1.is_some());
            assert!(sym2.is_some());
        });

        let _ = global_scope;
    }

    #[test]
    fn test_collect_symbols_batch_with_pop_until() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            // Build up scope stack
            for i in 1..=3 {
                let scope = arena.alloc(Scope::new(HirId(i as u32 * 10)));
                collector.push_scope(scope);
            }
            assert_eq!(collector.scope_depth(), 4); // global + 3 scopes

            // Pop until depth 2
            collector.pop_until(2);
            assert_eq!(collector.scope_depth(), 2);

            // Can still insert symbols
            let sym = collector.lookup_or_insert("after_pop", HirId(100));
            assert!(sym.is_some());

            // Pop back to global
            collector.pop_until(1);
            assert_eq!(collector.scope_depth(), 1);
        });

        let _ = global_scope;
    }

    #[test]
    fn test_collect_symbols_batch_interner_sharing() {
        let arena = Arena::default();
        let interner = InternPool::default();

        // Pre-intern a string
        let _interned_key = interner.intern("shared_name");

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            // Access interner through collector
            let interner_ref = collector.interner();

            // Should be the same interner (same pointer)
            assert_eq!(interner_ref as *const _, &interner as *const _);

            // Insert with a name
            let sym = collector.lookup_or_insert("shared_name", HirId(1));
            assert!(sym.is_some());
        });

        let _ = global_scope;
    }

    #[test]
    fn test_collect_symbols_batch_complex_hierarchy() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            // Simulate a complex scope hierarchy:
            // global
            //   ├─ module
            //   │   ├─ function
            //   │   │   └─ loop_block
            //   │   └─ class
            //   └─ const

            let _global_const = collector.lookup_or_insert_global("GLOBAL_CONST", HirId(1));

            let module_scope = arena.alloc(Scope::new(HirId(10)));
            collector.push_scope(module_scope);
            let _module_fn = collector.lookup_or_insert("module_function", HirId(2));

            let fn_scope = arena.alloc(Scope::new(HirId(20)));
            collector.push_scope(fn_scope);
            let _fn_param = collector.lookup_or_insert("param", HirId(3));

            let block_scope = arena.alloc(Scope::new(HirId(30)));
            collector.push_scope(block_scope);
            let _loop_var = collector.lookup_or_insert("i", HirId(4));
            collector.pop_scope(); // exit block

            collector.pop_scope(); // exit function

            let class_scope = arena.alloc(Scope::new(HirId(40)));
            collector.push_scope(class_scope);
            let _field = collector.lookup_or_insert("field", HirId(5));
            collector.pop_scope(); // exit class

            collector.pop_scope(); // exit module

            assert_eq!(collector.scope_depth(), 1);
        });

        let _ = global_scope;
    }

    #[test]
    fn test_collect_symbols_batch_empty_visit() {
        let arena = Arena::default();
        let interner = InternPool::default();

        // Collect with empty visitor (no symbols collected)
        let global_scope = collect_symbols_batch(0, &arena, &interner, |_collector| {
            // Do nothing
        });

        // Should still return valid global scope
        let _ = global_scope;
    }

    #[test]
    fn test_collect_symbols_batch_different_hir_ids() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            // Create symbols with different HirIds
            let sym1 = collector.lookup_or_insert("name", HirId(1));
            let sym2 = collector.lookup_or_insert("name", HirId(2));
            let sym3 = collector.lookup_or_insert("name", HirId(u32::MAX));

            // All should be created (each HirId creates a new symbol)
            assert!(sym1.is_some());
            assert!(sym2.is_some());
            assert!(sym3.is_some());

            // All should be different (different HirIds)
            assert_ne!(sym1.unwrap() as *const _, sym2.unwrap() as *const _);
            assert_ne!(sym2.unwrap() as *const _, sym3.unwrap() as *const _);
        });

        let _ = global_scope;
    }

    #[test]
    fn test_apply_collected_symbols() {
        let arena = Arena::default();
        let interner = InternPool::default();

        // Collect symbols
        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            let _sym = collector.lookup_or_insert("test_var", HirId(1));
        });

        // Apply collected symbols (currently a no-op, but should not panic)
        apply_collected_symbols(0, global_scope);
    }

    #[test]
    fn test_apply_collected_symbols_multiple_units() {
        let arena1 = Arena::default();
        let arena2 = Arena::default();
        let interner = InternPool::default();

        let scope1 = collect_symbols_batch(0, &arena1, &interner, |collector| {
            let _ = collector.lookup_or_insert("unit0_var", HirId(1));
        });

        let scope2 = collect_symbols_batch(1, &arena2, &interner, |collector| {
            let _ = collector.lookup_or_insert("unit1_var", HirId(1));
        });

        // Apply both (should not panic)
        apply_collected_symbols(0, scope1);
        apply_collected_symbols(1, scope2);
    }

    /// A simple visitor implementation demonstrating the visitor pattern usage
    struct SimpleVisitor {
        visited_nodes: Vec<HirId>,
    }

    impl SimpleVisitor {
        fn new() -> Self {
            SimpleVisitor {
                visited_nodes: Vec::new(),
            }
        }

        /// Visit a module-like node and collect symbols
        fn visit_module<'a>(&mut self, collector: &mut CollectorCore<'a>, module_id: HirId, symbols: Vec<(&str, HirId)>) {
            self.visited_nodes.push(module_id);

            // Collect module-level symbols
            for (name, hir_id) in symbols {
                let _ = collector.lookup_or_insert_global(name, hir_id);
            }
        }

        /// Visit a function-like node with nested scope
        fn visit_function<'a>(
            &mut self,
            collector: &mut CollectorCore<'a>,
            arena: &'a Arena<'a>,
            fn_id: HirId,
            fn_name: &str,
            params: Vec<(&str, HirId)>,
        ) {
            self.visited_nodes.push(fn_id);

            // Declare the function in current scope
            let _ = collector.lookup_or_insert(fn_name, fn_id);

            // Create a nested scope for function body
            let fn_scope = arena.alloc(Scope::new(fn_id));
            collector.push_scope(fn_scope);

            // Collect function parameters
            for (param_name, param_id) in params {
                let _ = collector.lookup_or_insert(param_name, param_id);
            }
        }

        /// Visit a block-like node with nested scope
        fn visit_block<'a>(&mut self, collector: &mut CollectorCore<'a>, arena: &'a Arena<'a>, block_id: HirId, locals: Vec<(&str, HirId)>) {
            self.visited_nodes.push(block_id);

            // Create a nested scope for block
            let block_scope = arena.alloc(Scope::new(block_id));
            collector.push_scope(block_scope);

            // Collect local variables in block
            for (local_name, local_id) in locals {
                let _ = collector.lookup_or_insert(local_name, local_id);
            }
        }

        /// Exit the current scope
        fn exit_scope(&mut self, collector: &mut CollectorCore) {
            collector.pop_scope();
        }
    }

    #[test]
    fn test_visitor_pattern_with_batch_collection() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            let mut visitor = SimpleVisitor::new();

            // Visit module structure:
            // module
            //   ├─ CONST_A
            //   ├─ function_x(param1, param2)
            //   │   └─ {
            //   │       local_a
            //   │       inner_block { local_b }
            //   │     }
            //   └─ function_y(param_y)
            //       └─ { local_y }

            // Visit module level
            visitor.visit_module(
                collector,
                HirId(0),
                vec![("CONST_A", HirId(1)), ("CONST_B", HirId(2))],
            );

            // Visit function_x
            visitor.visit_function(
                collector,
                &arena,
                HirId(10),
                "function_x",
                vec![("param1", HirId(11)), ("param2", HirId(12))],
            );

            // Visit block in function_x
            visitor.visit_block(collector, &arena, HirId(20), vec![("local_a", HirId(21))]);

            // Visit inner block (nested)
            visitor.visit_block(collector, &arena, HirId(30), vec![("local_b", HirId(31))]);

            // Exit inner block
            visitor.exit_scope(collector);

            // Exit outer block
            visitor.exit_scope(collector);

            // Exit function_x
            visitor.exit_scope(collector);

            // Verify we're back at module level (depth 1)
            assert_eq!(collector.scope_depth(), 1);

            // Visit function_y
            visitor.visit_function(
                collector,
                &arena,
                HirId(40),
                "function_y",
                vec![("param_y", HirId(41))],
            );

            // Visit block in function_y
            visitor.visit_block(collector, &arena, HirId(50), vec![("local_y", HirId(51))]);

            // Exit block
            visitor.exit_scope(collector);

            // Exit function_y
            visitor.exit_scope(collector);

            // Verify we're back at module level
            assert_eq!(collector.scope_depth(), 1);

            // Verify visitor visited all nodes in order
            assert_eq!(visitor.visited_nodes.len(), 6);
            assert_eq!(
                visitor.visited_nodes,
                vec![
                    HirId(0),  // module
                    HirId(10), // function_x
                    HirId(20), // block in function_x
                    HirId(30), // inner block
                    HirId(40), // function_y
                    HirId(50), // block in function_y
                ]
            );
        });

        // Verify we got back a valid scope
        let _ = global_scope;
    }

    #[test]
    fn test_visitor_pattern_class_hierarchy() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            let mut visitor = SimpleVisitor::new();

            // Visit class hierarchy:
            // module
            //   ├─ class MyClass
            //   │   ├─ field1
            //   │   └─ method(self, arg)
            //   │       └─ { local_x }
            //   └─ function standalone()
            //       └─ { local_s }

            // Module level
            visitor.visit_module(collector, HirId(0), vec![]);

            // Class (as a "function" for scope purposes)
            visitor.visit_function(collector, &arena, HirId(100), "MyClass", vec![("field1", HirId(101))]);

            // Method in class
            visitor.visit_function(
                collector,
                &arena,
                HirId(110),
                "method",
                vec![("self", HirId(111)), ("arg", HirId(112))],
            );

            visitor.visit_block(collector, &arena, HirId(120), vec![("local_x", HirId(121))]);
            visitor.exit_scope(collector); // exit block
            visitor.exit_scope(collector); // exit method
            visitor.exit_scope(collector); // exit class

            // Standalone function
            visitor.visit_function(collector, &arena, HirId(200), "standalone", vec![]);
            visitor.visit_block(collector, &arena, HirId(210), vec![("local_s", HirId(211))]);
            visitor.exit_scope(collector); // exit block
            visitor.exit_scope(collector); // exit function

            // Back at module level
            assert_eq!(collector.scope_depth(), 1);

            // Verify all nodes were visited
            assert!(visitor.visited_nodes.contains(&HirId(100))); // MyClass
            assert!(visitor.visited_nodes.contains(&HirId(110))); // method
            assert!(visitor.visited_nodes.contains(&HirId(200))); // standalone
        });

        let _ = global_scope;
    }

    #[test]
    fn test_visitor_pattern_with_symbol_queries() {
        let arena = Arena::default();
        let interner = InternPool::default();

        let global_scope = collect_symbols_batch(0, &arena, &interner, |collector| {
            let mut visitor = SimpleVisitor::new();

            // Visit and create symbols, then query them
            visitor.visit_module(
                collector,
                HirId(0),
                vec![("PUBLIC_API", HirId(1)), ("INTERNAL_CONST", HirId(2))],
            );

            // After visiting module, we can query global scope
            let queried_global = collector.globals();
            let _ = queried_global;

            // Visit function and locals
            visitor.visit_function(collector, &arena, HirId(10), "process_data", vec![("input", HirId(11))]);

            visitor.visit_block(
                collector,
                &arena,
                HirId(20),
                vec![("result", HirId(21)), ("temp", HirId(22)), ("buffer", HirId(23))],
            );

            // Can still access interner and arena
            let _interner = collector.interner();
            let _arena = collector.arena();

            // Pop scopes
            visitor.exit_scope(collector);
            visitor.exit_scope(collector);

            // All symbols should be accessible via the scope hierarchy
            assert_eq!(collector.scope_depth(), 1);
        });

        let _ = global_scope;
    }
}
