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
use llmcc_core::context::CompileCtxt;
use llmcc_core::interner::InternPool;
use llmcc_core::ir::{Arena, HirId};
use llmcc_core::scope::{LookupOptions, Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymId, Symbol, SymbolKind};

/// Core symbol collector for a single compilation unit.
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
    pub fn new(
        unit_index: usize,
        arena: &'a Arena<'a>,
        interner: &'a InternPool,
        globals: &'a Scope<'a>,
    ) -> Self {
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

    /// Gets the current depth of the scope stack (number of nested scopes).
    ///
    /// - 0 means no scope has been pushed yet
    /// - 1 means global scope is active
    /// - 2+ means nested scopes are active
    #[inline]
    pub fn scope_depth(&self) -> usize {
        self.scopes.depth()
    }

    /// Gets the top (current) scope on the stack.
    ///
    /// Returns the most recently pushed scope.
    /// Panics if stack is empty (should never happen in normal use since we always have global scope).
    #[inline]
    pub fn top_scope(&self) -> &'a Scope<'a> {
        self.scopes
            .top()
            .expect("scope stack should never be empty")
    }

    /// Pushes a scope onto the stack.
    ///
    /// Increases nesting depth and makes the scope active for symbol insertions.
    #[inline]
    pub fn push_scope(&mut self, scope: &'a Scope<'a>) {
        self.scopes.push(scope);
    }

    /// Pushes a scope onto the stack.
    ///
    /// Increases nesting depth and makes the scope active for symbol insertions.
    #[inline]
    pub fn push_scope_recursively(&mut self, scope: &'a Scope<'a>) {
        self.scopes.push_recursively(scope);
    }

    /// Pushes a scope onto the stack.
    ///
    /// Increases nesting depth and makes the scope active for symbol insertions.
    #[inline]
    pub fn push_scope_with(&mut self, id: HirId, symbol: Option<&'a Symbol>) {
        let scope = self.arena.alloc(Scope::new_with(id, symbol));
        if let Some(symbol) = symbol {
            symbol.set_scope(Some(scope.id()));
            if let Some(parent_scope) = self.scopes.top() {
                symbol.set_parent_scope(Some(parent_scope.id()));
            }
        }
        self.push_scope(scope);
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
    /// * `kind` - The kind of symbol (function, struct, variable, etc.)
    ///
    /// # Returns
    /// Some(symbol) if name is non-empty, None if name is empty
    #[inline]
    pub fn lookup_or_insert(
        &self,
        name: &str,
        node: HirId,
        kind: SymbolKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert(name, node)?;
        symbol.set_kind(kind);
        symbol.set_unit_index(self.unit_index());
        Some(symbol)
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
    /// * `kind` - The kind of symbol (function, struct, variable, etc.)
    ///
    /// # Returns
    /// Some(symbol) if name is non-empty, None if name is empty
    #[inline]
    pub fn lookup_or_insert_chained(
        &self,
        name: &str,
        node: HirId,
        kind: SymbolKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_chained(name, node)?;
        symbol.set_kind(kind);
        symbol.set_unit_index(self.unit_index());
        Some(symbol)
    }

    /// Find or insert symbol in the parent scope.
    ///
    /// Inserts into the parent scope (depth-1) if it exists, otherwise fails.
    /// Useful for lifting definitions out of the current scope.
    ///
    /// # Arguments
    /// * `name` - The symbol name
    /// * `node` - The HIR node for the symbol
    /// * `kind` - The kind of symbol (function, struct, variable, etc.)
    ///
    /// # Returns
    /// Some(symbol) if name is non-empty and parent scope exists,
    /// None if name is empty or no parent scope available
    #[inline]
    pub fn lookup_or_insert_parent(
        &self,
        name: &str,
        node: HirId,
        kind: SymbolKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_parent(name, node)?;
        symbol.set_kind(kind);
        symbol.set_unit_index(self.unit_index());
        Some(symbol)
    }

    /// Find or insert symbol in the global scope.
    ///
    /// Inserts into the global scope (depth 0) regardless of current nesting.
    /// Used for module-level definitions.
    ///
    /// # Arguments
    /// * `name` - The symbol name
    /// * `node` - The HIR node for the symbol
    /// * `kind` - The kind of symbol (function, struct, variable, etc.)
    ///
    /// # Returns
    /// Some(symbol) if name is non-empty, None if name is empty
    #[inline]
    pub fn lookup_or_insert_global(
        &self,
        name: &str,
        node: HirId,
        kind: SymbolKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_global(name, node)?;
        symbol.set_kind(kind);
        symbol.set_unit_index(self.unit_index());
        Some(symbol)
    }

    /// Full control API for symbol lookup and insertion with custom options.
    ///
    /// Provides maximum flexibility for symbol resolution. All behavior is
    /// controlled via the `LookupOptions` parameter.
    ///
    /// # Arguments
    /// * `name` - The symbol name (None for anonymous if force=true)
    /// * `node` - The HIR node for the symbol
    /// * `kind` - The kind of symbol (function, struct, variable, etc.)
    /// * `options` - Lookup options controlling scope selection and behavior
    ///
    /// # Returns
    /// Some(symbol) if found/created, None if name is empty/null and force=false
    ///
    /// # Example
    /// ```ignore
    /// use llmcc_core::scope::LookupOptions;
    /// let opts = LookupOptions::global().with_force(true);
    /// let symbol = collector.lookup_or_insert_with(None, node_id, SymbolKind::Function, opts)?;
    /// ```
    #[inline]
    pub fn lookup_or_insert_with(
        &self,
        name: Option<&str>,
        node: HirId,
        kind: SymbolKind,
        options: llmcc_core::scope::LookupOptions,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_with(name, node, options)?;
        symbol.set_kind(kind);
        symbol.set_unit_index(self.unit_index());
        Some(symbol)
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
/// let collected_scope = collect_symbols_with(
///     0,
///     &arena,
///     &interner,
///     |collector| {
///         // Visit AST nodes and call collector methods
///         visitor.visit_module(&collector, module_node);
///     }
/// );
/// ```
pub fn collect_symbols_with<'a, F>(
    unit_index: usize,
    arena: &'a Arena<'a>,
    interner: &'a InternPool,
    globals: &'a Scope<'a>,
    visitor: F,
) -> &'a Scope<'a>
where
    F: FnOnce(&mut CollectorCore<'a>),
{
    let mut collector = CollectorCore::new(unit_index, arena, interner, globals);
    visitor(&mut collector);
    collector.globals()
}

/// Apply symbols collected from a single compilation unit to the global context.
///
/// NOTE: even arena is the per-file, but the sym_id and scope_id is actually
/// Apply collected symbols from a per-unit arena to the global compilation context.
///
/// This function transfers all scopes and symbols from a per-unit arena (where they were
/// allocated during collection) to the global compilation context. This is typically called
/// after symbol collection is complete for a single compilation unit.
///
/// # How It Works
/// 1. Creates/gets the global scope in the compilation context
/// 2. Iterates through all scopes in the per-unit arena
/// 3. For the global scope: merges it into the global context's global scope
/// 4. For all other scopes: allocates new scopes in the global arena while preserving IDs
/// 5. All symbols are cloned and registered in the global symbol map
///
/// # Key Insight
/// - Symbols and ScopeIds are assigned globally via atomics during collection (not per-unit)
/// - This ensures uniqueness across all compilation units
/// - When transferring to global arena, we preserve the IDs (don't create new ones)
/// - The per-unit scope links and relationships are preserved in the transfer
///
/// # Arguments
/// * `cc` - The global compilation context (for registering symbols)
/// * `arena` - The per-unit arena containing collected symbols and scopes
/// * `globals` - The global scope from the per-unit arena
///
/// # Returns
/// Reference to the final global scope (in the global compilation context)
///
/// # Example
/// ```ignore
/// let mut per_unit_arena = Arena::new();
/// let mut collector = CollectorCore::new(unit_index, &per_unit_arena, &interner);
/// // ... collect symbols ...
/// let globals = collector.globals();
/// let final_globals = apply_collected_symbols(&cc, &mut per_unit_arena, globals);
/// ```
pub fn apply_collected_symbols<'tcx, 'unit>(
    cc: &'tcx CompileCtxt<'tcx>,
    arena: &'unit Arena<'unit>,
    globals: &'unit Scope<'unit>,
) -> &'tcx Scope<'tcx> {
    // Create or get the global scope in the compilation context
    let final_globals = cc.create_globals();

    // Transfer all scopes from per-unit arena to global context
    for scope in arena.iter_scope() {
        if scope.id() == globals.id() {
            // For the global scope: merge into the final global scope
            // This combines all global-level symbols into one scope
            cc.merge_two_scopes(final_globals, scope);
        } else {
            // For all other scopes: allocate new instances in the global arena
            // while preserving their IDs and symbol relationships
            cc.alloc_scope_with(scope);
        }
    }

    final_globals
}
