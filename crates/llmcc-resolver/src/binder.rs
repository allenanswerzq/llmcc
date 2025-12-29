use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternPool;
use llmcc_core::ir::HirScope;
use llmcc_core::scope::{LookupOptions, Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, SymKindSet, Symbol};
use llmcc_core::{CompileCtxt, LanguageTraitImpl};

use rayon::prelude::*;

use crate::ResolverOption;

#[derive(Debug)]
pub struct BinderScopes<'a> {
    unit: CompileUnit<'a>,
    scopes: ScopeStack<'a>,
}

impl<'a> BinderScopes<'a> {
    pub fn new(unit: CompileUnit<'a>, globals: &'a Scope<'a>) -> Self {
        let scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner);
        scopes.push(globals);

        Self { unit, scopes }
    }

    #[inline]
    pub fn top(&self) -> &'a Scope<'a> {
        self.scopes.top().unwrap()
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
        tracing::trace!("push_scope: {:?}", id);
        let scope = self.unit.get_scope(id);
        self.scopes.push(scope);
    }

    /// Pushes a scope recursively with all its parent scopes.
    pub fn push_scope_recursive(&mut self, id: ScopeId) {
        tracing::trace!("push_scope_recursive: {:?}", id);
        let scope = self.unit.get_scope(id);
        self.scopes.push_recursive(scope);
    }

    /// Pushes the scope represented by a HirScope node.
    pub fn push_scope_node(&mut self, sn: &'a HirScope<'a>) {
        if sn.opt_ident().is_some() {
            self.push_scope_recursive(sn.scope().id());
        } else {
            self.push_scope(sn.scope().id());
        }
    }

    /// Pops the current scope from the stack.
    #[inline]
    pub fn pop_scope(&mut self) {
        tracing::trace!("pop_scope: depth {}", self.scopes.depth());
        self.scopes.pop();
    }

    /// Pops scopes until reaching the specified depth.
    #[inline]
    pub fn pop_until(&mut self, depth: usize) {
        tracing::trace!("pop_until: {} -> {}", self.scopes.depth(), depth);
        self.scopes.pop_until(depth);
    }

    /// Gets the global scope (always at index 0).
    #[inline]
    pub fn globals(&self) -> &'a Scope<'a> {
        self.scopes.globals()
    }

    #[inline]
    pub fn lookup_globals(
        &self,
        name: &str,
        kind_filters: SymKindSet,
    ) -> Option<Vec<&'a Symbol>> {
        tracing::trace!(
            "lookup globals '{}' with filters {:?}",
            name,
            kind_filters
        );
        let options = LookupOptions::current().with_kind_set(kind_filters);
        let name_key = self.unit.cc.interner.intern(name);
        self.scopes.globals().lookup_symbols(name_key, options)
    }

    #[inline]
    pub fn lookup_global(&self, name: &str, kind_filters: SymKindSet) -> Option<&'a Symbol> {
        let symbols = self.lookup_globals(name, kind_filters)?;
        if symbols.len() > 1 {
            tracing::warn!(
                "multiple global symbols found for '{}', returning the last one",
                name
            );
        }
        symbols.last().copied()
    }

    /// Lookup symbols by name with options
    #[inline]
    pub fn lookup_symbols(
        &self,
        name: &str,
        kind_filters: SymKindSet,
    ) -> Option<Vec<&'a Symbol>> {
        tracing::trace!(
            "lookup symbols '{}' with filters {:?}",
            name,
            kind_filters
        );
        let options = LookupOptions::current().with_kind_set(kind_filters);
        self.scopes.lookup_symbols(name, options)
    }

    #[inline]
    pub fn lookup_symbol(&self, name: &str, kind_filters: SymKindSet) -> Option<&'a Symbol> {
        let symbols = self.lookup_symbols(name, kind_filters)?;
        let current_unit = self.unit.index;
        tracing::trace!(
            "lookup_symbol '{}' found {} symbols (current_unit={}): {:?}",
            name,
            symbols.len(),
            current_unit,
            symbols
                .iter()
                .map(|s| (s.id(), s.unit_index()))
                .collect::<Vec<_>>()
        );
        if symbols.len() > 1 {
            // Prefer symbols from the current unit to avoid cross-crate false matches
            if let Some(local_sym) = symbols
                .iter()
                .find(|s| s.unit_index() == Some(current_unit))
            {
                return Some(*local_sym);
            }
            tracing::warn!(
                "multiple symbols found for '{}', returning the last one",
                name
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
            tracing::trace!(
                "looking up member '{}' in type scope with filters {:?}",
                member_name,
                kind_filters
            );
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
                    tracing::trace!("  filter: {:?}", kind);
                    let sym = self.lookup_member_symbol(obj_type_symbol, member_name, Some(kind));
                    if sym.is_some() {
                        return sym;
                    }
                }
            }
        } else {
            tracing::trace!("looking up member '{}' in type scope", member_name);
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
        tracing::trace!(
            "lookup_member_symbol: '{}' in {:?} (kind={:?})",
            member_name,
            obj_type_symbol.name,
            obj_type_symbol.kind()
        );

        // For TypeAlias (like Self), follow type_of to get the actual type
        let effective_symbol = if obj_type_symbol.kind() == SymKind::TypeAlias {
            if let Some(type_of_id) = obj_type_symbol.type_of() {
                let resolved = self
                    .unit
                    .cc
                    .opt_get_symbol(type_of_id)
                    .unwrap_or(obj_type_symbol);
                tracing::trace!(
                    "  -> followed type_of to {:?} (kind={:?})",
                    resolved.name,
                    resolved.kind()
                );
                resolved
            } else {
                obj_type_symbol
            }
        } else {
            obj_type_symbol
        };

        let scope_id = effective_symbol.opt_scope()?;
        let scope = self.unit.get_scope(scope_id);

        // Create isolated scope stack for member lookup to avoid falling back to lexical scopes
        let scopes = ScopeStack::new(&self.unit.cc.arena, &self.unit.cc.interner);
        scopes.push_recursive(scope);

        let options = if let Some(filter) = kind_filter {
            LookupOptions::current().with_kind_set(SymKindSet::from_kind(filter))
        } else {
            LookupOptions::current()
        };

        scopes
            .lookup_symbols(member_name, options)?
            .into_iter()
            .last()
    }

    /// Look up a qualified path (e.g., foo::Bar::baz) with optional kind filters.
    pub fn lookup_qualified(
        &self,
        qualified_name: &[&str],
        kind_filters: SymKindSet,
    ) -> Option<Vec<&'a Symbol>> {
        tracing::trace!(
            "lookup qualified {:?} with kind_filters {:?}",
            qualified_name,
            kind_filters
        );
        let mut options = LookupOptions::default().with_shift_start(true);
        if !kind_filters.is_empty() {
            options = options.with_kind_set(kind_filters)
        }
        let symbols = self.scopes.lookup_qualified(qualified_name, options)?;
        Some(symbols)
    }
}

/// Bind symbols from all compilation units, optionally in parallel.
///
/// The binding phase resolves all symbol references and establishes relationships between symbols
/// across compilation units. This happens after collection when all symbols have been discovered.
pub fn bind_symbols_with<'a, L: LanguageTraitImpl>(
    cc: &'a CompileCtxt<'a>,
    globals: &'a Scope<'a>,
    config: &ResolverOption,
) {
    let total_start = Instant::now();

    tracing::info!("starting symbol binding for total {} units", cc.files.len());

    // Atomic counter for parallel CPU time
    let bind_cpu_time_ns = AtomicU64::new(0);

    let bind_unit = |unit_index: usize| {
        let bind_start = Instant::now();

        tracing::debug!("binding symbols for unit {}", unit_index);
        let unit = cc.compile_unit(unit_index);
        let id = unit.file_root_id().unwrap();
        let node = unit.hir_node(id);
        L::bind_symbols(unit, node, globals, config);

        bind_cpu_time_ns.fetch_add(bind_start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    };

    let parallel_start = Instant::now();
    if config.sequential {
        tracing::debug!("running symbol binding sequentially");
        (0..cc.files.len()).for_each(bind_unit);
    } else {
        tracing::debug!("running symbol binding in parallel");
        (0..cc.files.len()).into_par_iter().for_each(bind_unit);
    }
    let parallel_time = parallel_start.elapsed();

    let total_time = total_start.elapsed();
    let bind_cpu_ms = bind_cpu_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;

    tracing::info!(
        "binding breakdown: parallel={:.2}ms (bind_cpu={:.2}ms), total={:.2}ms",
        parallel_time.as_secs_f64() * 1000.0,
        bind_cpu_ms,
        total_time.as_secs_f64() * 1000.0,
    );

    tracing::info!("symbol binding complete");
}
