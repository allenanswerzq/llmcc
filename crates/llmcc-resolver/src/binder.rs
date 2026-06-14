use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirScope;
use llmcc_core::scope::{QualifiedLookup, Scope, ScopeStack, SymbolFilter};
use llmcc_core::symbol::{ScopeId, SymKind, SymKindSet, Symbol};
use llmcc_core::{CompileCtxt, Language, Result};

use rayon::prelude::*;

use crate::{ResolveOptions, elapsed_ms, try_resolve_ambiguous};

/// Binding context for one compilation unit.
///
/// The global scope stays at stack depth 1. Pop operations never remove it, so
/// malformed traversal cannot abort binding by emptying the stack.
#[derive(Debug)]
pub struct BindCtxt<'a> {
    unit: CompileUnit<'a>,
    scopes: ScopeStack<'a>,
    globals: &'a Scope<'a>,
}

impl<'a> BindCtxt<'a> {
    pub fn new(unit: CompileUnit<'a>, globals: &'a Scope<'a>) -> Self {
        let scopes = ScopeStack::new(unit.context().arena(), unit.context().interner());
        scopes.push(globals);

        Self {
            unit,
            scopes,
            globals,
        }
    }

    /// Current lexical scope, or globals if the stack invariant is broken.
    #[inline]
    pub fn current(&self) -> &'a Scope<'a> {
        match self.scopes.try_current() {
            Some(scope) => scope,
            None => {
                tracing::error!(
                    unit_index = self.unit.index(),
                    "scope stack was empty during binding, falling back to globals"
                );
                self.globals
            }
        }
    }

    /// Current scope stack depth. Depth 1 is globals only.
    #[inline]
    pub fn depth(&self) -> usize {
        self.scopes.depth()
    }

    /// Push a scope by id.
    pub fn push_scope(&mut self, id: ScopeId) -> bool {
        let Some(scope) = self.try_scope(id) else {
            return false;
        };
        self.scopes.push(scope);
        true
    }

    fn push_scope_recursive(&mut self, id: ScopeId) -> bool {
        let Some(scope) = self.try_scope(id) else {
            return false;
        };
        self.scopes.push_recursive(scope);
        true
    }

    /// Push a HIR scope node's semantic scope.
    pub fn push_node_scope(&mut self, sn: &'a HirScope<'a>) -> bool {
        let Some(scope) = sn.try_scope() else {
            return false;
        };
        if sn.try_ident().is_some() {
            self.push_scope_recursive(scope.id())
        } else {
            self.push_scope(scope.id())
        }
    }

    /// Pop the current scope, keeping globals.
    #[inline]
    pub fn pop_scope(&mut self) {
        if self.scopes.depth() <= 1 {
            tracing::error!(
                unit_index = self.unit.index(),
                "attempted to pop binder global scope"
            );
            return;
        }
        self.scopes.pop();
    }

    /// Pop to `depth`, keeping globals.
    #[inline]
    pub fn pop_to(&mut self, depth: usize) {
        self.scopes.pop_until(depth.max(1));
    }

    /// Shared global scope.
    #[inline]
    pub fn globals(&self) -> &'a Scope<'a> {
        self.globals
    }

    fn try_scope(&self, id: ScopeId) -> Option<&'a Scope<'a>> {
        let scope = self.unit.try_scope(id);
        if scope.is_none() {
            tracing::warn!(
                unit_index = self.unit.index(),
                scope_id = id.0,
                "scope id not found during binding"
            );
        }
        scope
    }

    fn choose(&self, symbols: &[&'a Symbol]) -> Option<&'a Symbol> {
        try_resolve_ambiguous(symbols, self.unit.index(), self.unit.package_index())
    }

    /// All matching global symbols.
    #[inline]
    pub fn lookup_globals(&self, name: &str, kind_filters: SymKindSet) -> Option<Vec<&'a Symbol>> {
        if name.is_empty() {
            return None;
        }
        let options = SymbolFilter::kinds(kind_filters);
        let name_key = self.unit.interner().intern(name);
        self.globals.try_lookup_symbols(name_key, options)
    }

    /// Preferred global symbol.
    #[inline]
    pub fn lookup_global(&self, name: &str, kind_filters: SymKindSet) -> Option<&'a Symbol> {
        let symbols = self.lookup_globals(name, kind_filters)?;
        if symbols.len() > 1 {
            tracing::warn!(
                name,
                count = symbols.len(),
                "multiple global symbols found, using preferred symbol"
            );
        }
        self.choose(&symbols)
    }

    /// All matching lexical symbols.
    #[inline]
    pub fn lookup_symbols(&self, name: &str, kind_filters: SymKindSet) -> Option<Vec<&'a Symbol>> {
        let options = SymbolFilter::kinds(kind_filters);
        self.scopes.try_lookup_symbols(name, options)
    }

    /// Preferred lexical symbol.
    #[inline]
    pub fn lookup_symbol(&self, name: &str, kind_filters: SymKindSet) -> Option<&'a Symbol> {
        let symbols = self.lookup_symbols(name, kind_filters)?;
        if symbols.len() > 1 {
            tracing::warn!(
                name,
                count = symbols.len(),
                "multiple symbols found, using preferred symbol"
            );
        }
        self.choose(&symbols)
    }

    /// Preferred member symbol.
    pub fn lookup_member(
        &self,
        obj_type_symbol: &'a Symbol,
        member_name: &str,
        kind_filters: SymKindSet,
    ) -> Option<&'a Symbol> {
        if !kind_filters.is_empty() {
            for kind in SymKind::member_lookup_order() {
                if kind_filters.contains(kind) {
                    let sym = self.lookup_member_kind(obj_type_symbol, member_name, kind);
                    if sym.is_some() {
                        return sym;
                    }
                }
            }
        }

        let filter = if kind_filters.is_empty() {
            SymbolFilter::any()
        } else {
            SymbolFilter::kinds(kind_filters)
        };
        self.lookup_member_with_filter(obj_type_symbol, member_name, filter)
    }

    /// Member symbol of one kind.
    pub fn lookup_member_kind(
        &self,
        obj_type_symbol: &'a Symbol,
        member_name: &str,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let options = SymbolFilter::kinds(SymKindSet::from_kind(kind));
        self.lookup_member_with_filter(obj_type_symbol, member_name, options)
    }

    fn lookup_member_with_filter(
        &self,
        obj_type_symbol: &'a Symbol,
        member_name: &str,
        options: SymbolFilter,
    ) -> Option<&'a Symbol> {
        // Type aliases such as `Self` delegate lookup to their target type.
        let actual_sym = if obj_type_symbol.kind() == SymKind::TypeAlias {
            if let Some(type_of_id) = obj_type_symbol.type_of() {
                self.unit.try_symbol(type_of_id).unwrap_or_else(|| {
                    tracing::warn!(
                        symbol_id = ?type_of_id,
                        "type alias target symbol not found during member lookup"
                    );
                    obj_type_symbol
                })
            } else {
                obj_type_symbol
            }
        } else {
            obj_type_symbol
        };

        let scope_id = actual_sym.try_owned_scope()?;
        let scope = self.try_scope(scope_id)?;

        // Isolate member lookup from lexical scopes.
        let scopes = ScopeStack::new(self.unit.context().arena(), self.unit.context().interner());
        scopes.push_recursive(scope);

        scopes
            .try_lookup_symbols(member_name, options)?
            .into_iter()
            .last()
    }

    /// All matches for a qualified path, such as `foo::Bar::baz`.
    pub fn lookup_path(&self, path: &[&str], kind_filters: SymKindSet) -> Option<Vec<&'a Symbol>> {
        let mut options = QualifiedLookup::lexical();
        if !kind_filters.is_empty() {
            options = options.with_result_kinds(kind_filters)
        }
        let symbols = self.scopes.try_lookup_qualified(path, options)?;
        Some(symbols)
    }

    /// Preferred match for a qualified path.
    pub fn lookup_path_symbol(
        &self,
        path: &[&str],
        kind_filters: SymKindSet,
    ) -> Option<&'a Symbol> {
        let symbols = self.lookup_path(path, kind_filters)?;
        if symbols.len() > 1 {
            tracing::warn!(
                path = ?path,
                count = symbols.len(),
                "multiple symbols found for qualified path, using preferred symbol"
            );
        }
        self.choose(&symbols)
    }
}

/// Bind all compilation units.
///
/// Binding resolves references after collection has discovered every symbol.
/// Recoverable misses are recorded on symbols and logged by language visitors.
pub fn bind_symbols<'a, L: Language>(
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
    let parallel_ms = elapsed_ms(parallel_start);
    let bind_cpu_ms = bind_cpu_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let total_ms = elapsed_ms(total_start);

    tracing::info!(
        parallel_ms,
        bind_cpu_ms,
        total_ms,
        "symbol binding complete"
    );
    Ok(())
}
