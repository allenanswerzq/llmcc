//! Symbol collection for parallel per-unit symbol table building.
use llmcc_core::context::CompileCtxt;
use llmcc_core::interner::InternPool;
use llmcc_core::ir::{Arena, HirNode};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_core::{HirId, LanguageTrait};

use rayon::prelude::*;
/// Core symbol collector for a single compilation unit
pub struct CollectorScopes<'a> {
    arena: &'a Arena<'a>,
    unit_index: usize,
    interner: &'a InternPool,
    scopes: ScopeStack<'a>,
    globals: &'a Scope<'a>,
}

impl<'a> std::fmt::Debug for CollectorScopes<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scopes: Vec<&Scope> = self.arena.scope();

        f.debug_struct("CollectorScopes")
            .field("unit_index", &self.unit_index)
            .field("scope_depth", &self.scopes.depth())
            .field("num_scopes", &scopes.len())
            .field("scopes", &scopes)
            .finish()
    }
}

impl<'a> CollectorScopes<'a> {
    /// Create new collector with arena, interner, and global scope
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

    /// Get compilation unit index
    #[inline]
    pub fn unit_index(&self) -> usize {
        self.unit_index
    }

    /// Get the arena
    #[inline]
    pub fn arena(&self) -> &Arena<'a> {
        self.arena
    }

    /// Get current scope stack depth
    #[inline]
    pub fn scope_depth(&self) -> usize {
        self.scopes.depth()
    }

    /// Get current (top) scope from stack
    #[inline]
    pub fn top_scope(&self) -> &'a Scope<'a> {
        self.scopes
            .top()
            .expect("scope stack should never be empty")
    }

    /// Push scope onto stack
    #[inline]
    pub fn push_scope(&mut self, scope: &'a Scope<'a>) {
        self.scopes.push(scope);
    }

    /// Push scope recursively
    #[inline]
    pub fn push_scope_recursive(&mut self, scope: &'a Scope<'a>) {
        self.scopes.push_recursive(scope);
    }

    /// Push new scope with optional symbol, allocate and register it
    #[inline]
    pub fn push_scope_with(&mut self, node: &HirNode<'a>, symbol: Option<&'a Symbol>) {
        let scope = self.arena.alloc(Scope::new_with(node.id(), symbol));
        if let Some(symbol) = symbol {
            symbol.set_scope(scope.id());
            if let Some(parent_scope) = self.scopes.top() {
                symbol.set_parent_scope(parent_scope.id());
            }
        }
        self.push_scope(scope);
    }

    /// Pop current scope from stack
    #[inline]
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Pop scopes until reaching target depth
    pub fn pop_until(&mut self, depth: usize) {
        self.scopes.pop_until(depth);
    }

    /// Get shared string interner
    #[inline]
    pub fn interner(&self) -> &'a InternPool {
        self.interner
    }

    /// Get global (module-level) scope
    #[inline]
    pub fn globals(&self) -> &'a Scope<'a> {
        self.globals
    }

    /// Find or insert symbol in current scope, set kind and unit index
    #[inline]
    pub fn lookup_or_insert(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert(name, node)?;
        symbol.set_kind(kind);
        symbol.set_unit_index(self.unit_index());
        Some(symbol)
    }

    /// Find or insert symbol with chaining for shadowing support
    #[inline]
    pub fn lookup_or_insert_chained(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_chained(name, node)?;
        symbol.set_kind(kind);
        symbol.set_unit_index(self.unit_index());
        Some(symbol)
    }

    /// Find or insert symbol in parent scope
    #[inline]
    pub fn lookup_or_insert_parent(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_parent(name, node)?;
        symbol.set_kind(kind);
        symbol.set_unit_index(self.unit_index());
        Some(symbol)
    }

    /// Find or insert symbol in global scope
    #[inline]
    pub fn lookup_or_insert_global(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_global(name, node)?;
        symbol.set_kind(kind);
        symbol.set_unit_index(self.unit_index());
        Some(symbol)
    }

    /// Find or insert symbol with custom lookup options
    #[inline]
    pub fn lookup_or_insert_with(
        &self,
        name: Option<&str>,
        node: &HirNode<'a>,
        kind: SymKind,
        options: llmcc_core::scope::LookupOptions,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_with(name, node, options)?;
        symbol.set_kind(kind);
        symbol.set_unit_index(self.unit_index());
        Some(symbol)
    }
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
fn apply_collected_symbols<'tcx, 'unit>(
    cc: &'tcx CompileCtxt<'tcx>,
    arena: &'unit Arena<'unit>,
    final_globals: &'tcx Scope<'tcx>,
    unit_globals: &'unit Scope<'unit>,
) -> &'tcx Scope<'tcx> {
    // Transfer all scopes from per-unit arena to global context
    for scope in arena.scope() {
        if scope.id() == unit_globals.id() {
            // For the global scope: merge into the final global scope
            // This combines all global-level symbols into one scope
            cc.merge_two_scopes(final_globals, unit_globals);
        } else {
            // For all other scopes: allocate new instances in the global arena
            // while preserving their IDs and symbol relationships
            cc.alloc_scope_with(scope);
        }
    }

    final_globals
}

#[derive(Default)]
pub struct CollectorOption {
    pub print_ir: bool,
}

impl CollectorOption {
    pub fn with_print_ir(mut self, print_ir: bool) -> Self {
        self.print_ir = print_ir;
        self
    }
}

/// Collect symbols from a compilation unit by invoking visitor on CollectorScopes
pub fn collect_symbols_with<'a, L: LanguageTrait>(
    cc: &'a CompileCtxt<'a>,
    config: CollectorOption,
) -> &'a Scope<'a> {
    let interner = &cc.interner;
    let unit_globals_vec = (0..cc.files.len())
        .into_par_iter()
        .map(|unit_index| {
            // Create a per-unit arena with 'tcx lifetime
            let unit_arena = cc.create_unit_arena();
            let unit_globals = unit_arena.alloc(Scope::new(HirId(unit_index)));

            let unit = cc.compile_unit(unit_index);
            let id = unit.file_start_hir_id().unwrap();
            let node = unit.hir_node(id);
            let mut collector =
                CollectorScopes::new(unit_index, unit_arena, interner, unit_globals);
            L::collect_symbols(&unit, &node, &mut collector, unit_globals);

            if config.print_ir {
                use llmcc_core::printer::print_llmcc_ir;
                println!("=== IR for unit {} ===", unit_index);
                let _ = print_llmcc_ir(unit);
            }

            unit_globals
        })
        .collect::<Vec<&'a Scope<'a>>>();

    let globals = cc.create_globals();
    for (unit_index, unit_globals) in unit_globals_vec.iter().enumerate() {
        let unit_arena = cc.get_unit_arena(unit_index);
        apply_collected_symbols(cc, unit_arena, globals, unit_globals);
    }
    globals
}
