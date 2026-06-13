use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternPool;
use llmcc_core::ir::HirScope;
use llmcc_core::scope::{QualifiedLookup, Scope, ScopeStack, SymbolFilter};
use llmcc_core::symbol::{ScopeId, SymKind, SymKindSet, Symbol};
use llmcc_core::{CompileCtxt, Language, Result};

use rayon::prelude::*;

use crate::ResolveOptions;

#[derive(Debug)]
pub struct BinderScopes<'a> {
    unit: CompileUnit<'a>,
    scopes: ScopeStack<'a>,
}

impl<'a> BinderScopes<'a> {
    pub fn new(unit: CompileUnit<'a>, globals: &'a Scope<'a>) -> Self {
        let scopes = ScopeStack::new(unit.context().arena(), unit.context().interner());
        scopes.push(globals);

        Self { unit, scopes }
    }

    #[inline]
    pub fn top(&self) -> &'a Scope<'a> {
        self.scopes.try_current().unwrap()
    }

    #[inline]
    pub fn unit(&self) -> CompileUnit<'a> {
        self.unit
    }

    #[inline]
    pub fn interner(&self) -> &InternPool {
        self.unit.interner()
    }

    #[inline]
    pub fn scopes(&self) -> &ScopeStack<'a> {
        &self.scopes
    }

    #[inline]
    pub fn scopes_mut(&mut self) -> &mut ScopeStack<'a> {
        &mut self.scopes
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

    /// Pushes a scope onto the stack by looking it up from the compilation unit.
    pub fn push_scope(&mut self, id: ScopeId) {
        let scope = self.unit.scope(id);
        self.scopes.push(scope);
    }

    /// Pushes a scope recursively with all its parent scopes.
    pub fn push_scope_recursive(&mut self, id: ScopeId) {
        let scope = self.unit.scope(id);
        self.scopes.push_recursive(scope);
    }

    /// Pushes the scope represented by a HirScope node.
    /// Returns true if scope was pushed, false if the scope wasn't set (e.g., unparsed macro).
    pub fn push_scope_node(&mut self, sn: &'a HirScope<'a>) -> bool {
        let Some(scope) = sn.try_scope() else {
            return false;
        };
        if sn.try_ident().is_some() {
            self.push_scope_recursive(scope.id());
        } else {
            self.push_scope(scope.id());
        }
        true
    }

    /// Pops the current scope from the stack.
    #[inline]
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Pops scopes until reaching the specified depth.
    #[inline]
    pub fn pop_until(&mut self, depth: usize) {
        self.scopes.pop_until(depth);
    }

    /// Gets the global scope (always at index 0).
    #[inline]
    pub fn globals(&self) -> &'a Scope<'a> {
        self.scopes.globals()
    }

    #[inline]
    pub fn lookup_globals(&self, name: &str, kind_filters: SymKindSet) -> Option<Vec<&'a Symbol>> {
        let options = SymbolFilter::kinds(kind_filters);
        let name_key = self.unit.interner().intern(name);
        self.scopes.globals().try_lookup_symbols(name_key, options)
    }

    #[inline]
    pub fn lookup_global(&self, name: &str, kind_filters: SymKindSet) -> Option<&'a Symbol> {
        let symbols = self.lookup_globals(name, kind_filters)?;
        if symbols.len() > 1 {
            tracing::warn!(
                name,
                count = symbols.len(),
                "multiple global symbols found, returning the last one"
            );
        }
        symbols.last().copied()
    }

    /// Lookup symbols by name with options
    #[inline]
    pub fn lookup_symbols(&self, name: &str, kind_filters: SymKindSet) -> Option<Vec<&'a Symbol>> {
        let options = SymbolFilter::kinds(kind_filters);
        self.scopes.try_lookup_symbols(name, options)
    }

    #[inline]
    pub fn lookup_symbol(&self, name: &str, kind_filters: SymKindSet) -> Option<&'a Symbol> {
        let symbols = self.lookup_symbols(name, kind_filters)?;
        if symbols.len() > 1 {
            let current_unit = self.unit.index();
            let current_crate_index = self.unit.unit_meta().crate_index;

            // 1. Prefer symbols from the current unit (same file)
            if let Some(local_sym) = symbols
                .iter()
                .find(|s| s.unit_index() == Some(current_unit))
            {
                return Some(*local_sym);
            }

            // 2. Prefer symbols from the same crate over cross-crate symbols
            // Only filter if there are actually cross-crate symbols
            let same_crate_symbols: Vec<_> = symbols
                .iter()
                .filter(|s| s.crate_index() == Some(current_crate_index))
                .copied()
                .collect();

            if !same_crate_symbols.is_empty() && same_crate_symbols.len() < symbols.len() {
                // There are both same-crate and cross-crate symbols, prefer same-crate
                return same_crate_symbols.last().copied();
            }

            tracing::warn!(
                name,
                count = symbols.len(),
                "multiple symbols found, returning the last one"
            );
        }
        symbols.last().copied()
    }

    /// Look up a member symbol in a type's scope.
    pub fn lookup_member_symbols(
        &self,
        obj_type_symbol: &'a Symbol,
        member_name: &str,
        kind_filters: SymKindSet,
    ) -> Option<&'a Symbol> {
        if !kind_filters.is_empty() {
            // Try each kind individually for priority ordering
            for kind in [
                SymKind::Method,
                SymKind::Function,
                SymKind::Field,
                SymKind::Variable,
                SymKind::Const,
                SymKind::Static,
            ] {
                if kind_filters.contains(kind) {
                    let sym = self.lookup_member_symbol(obj_type_symbol, member_name, Some(kind));
                    if sym.is_some() {
                        return sym;
                    }
                }
            }
        }
        None
    }

    /// Look up a member symbol in a type's scope.
    pub fn lookup_member_symbol(
        &self,
        obj_type_symbol: &'a Symbol,
        member_name: &str,
        kind_filter: Option<SymKind>,
    ) -> Option<&'a Symbol> {
        // For TypeAlias (like Self), follow type_of to get the actual type
        let effective_symbol = if obj_type_symbol.kind() == SymKind::TypeAlias {
            if let Some(type_of_id) = obj_type_symbol.type_of() {
                self.unit.try_symbol(type_of_id).unwrap_or(obj_type_symbol)
            } else {
                obj_type_symbol
            }
        } else {
            obj_type_symbol
        };

        let scope_id = effective_symbol.try_owned_scope()?;
        let scope = self.unit.scope(scope_id);

        // Create isolated scope stack for member lookup to avoid falling back to lexical scopes
        let scopes = ScopeStack::new(self.unit.context().arena(), self.unit.context().interner());
        scopes.push_recursive(scope);

        let options = if let Some(filter) = kind_filter {
            SymbolFilter::kinds(SymKindSet::from_kind(filter))
        } else {
            SymbolFilter::any()
        };

        scopes
            .try_lookup_symbols(member_name, options)?
            .into_iter()
            .last()
    }

    /// Look up a qualified path (e.g., foo::Bar::baz) with optional kind filters.
    pub fn lookup_qualified(
        &self,
        qualified_name: &[&str],
        kind_filters: SymKindSet,
    ) -> Option<Vec<&'a Symbol>> {
        let mut options = QualifiedLookup::lexical();
        if !kind_filters.is_empty() {
            options = options.with_result_kinds(kind_filters)
        }
        let symbols = self.scopes.try_lookup_qualified(qualified_name, options)?;
        Some(symbols)
    }

    /// Look up a qualified path and apply same-crate preference for multi-crate scenarios.
    pub fn lookup_qualified_symbol(
        &self,
        qualified_name: &[&str],
        kind_filters: SymKindSet,
    ) -> Option<&'a Symbol> {
        let symbols = self.lookup_qualified(qualified_name, kind_filters)?;
        let current_unit = self.unit.index();

        if symbols.len() > 1 {
            // 1. Prefer symbols from the current unit (same file)
            if let Some(local_sym) = symbols
                .iter()
                .find(|s| s.unit_index() == Some(current_unit))
            {
                return Some(*local_sym);
            }

            // 2. Prefer symbols from the same crate using crate_index (O(1) check)
            let current_crate_index = self.unit.unit_meta().crate_index;
            if let Some(same_crate_sym) = symbols
                .iter()
                .find(|s| s.crate_index() == Some(current_crate_index))
            {
                return Some(*same_crate_sym);
            }

            tracing::warn!(
                path = ?qualified_name,
                count = symbols.len(),
                "multiple symbols found for qualified path, returning the last one"
            );
        }
        symbols.last().copied()
    }
}

/// Bind symbols from all compilation units, optionally in parallel.
///
/// The binding phase resolves all symbol references and establishes relationships between symbols
/// across compilation units. This happens after collection when all symbols have been discovered.
pub fn bind_symbols_with<'a, L: Language>(
    cc: &'a CompileCtxt<'a>,
    globals: &'a Scope<'a>,
    config: &ResolveOptions,
) -> Result<()> {
    let total_start = Instant::now();
    let unit_count = cc.unit_count();
    tracing::info!(unit_count, "starting symbol binding");

    let bind_cpu_time_ns = AtomicU64::new(0);

    let bind_unit = |unit_index: usize| -> Result<()> {
        let bind_start = Instant::now();
        let unit = cc.compile_unit(unit_index);
        let id = unit.file_root_id()?;
        let node = unit.hir_node(id);
        L::bind_symbols(unit, node, globals, config);
        bind_cpu_time_ns.fetch_add(bind_start.elapsed().as_nanos() as u64, Ordering::Relaxed);
        Ok(())
    };

    let parallel_start = Instant::now();
    let results = if config.sequential {
        (0..unit_count).map(bind_unit).collect::<Vec<_>>()
    } else {
        (0..unit_count)
            .into_par_iter()
            .map(bind_unit)
            .collect::<Vec<_>>()
    };
    results.into_iter().collect::<Result<Vec<_>>>()?;
    let parallel_ms = parallel_time_ms(parallel_start);
    let bind_cpu_ms = bind_cpu_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let total_ms = parallel_time_ms(total_start);

    tracing::info!(
        parallel_ms,
        bind_cpu_ms,
        total_ms,
        "symbol binding complete"
    );
    Ok(())
}

fn parallel_time_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
