use llmcc_core::HirId;
use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternPool;
use llmcc_core::ir::HirNode;
use llmcc_core::scope::{LookupOptions, Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationDirection {
    Forward,
    Backward,
}

/// Binder for resolving symbols and managing symbol relationships.
///
/// The BinderScopes uses a hashmap-based lookup strategy:
/// 1. First attempts to find scopes/symbols in CompileCtxt's hashmap storage
/// 2. If not found, allocates new ones in the CompileUnit's arena
/// 3. Maintains a scope stack for hierarchical traversal
///
/// This is different from CollectorScopes which always uses the per-unit arena.
#[derive(Debug)]
pub struct BinderScopes<'tcx> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    relation_direction: RelationDirection,
}

impl<'tcx> BinderScopes<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner);
        scopes.push(globals);

        Self {
            unit,
            scopes,
            relation_direction: RelationDirection::Forward,
        }
    }

    #[inline]
    pub fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    #[inline]
    pub fn interner(&self) -> &InternPool {
        self.unit.interner()
    }

    #[inline]
    pub fn set_forward_relation(&mut self) {
        self.relation_direction = RelationDirection::Forward;
    }

    #[inline]
    pub fn set_backward_relation(&mut self) {
        self.relation_direction = RelationDirection::Backward;
    }

    #[inline]
    pub fn scopes(&self) -> &ScopeStack<'tcx> {
        &self.scopes
    }

    #[inline]
    pub fn scopes_mut(&mut self) -> &mut ScopeStack<'tcx> {
        &mut self.scopes
    }

    #[inline]
    pub fn top_symbol(&self) -> Option<&'tcx Symbol> {
        // Get the current (top) scope and its associated symbol
        self.scopes.top().and_then(|scope| scope.symbol())
    }

    /// Gets the current depth of the scope stack.
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
    /// Panics if stack is empty (should never happen since we always have global scope).
    #[inline]
    pub fn top_scope(&self) -> &'tcx Scope<'tcx> {
        self.scopes
            .top()
            .expect("scope stack should never be empty")
    }

    #[inline]
    pub fn get_scope(&self, owner: HirId) -> &'tcx Scope<'tcx> {
        self.unit
            .opt_get_scope(owner)
            .expect("scope must exist in CompileUnit")
    }

    /// Pushes a new scope created from a symbol onto the stack.
    ///
    /// Allocates a new scope in the CompileUnit's arena, associates it with the symbol,
    /// and pushes it onto the scope stack. This establishes the parent-child relationship
    /// between the new scope and the current scope.
    ///
    /// # Arguments
    /// * `id` - The HIR node ID for the scope
    /// * `symbol` - The symbol this scope belongs to (e.g., function, struct, trait)
    pub fn push_scope(&mut self, owner: HirId) {
        // NOTE: this is the biggest difference from CollectorScopes, we would expect
        // the scope must already exist in the CompileUnit
        let scope = self.get_scope(owner);
        self.scopes.push(scope);
    }

    pub fn push_scope_recursive(&mut self, owner: HirId) {
        // NOTE: this is the biggest difference from CollectorScopes, we would expect
        // the scope must already exist in the CompileUnit
        let scope = self
            .unit
            .opt_get_scope(owner)
            .expect("scope must exist in CompileUnit");
        self.scopes.push_recursive(scope);
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

    /// Gets the global scope.
    ///
    #[inline]
    pub fn globals(&self) -> &'tcx Scope<'tcx> {
        self.scopes
            .iter()
            .next()
            .expect("global scope should always be present")
    }

    /// Find or insert symbol in the current scope.
    ///
    /// Uses hashmap-based lookup first, then arena allocation if needed.
    /// Sets the symbol kind and unit index.
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
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let symbol = self.scopes.lookup_or_insert(name, node.id())?;
        symbol.set_kind(kind);
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
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let symbol = self.scopes.lookup_or_insert_chained(name, node.id())?;
        symbol.set_kind(kind);
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
    pub fn lookup_or_insert_parent(
        &self,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let symbol = self.scopes.lookup_or_insert_parent(name, node.id())?;
        symbol.set_kind(kind);
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
    pub fn lookup_or_insert_global(
        &self,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let symbol = self.scopes.lookup_or_insert_global(name, node.id())?;
        symbol.set_kind(kind);
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
    /// let symbol = binder.lookup_or_insert_with(None, node_id, SymKind::Function, opts)?;
    /// ```
    pub fn lookup_or_insert_with(
        &self,
        name: Option<&str>,
        node: &HirNode<'tcx>,
        kind: SymKind,
        options: LookupOptions,
    ) -> Option<&'tcx Symbol> {
        let symbol = self
            .scopes
            .lookup_or_insert_with(name, node.id(), options)?;
        symbol.set_kind(kind);
        Some(symbol)
    }
}

/// Public API for binding symbols with a custom visitor function.
///
/// This is a higher-order function that creates a BinderScopes and executes a closure
/// to perform binding operations. It provides a convenient way to perform symbol binding
/// while automatically managing the BinderScopes lifecycle.
///
/// # Two-Phase Symbol Resolution
///
/// This function is part of the second phase (binding) of symbol resolution:
/// - **Phase 1 (Collection)**: DeclVisitor + CollectorScopes create all symbols and scopes
/// - **Phase 2 (Binding)**: BinderVisitor + BinderScopes resolve and establish relationships
///
/// # Arguments
///
/// - `cc`: The compilation unit containing pre-created symbols from the collection phase
/// - `globals`: The global scope (root of the scope hierarchy)
/// - `visitor`: A closure that receives a mutable BinderScopes to perform binding operations
///
/// # Returns
///
/// Returns the global scope after binding is complete. This allows for chaining
/// and further processing if needed.
///
/// # Example
///
/// ```ignore
/// let globals = cc.create_globals();
/// let result = bind_symbols_with(unit, globals, |binder| {
///     // Perform custom binding operations
///     let sym = binder.lookup_or_insert("my_var", node, SymKind::Variable);
///     // ... more binding logic
/// });
/// ```
///
/// # Strategy
///
/// The BinderScopes uses a **hashmap-first lookup strategy**:
/// 1. First attempts to find scopes/symbols in the CompileCtxt's hashmap storage
/// 2. If not found, allocates new ones in the CompileUnit's arena
/// 3. Maintains a scope stack for hierarchical scope traversal
///
/// This differs from CollectorScopes which always allocates directly in the per-unit arena.
pub fn bind_symbols_with<'a, F>(
    cc: CompileUnit<'a>,
    globals: &'a Scope<'a>,
    visitor: F,
) -> &'a Scope<'a>
where
    F: FnOnce(&mut BinderScopes<'a>),
{
    let mut collector = BinderScopes::new(cc, globals);
    visitor(&mut collector);
    collector.globals()
}

#[cfg(test)]
mod tests {
    use super::*;
    use llmcc_core::ir::Arena;

    #[test]
    fn test_binder_core_creation() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let cc = llmcc_core::context::CompileCtxt::new(&arena, &interner);
        let unit = CompileUnit::new(cc, None);

        let global_scope = arena.alloc(Scope::new(0));
        let mut binder = BinderScopes::new(unit, global_scope);

        assert_eq!(binder.scope_depth(), 1);
        assert!(binder.top_symbol().is_none());
    }

    #[test]
    fn test_lookup_or_insert_current_scope() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let cc = llmcc_core::context::CompileCtxt::new(&arena, &interner);
        let unit = CompileUnit::new(cc, None);

        let global_scope = arena.alloc(Scope::new(0));
        let binder = BinderScopes::new(unit, global_scope);

        // Create a dummy HirNode
        let hir_id = 1;
        let node = HirNode::from_raw_parts(hir_id, hir_id);

        // Lookup or insert a symbol
        let sym1 = binder
            .lookup_or_insert("my_function", node, SymKind::Function)
            .expect("symbol should be created");

        assert_eq!(sym1.kind(), SymKind::Function);

        // Lookup the same symbol should return the existing one
        let sym2 = binder
            .lookup_or_insert("my_function", node, SymKind::Function)
            .expect("symbol should exist");

        assert_eq!(sym1.id(), sym2.id(), "should return the same symbol");
    }

    #[test]
    fn test_lookup_or_insert_chained() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let cc = llmcc_core::context::CompileCtxt::new(&arena, &interner);
        let unit = CompileUnit::new(cc, None);

        let global_scope = arena.alloc(Scope::new(0));
        let binder = BinderScopes::new(unit, global_scope);

        let hir_id = 1;
        let node = HirNode::from_raw_parts(hir_id, hir_id);

        // First symbol
        let sym1 = binder
            .lookup_or_insert_chained("var", node, SymKind::Variable)
            .expect("first symbol created");

        // Chained symbol should create a new one that links to the previous
        let sym2 = binder
            .lookup_or_insert_chained("var", node, SymKind::Variable)
            .expect("second symbol created");

        assert_ne!(
            sym1.id(),
            sym2.id(),
            "chained lookup should create new symbols"
        );
        assert_eq!(
            sym2.previous(),
            Some(sym1.id()),
            "second symbol should chain to first"
        );
    }

    #[test]
    fn test_lookup_or_insert_global() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let cc = llmcc_core::context::CompileCtxt::new(&arena, &interner);
        let unit = CompileUnit::new(cc, None);

        let global_scope = arena.alloc(Scope::new(0));
        let mut binder = BinderScopes::new(unit, global_scope);

        let hir_id = 1;
        let node = HirNode::from_raw_parts(hir_id, hir_id);

        // Push a nested scope
        let nested_scope = arena.alloc(Scope::new(1));
        binder.push_scope(nested_scope);

        assert_eq!(binder.scope_depth(), 2);

        // Insert into global scope from nested scope
        let sym = binder
            .lookup_or_insert_global("global_const", node, SymKind::Const)
            .expect("symbol should be inserted in global scope");

        assert_eq!(sym.kind(), SymKind::Const);

        // Verify it's in the global scope
        let found = binder
            .globals()
            .lookup_symbols(interner.intern("global_const"));
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn test_scope_depth_tracking() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let cc = llmcc_core::context::CompileCtxt::new(&arena, &interner);
        let unit = CompileUnit::new(cc, None);

        let global_scope = arena.alloc(Scope::new(0));
        let mut binder = BinderScopes::new(unit, global_scope);

        assert_eq!(binder.scope_depth(), 1);

        let scope1 = arena.alloc(Scope::new(1));
        binder.push_scope(scope1);
        assert_eq!(binder.scope_depth(), 2);

        let scope2 = arena.alloc(Scope::new(2));
        binder.push_scope(scope2);
        assert_eq!(binder.scope_depth(), 3);

        binder.pop_scope();
        assert_eq!(binder.scope_depth(), 2);

        binder.pop_until(1);
        assert_eq!(binder.scope_depth(), 1);
    }

    #[test]
    fn test_relation_direction() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let cc = llmcc_core::context::CompileCtxt::new(&arena, &interner);
        let unit = CompileUnit::new(cc, None);

        let global_scope = arena.alloc(Scope::new(0));
        let mut binder = BinderScopes::new(unit, global_scope);

        binder.set_forward_relation();
        binder.set_backward_relation();
        // Just verify these methods exist and can be called
    }

    #[test]
    fn test_lookup_or_insert_parent() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let cc = llmcc_core::context::CompileCtxt::new(&arena, &interner);
        let unit = CompileUnit::new(cc, None);

        let global_scope = arena.alloc(Scope::new(0));
        let mut binder = BinderScopes::new(unit, global_scope);

        let hir_id = 1;
        let node = HirNode::from_raw_parts(hir_id, hir_id);

        // Push nested scope
        let nested = arena.alloc(Scope::new(1));
        binder.push_scope(nested);

        // Insert into parent scope
        let sym = binder
            .lookup_or_insert_parent("parent_var", node, SymKind::Variable)
            .expect("symbol should be inserted in parent");

        // Verify it's in the global (parent) scope, not the nested scope
        let in_global = binder
            .globals()
            .lookup_symbols(interner.intern("parent_var"));
        assert_eq!(in_global.len(), 1);

        // Verify it's NOT in the nested scope
        let in_nested = nested.lookup_symbols(interner.intern("parent_var"));
        assert_eq!(in_nested.len(), 0);
    }

    #[test]
    fn test_lookup_or_insert_with_options() {
        use llmcc_core::scope::LookupOptions;

        let arena = Arena::new();
        let interner = InternPool::new();
        let cc = llmcc_core::context::CompileCtxt::new(&arena, &interner);
        let unit = CompileUnit::new(cc, None);

        let global_scope = arena.alloc(Scope::new(0));
        let binder = BinderScopes::new(unit, global_scope);

        let hir_id = 1;
        let node = HirNode::from_raw_parts(hir_id, hir_id);

        // Use custom options to insert globally
        let opts = LookupOptions::global();
        let sym = binder
            .lookup_or_insert_with(Some("custom_sym"), node, SymKind::Function, opts)
            .expect("symbol should be created");

        assert_eq!(sym.kind(), SymKind::Function);
    }

    #[test]
    fn test_hashmap_lookup_strategy() {
        // This test verifies the hashmap-first lookup strategy
        let arena = Arena::new();
        let interner = InternPool::new();
        let cc = llmcc_core::context::CompileCtxt::new(&arena, &interner);
        let unit = CompileUnit::new(cc, None);

        let global_scope = arena.alloc(Scope::new(0));
        let binder = BinderScopes::new(unit, global_scope);

        let hir_id_1 = 1;
        let node_1 = HirNode::from_raw_parts(hir_id_1, hir_id_1);

        // First lookup/insert
        let sym1 = binder
            .lookup_or_insert("hashmap_test", node_1, SymKind::Struct)
            .expect("first symbol");

        let hir_id_2 = 2;
        let node_2 = HirNode::from_raw_parts(hir_id_2, hir_id_2);

        // Second lookup with different node ID should return the same symbol
        // because hashmap-based lookup finds it first
        let sym2 = binder
            .lookup_or_insert("hashmap_test", node_2, SymKind::Struct)
            .expect("second lookup");

        // Same symbol ID means we hit the hashmap and didn't allocate a new one
        assert_eq!(sym1.id(), sym2.id(), "should use hashmap-first strategy");
    }
}
