//! Symbol collection for parallel per-unit symbol table building.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use llmcc_core::context::{CompileCtxt, CompileUnit};
use llmcc_core::interner::InternPool;
use llmcc_core::ir::{Arena, HirNode};
use llmcc_core::scope::{InsertOptions, Scope, ScopeStack, SymbolFilter};
use llmcc_core::symbol::{SymKind, SymKindSet, Symbol};
use llmcc_core::{Language, Result};

use rayon::prelude::*;

use crate::{ResolveOptions, elapsed_ms, try_resolve_ambiguous};

/// Symbol collection context for one compilation unit.
///
/// The global scope stays at stack depth 1. Pop operations never remove it, so
/// language collectors cannot lose their merge target during traversal.
pub struct CollectCtxt<'a> {
    cc: &'a CompileCtxt<'a>,
    unit_index: usize,
    scopes: ScopeStack<'a>,
    globals: &'a Scope<'a>,
}

impl<'a> CollectCtxt<'a> {
    /// Create a collection context from a unit-local scope stack and globals.
    pub fn new(
        cc: &'a CompileCtxt<'a>,
        unit_index: usize,
        scopes: ScopeStack<'a>,
        globals: &'a Scope<'a>,
    ) -> Self {
        scopes.push(globals);
        Self {
            cc,
            unit_index,
            scopes,
            globals,
        }
    }

    /// Compilation unit index.
    #[inline]
    fn unit_index(&self) -> usize {
        self.unit_index
    }

    /// Package group index for the current unit.
    #[inline]
    fn package_index(&self) -> usize {
        self.cc.package_index(self.unit_index)
    }

    /// Shared arena.
    #[inline]
    fn arena(&self) -> &'a Arena<'a> {
        self.cc.arena()
    }

    /// Current scope stack depth. Depth 1 is globals only.
    #[inline]
    pub fn depth(&self) -> usize {
        self.scopes.depth()
    }

    /// Push a scope.
    #[inline]
    pub fn push_scope(&mut self, scope: &'a Scope<'a>) {
        self.scopes.push(scope);
    }

    /// Push a symbol-owned scope, creating one when needed
    #[inline]
    pub fn push_symbol_scope(
        &mut self,
        node: &HirNode<'a>,
        symbol: Option<&'a Symbol>,
    ) -> &'a Scope<'a> {
        if let Some(symbol) = symbol
            && let Some(existing_scope_id) = symbol.try_owned_scope()
        {
            if let Some(existing_scope) = self.cc.try_scope(existing_scope_id) {
                self.push_scope(existing_scope);
                return existing_scope;
            }
            tracing::warn!(
                unit_index = self.unit_index,
                scope_id = existing_scope_id.0,
                "symbol-owned scope missing during collection"
            );
        }

        let scope_val = Scope::new_with(node.id(), symbol, Some(self.interner()));
        let scope_id = scope_val.id().0;
        let scope = self.arena().alloc_with_id(scope_id, scope_val);
        if let Some(symbol) = symbol {
            symbol.set_owned_scope(scope.id());
        }
        self.push_scope(scope);
        scope
    }

    /// Declare and push a package/crate scope.
    pub fn push_package_scope(
        &mut self,
        node: &HirNode<'a>,
        package_name: &str,
    ) -> Option<&'a Scope<'a>> {
        let symbol = self.declare_global(package_name, node, SymKind::Package)?;
        Some(self.push_symbol_scope(node, Some(symbol)))
    }

    /// Declare a module wrapper scope with an optional semantic parent.
    pub fn module_scope(
        &self,
        node: &HirNode<'a>,
        module_name: &str,
        parent: Option<&'a Scope<'a>>,
    ) -> Option<&'a Scope<'a>> {
        let symbol = self.declare_global(module_name, node, SymKind::Module)?;
        let scope = self.alloc_symbol_scope(symbol);
        if let Some(parent) = parent {
            scope.add_parent(parent);
        }
        Some(scope)
    }

    /// Link a top-level file scope as a crate module alias.
    pub fn alias_file_module(
        &self,
        node: &HirNode<'a>,
        module_name: &str,
        file_scope: &'a Scope<'a>,
        crate_scope: Option<&'a Scope<'a>>,
    ) {
        if module_name == "lib" || module_name == "main" {
            return;
        }

        if let Some(symbol) = self.declare_fresh_global(module_name, node, SymKind::Module) {
            symbol.set_owned_scope(file_scope.id());
        }

        if let Some(crate_scope) = crate_scope
            && let Some(symbol) = self.declare_in(crate_scope, module_name, node, SymKind::Module)
        {
            symbol.set_owned_scope(file_scope.id());
        }
    }

    /// Pop the current scope, keeping globals.
    #[inline]
    pub fn pop_scope(&mut self) {
        if self.scopes.depth() <= 1 {
            tracing::error!(
                unit_index = self.unit_index,
                "attempted to pop collector global scope"
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

    /// Shared string interner.
    #[inline]
    fn interner(&self) -> &'a InternPool {
        self.cc.interner()
    }

    fn alloc_symbol_scope(&self, symbol: &'a Symbol) -> &'a Scope<'a> {
        let scope = Scope::new_with(symbol.owner(), Some(symbol), Some(self.interner()));
        let scope_id = scope.id().0;
        self.arena().alloc_with_id(scope_id, scope)
    }

    /// Shared global scope.
    #[inline]
    pub fn globals(&self) -> &'a Scope<'a> {
        self.globals
    }

    /// Current scope, or globals if the stack invariant is broken.
    #[inline]
    pub fn current(&self) -> &'a Scope<'a> {
        match self.scopes.try_current() {
            Some(scope) => scope,
            None => {
                tracing::error!(
                    unit_index = self.unit_index,
                    "scope stack was empty during collection, falling back to globals"
                );
                self.globals
            }
        }
    }

    fn init_symbol(&self, symbol: &'a Symbol, node: &HirNode<'a>, kind: SymKind) {
        if symbol.kind() == SymKind::Unknown {
            symbol.set_owner(node.id());
            symbol.set_kind(kind);
            symbol.set_unit_index(self.unit_index());
            symbol.set_package_index(self.package_index());
            symbol.add_defining(node.id());
        }
    }

    fn choose(&self, symbols: &[&'a Symbol]) -> Option<&'a Symbol> {
        try_resolve_ambiguous(symbols, self.unit_index(), self.package_index())
    }

    /// Declare or reuse a symbol in the current scope.
    #[inline]
    pub fn declare(&self, name: &str, node: &HirNode<'a>, kind: SymKind) -> Option<&'a Symbol> {
        let symbols =
            self.scopes
                .try_lookup_or_insert(name, node.id(), InsertOptions::current())?;
        let symbol = symbols.last().copied()?;
        self.init_symbol(symbol, node, kind);
        Some(symbol)
    }

    /// Declare or reuse a symbol in globals, separated by kind.
    #[inline]
    pub fn declare_global(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let options = InsertOptions::global().with_existing_kinds(SymKindSet::from_kind(kind));
        let symbols = self.scopes.try_lookup_or_insert(name, node.id(), options)?;
        let symbol = symbols.last().copied()?;
        self.init_symbol(symbol, node, kind);
        symbol.set_is_global(true);
        Some(symbol)
    }

    /// Declare a fresh global symbol, even when the name already exists.
    #[inline]
    fn declare_fresh_global(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        if name.is_empty() {
            return None;
        }
        let name_key = self.interner().intern(name);
        let new_symbol = Symbol::new(node.id(), name_key);
        let sym_id = new_symbol.id().0;
        let allocated = self.arena().alloc_with_id(sym_id, new_symbol);
        self.globals.insert(allocated);
        self.init_symbol(allocated, node, kind);
        allocated.set_is_global(true);
        Some(allocated)
    }

    /// Declare a fresh symbol in a specific scope.
    #[inline]
    fn declare_in(
        &self,
        scope: &'a Scope<'a>,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        if name.is_empty() {
            return None;
        }
        let name_key = self.interner().intern(name);
        let new_symbol = Symbol::new(node.id(), name_key);
        let sym_id = new_symbol.id().0;
        let allocated = self.arena().alloc_with_id(sym_id, new_symbol);
        scope.insert(allocated);
        self.init_symbol(allocated, node, kind);
        Some(allocated)
    }

    /// All matching lexical symbols.
    #[inline]
    fn lookup_symbols(&self, name: &str, kind_filters: SymKindSet) -> Option<Vec<&'a Symbol>> {
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
}

fn print_ir_if_enabled(unit: CompileUnit<'_>, config: &ResolveOptions) {
    if !config.print_ir {
        return;
    }

    if let Err(error) = llmcc_core::printer::print_ir(unit) {
        tracing::warn!(
            ?error,
            unit_index = unit.index(),
            "failed to print IR during collection"
        );
    }
}

struct CollectRun<'a> {
    globals: &'a Scope<'a>,
    init_ms: f64,
    parallel_ms: f64,
    merge_ms: f64,
    total_ms: f64,
}

fn run_collect_pass<'a, L, F>(
    cc: &'a CompileCtxt<'a>,
    config: &ResolveOptions,
    pass: &'static str,
    unit_pass: F,
) -> Result<CollectRun<'a>>
where
    L: Language,
    F: Fn(usize, &ScopeStack<'a>) -> Result<&'a Scope<'a>> + Send + Sync,
{
    let total_start = Instant::now();
    let unit_count = cc.unit_count();
    tracing::info!(unit_count, pass, "starting symbol collection pass");

    let init_start = Instant::now();
    let scope_stack = L::collect_init(cc);
    let scope_stack_clone = scope_stack.clone();
    let init_time = init_start.elapsed();

    let parallel_start = Instant::now();
    let unit_globals_vec = if config.sequential {
        (0..unit_count)
            .map(|unit_index| unit_pass(unit_index, &scope_stack_clone))
            .collect::<Vec<_>>()
    } else {
        (0..unit_count)
            .into_par_iter()
            .map(|unit_index| unit_pass(unit_index, &scope_stack_clone))
            .collect::<Vec<_>>()
    };
    let unit_globals_vec = unit_globals_vec.into_iter().collect::<Result<Vec<_>>>()?;
    let parallel_ms = elapsed_ms(parallel_start);

    let globals = scope_stack.globals();

    let merge_start = Instant::now();
    for unit_globals in unit_globals_vec.iter() {
        cc.merge_two_scopes(globals, unit_globals);
    }
    let merge_time = merge_start.elapsed();

    Ok(CollectRun {
        globals,
        init_ms: init_time.as_secs_f64() * 1000.0,
        parallel_ms,
        merge_ms: merge_time.as_secs_f64() * 1000.0,
        total_ms: elapsed_ms(total_start),
    })
}

fn collect_unit<'a, L: Language>(
    cc: &'a CompileCtxt<'a>,
    unit_index: usize,
    scope_stack: &ScopeStack<'a>,
    config: &ResolveOptions,
    clone_time_ns: &AtomicU64,
    visit_time_ns: &AtomicU64,
) -> Result<&'a Scope<'a>> {
    let clone_start = Instant::now();
    let unit_scope_stack = scope_stack.clone();
    clone_time_ns.fetch_add(clone_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

    let unit = cc.compile_unit(unit_index);

    let visit_start = Instant::now();
    let node = unit.hir_node(unit.file_root_id()?);
    let unit_globals = L::collect_symbols(unit, node, unit_scope_stack, config);
    visit_time_ns.fetch_add(visit_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

    print_ir_if_enabled(unit, config);

    Ok(unit_globals)
}

fn build_and_collect_unit<'a, L: Language>(
    cc: &'a CompileCtxt<'a>,
    unit_index: usize,
    scope_stack: &ScopeStack<'a>,
    config: &ResolveOptions,
    ir_build_ns: &AtomicU64,
    collect_ns: &AtomicU64,
) -> Result<&'a Scope<'a>> {
    let ir_start = Instant::now();

    let unit = cc.compile_unit(unit_index);
    let file_root_id = llmcc_core::ir_builder::build_file_hir::<L>(unit)?;

    cc.set_file_root_id(unit_index, file_root_id);
    ir_build_ns.fetch_add(ir_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

    let collect_start = Instant::now();

    let unit_scope_stack = scope_stack.clone();
    let node = unit.hir_node(file_root_id);
    let unit_globals = L::collect_symbols(unit, node, unit_scope_stack, config);

    collect_ns.fetch_add(collect_start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    print_ir_if_enabled(unit, config);

    Ok(unit_globals)
}

/// Collect symbols from all compilation units.
///
/// Collection is intentionally per-unit. Cross-unit references are represented
/// by placeholders and resolved by the later binding pass.
pub fn collect_symbols<'a, L: Language>(
    cc: &'a CompileCtxt<'a>,
    config: &ResolveOptions,
) -> Result<&'a Scope<'a>> {
    let clone_time_ns = AtomicU64::new(0);
    let visit_time_ns = AtomicU64::new(0);

    let run = run_collect_pass::<L, _>(cc, config, "collect", |unit_index, scope_stack| {
        collect_unit::<L>(
            cc,
            unit_index,
            scope_stack,
            config,
            &clone_time_ns,
            &visit_time_ns,
        )
    })?;

    let clone_ms = clone_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let visit_ms = visit_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;

    tracing::info!(
        init_ms = run.init_ms,
        parallel_ms = run.parallel_ms,
        clone_ms,
        visit_ms,
        merge_ms = run.merge_ms,
        total_ms = run.total_ms,
        "symbol collection complete"
    );
    Ok(run.globals)
}

/// Build HIR and collect symbols in one parallel pass.
///
/// This avoids a separate synchronization point between IR build and collection.
pub fn build_and_collect<'a, L: Language>(
    cc: &'a CompileCtxt<'a>,
    config: &ResolveOptions,
) -> Result<&'a Scope<'a>> {
    let ir_build_ns = AtomicU64::new(0);
    let collect_ns = AtomicU64::new(0);

    let run = run_collect_pass::<L, _>(
        cc,
        config,
        "build_and_collect",
        |unit_index, scope_stack| {
            build_and_collect_unit::<L>(
                cc,
                unit_index,
                scope_stack,
                config,
                &ir_build_ns,
                &collect_ns,
            )
        },
    )?;

    let ir_ms = ir_build_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let collect_ms = collect_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;

    tracing::info!(
        init_ms = run.init_ms,
        parallel_ms = run.parallel_ms,
        ir_ms,
        collect_ms,
        merge_ms = run.merge_ms,
        total_ms = run.total_ms,
        "fused build+collect complete"
    );
    Ok(run.globals)
}
