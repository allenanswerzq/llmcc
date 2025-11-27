//! Symbol collection for parallel per-unit symbol table building.
use llmcc_core::LanguageTrait;
use llmcc_core::context::CompileCtxt;
use llmcc_core::interner::InternPool;
use llmcc_core::interner::InternedStr;
use llmcc_core::ir::{Arena, HirNode};
use llmcc_core::scope::{LookupOptions, Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};

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
        cc: &'a CompileCtxt<'a>,
        unit_index: usize,
        scopes: ScopeStack<'a>,
        globals: &'a Scope<'a>,
    ) -> Self {
        scopes.push(globals);
        Self {
            arena: &cc.arena,
            unit_index,
            interner: &cc.interner,
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
    pub fn build_fqn(&self, name: &str) -> InternedStr {
        for scope in self.scopes.iter().into_iter().rev() {
            if let Some(ns_sym) = scope.opt_symbol() {
                let ns_fqn = ns_sym.fqn();
                if let Some(ns_fqn_str) = self.interner.resolve_owned(ns_fqn) {
                    let fqn_str = format!("{}::{}", ns_fqn_str, name);
                    return self.interner.intern(&fqn_str);
                }
            }
        }
        self.interner.intern(name)
    }

    /// Initialize a symbol with common properties
    fn init_symbol(&self, symbol: &'a Symbol, name: &str, node: &HirNode<'a>, kind: SymKind) {
        if symbol.kind() == SymKind::Unknown {
            symbol.set_owner(node.id());
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
        let symbol = self
            .scopes
            .lookup_or_insert(name, node.id(), LookupOptions::current())?;
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
        let symbol = self
            .scopes
            .lookup_or_insert(name, node.id(), LookupOptions::chained())?;
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
        let symbol =
            self.scopes
                .lookup_or_insert_parent(name, node.id(), LookupOptions::current())?;
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
        let symbol =
            self.scopes
                .lookup_or_insert_global(name, node.id(), LookupOptions::current())?;
        self.init_symbol(symbol, name, node, kind);
        symbol.set_is_global(true);
        Some(symbol)
    }

    pub fn lookup_symbol_with(
        &self,
        name_sym: &'a Symbol,
        kind_filters: Vec<SymKind>,
    ) -> Option<&'a Symbol> {
        let option = LookupOptions::current().with_kind_filters(kind_filters);
        self.scopes.lookup_symbol(name_sym, option)
    }
}

/// Collect symbols from a compilation unit by invoking visitor on CollectorScopes
///
/// At the collect pass, we can only know all the sutff in a single compilation unit, because of the
/// random order of collecting, for symbols we can not resolve at the unit, we just create a symbol
/// placeholder, and resolve them in the later binding phase.
#[rustfmt::skip]
pub fn collect_symbols_with<'a, L: LanguageTrait>(
    cc: &'a CompileCtxt<'a>,
    config: &ResolverOption,
) -> &'a Scope<'a> {
    use std::time::Instant;
    use std::sync::atomic::{AtomicU64, Ordering};

    let total_start = Instant::now();
    let scope_stack = L::collect_init(cc);
    let visit_time_ns = AtomicU64::new(0);

    let collect_unit = |i: usize| {
        let unit_scope_stack = scope_stack.clone();
        let unit = cc.compile_unit(i);
        let node = unit.hir_node(unit.file_root_id().unwrap());

        let visit_start = Instant::now();
        L::collect_symbols(unit, node, unit_scope_stack, config);
        visit_time_ns.fetch_add(visit_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        if config.print_ir {
            use llmcc_core::printer::print_llmcc_ir;
            println!("=== IR for unit {} ===", i);
            let _ = print_llmcc_ir(unit);
        }
    };

    let parallel_start = Instant::now();
    if config.sequential {
        (0..cc.files.len()).for_each(collect_unit);
    } else {
        (0..cc.files.len())
            .into_par_iter()
            .for_each(collect_unit);
    };
    let parallel_elapsed = parallel_start.elapsed();

    let maps_start = Instant::now();
    cc.build_lookup_maps_from_arena();
    let maps_elapsed = maps_start.elapsed();

    let total_elapsed = total_start.elapsed();
    let visit_total_ms = visit_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;

    tracing::info!(
        "Symbol collection breakdown: total={:.2}s, parallel_wall={:.2}s, visitor_cpu={:.2}s, build_maps={:.2}s",
        total_elapsed.as_secs_f64(),
        parallel_elapsed.as_secs_f64(),
        visit_total_ms / 1000.0,
        maps_elapsed.as_secs_f64()
    );

    scope_stack.first()
}
