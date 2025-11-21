//! Symbol collection for parallel per-unit symbol table building.
use llmcc_core::context::CompileCtxt;
use llmcc_core::interner::InternPool;
use llmcc_core::interner::InternedStr;
use llmcc_core::ir::{Arena, HirNode};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_core::{HirId, LanguageTrait};

use rayon::prelude::*;

use crate::ResolverOption;

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
        let scopes = ScopeStack::new(arena, interner);
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

    /// Get the scope stack for iteration
    #[inline]
    pub fn scopes(&self) -> &ScopeStack<'a> {
        &self.scopes
    }

    /// Get the current (top) scope on the stack
    #[inline]
    pub fn top(&self) -> Option<&'a Scope<'a>> {
        self.scopes.top()
    }

    /// Build fully qualified name from current scope
    fn build_fqn(&self, name: &str) -> InternedStr {
        let fqn_str = self
            .top()
            .and_then(|parent| parent.symbol())
            .and_then(|parent_sym| {
                // Read the InternedStr FQN of the scope's symbol
                let fqn = parent_sym.fqn.read();
                // Resolve the InternedStr to an owned String
                self.interner.resolve_owned(*fqn)
            })
            .map(|scope_fqn| {
                // If we have a scope FQN, format it as "scope::name"
                format!("{}::{}", scope_fqn, name)
            })
            .unwrap_or_else(|| {
                // If any step failed (no scope, no symbol, or no resolved scope FQN),
                // the FQN is just the name itself.
                name.to_string()
            });

        // Intern the final FQN string
        self.interner().intern(&fqn_str)
    }

    /// Initialize a symbol with common properties
    fn init_symbol(&self, symbol: &'a Symbol, name: &str, node: &HirNode<'a>, kind: SymKind) {
        if symbol.kind() == SymKind::Unknown {
            symbol.set_kind(kind);
            symbol.set_unit_index(self.unit_index());
            symbol.set_fqn(self.build_fqn(name));
            symbol.add_defining(node.id());
            if let Some(parent) = self.top() {
                symbol.set_parent_scope(parent.id());
            }
        }
    }

    /// Find or insert symbol for node in current scope, set kind and unit index
    #[inline]
    pub fn lookup_or_insert(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert(name, node)?;
        self.init_symbol(symbol, name, node, kind);
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
        self.init_symbol(symbol, name, node, kind);
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
        self.init_symbol(symbol, name, node, kind);
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
        self.init_symbol(symbol, name, node, kind);
        symbol.set_is_global(true);
        Some(symbol)
    }

    /// Find or insert symbol with custom lookup options
    #[inline]
    pub fn lookup_or_insert_with(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
        options: llmcc_core::scope::LookupOptions,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_with(name, node, options)?;
        self.init_symbol(symbol, name, node, kind);
        Some(symbol)
    }

    pub fn lookup_symbol_with(
        &self,
        name: &str,
        kind_filters: Option<Vec<SymKind>>,
        unit_filters: Option<Vec<usize>>,
        fqn_filters: Option<Vec<&str>>,
    ) -> Option<&'a Symbol> {
        self.scopes
            .lookup_symbol_with(name, kind_filters, unit_filters, fqn_filters)
    }
}

/// Apply symbols collected from a single compilation unit to the global context.
fn apply_collected_symbols<'tcx>(
    cc: &'tcx CompileCtxt<'tcx>,
    arena: &'tcx Arena<'tcx>,
    final_globals: &'tcx Scope<'tcx>,
    unit_globals: &'tcx Scope<'tcx>,
) -> &'tcx Scope<'tcx> {
    // Transfer all scopes from per-unit arena to global
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

/// Collect symbols from a compilation unit by invoking visitor on CollectorScopes
#[rustfmt::skip]
pub fn collect_symbols_with<'a, L: LanguageTrait>(
    cc: &'a CompileCtxt<'a>,
    config: &ResolverOption,
) -> &'a Scope<'a> {
    let arena = &cc.arena;
    let interner = &cc.interner;

    let collect_unit = |i: usize| {
        let unit = cc.compile_unit(i);
        let unit_globals = arena.alloc(Scope::new(HirId(i)));
        let node = unit.hir_node(unit.file_root_id().unwrap());

        let mut collector = CollectorScopes::new(i, arena, interner, unit_globals);
        L::collect_symbols(&unit, &node, &mut collector, unit_globals, config);

        if config.print_ir {
            use llmcc_core::printer::print_llmcc_ir;
            println!("=== IR for unit {} ===", i);
            let _ = print_llmcc_ir(unit);
        }

        unit_globals
    };

    let unit_globals_vec = if config.sequential {
        (0..cc.files.len()).map(collect_unit).collect::<Vec<_>>()
    } else {
        (0..cc.files.len())
            .into_par_iter()
            .map(collect_unit)
            .collect::<Vec<_>>()
    };

    let globals = cc.create_globals();
    for unit_globals in unit_globals_vec.iter() {
        apply_collected_symbols(cc, arena, globals, unit_globals);
    }
    globals
}
