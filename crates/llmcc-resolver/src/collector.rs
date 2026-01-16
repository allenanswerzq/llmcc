//! Symbol collection for parallel per-unit symbol table building.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use llmcc_core::LanguageTrait;
use llmcc_core::context::CompileCtxt;
use llmcc_core::interner::InternPool;
use llmcc_core::ir::{Arena, HirNode};
use llmcc_core::scope::{LookupOptions, Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, SymKindSet, Symbol};

use rayon::prelude::*;

use crate::ResolverOption;

/// Core symbol collector for a single compilation unit
pub struct CollectorScopes<'a> {
    cc: &'a CompileCtxt<'a>,
    unit_index: usize,
    scopes: ScopeStack<'a>,
    globals: &'a Scope<'a>,
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
            cc,
            unit_index,
            scopes,
            globals,
        }
    }

    /// Get compilation unit index
    #[inline]
    pub fn unit_index(&self) -> usize {
        self.unit_index
    }

    /// Get the crate index for the current unit
    #[inline]
    pub fn crate_index(&self) -> usize {
        self.cc
            .unit_metas
            .get(self.unit_index)
            .map(|m| m.crate_index)
            .unwrap_or(usize::MAX)
    }

    /// Get the arena
    #[inline]
    pub fn arena(&self) -> &'a Arena<'a> {
        &self.cc.arena
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

    /// Push scope recursively with all parent scopes
    #[inline]
    pub fn push_scope_recursive(&mut self, scope: &'a Scope<'a>) {
        self.scopes.push_recursive(scope);
    }

    /// Create and push a new scope with optional associated symbol.
    /// If the symbol already has a scope, use that scope instead of creating a new one.
    #[inline]
    pub fn push_scope_with(&mut self, node: &HirNode<'a>, symbol: Option<&'a Symbol>) {
        if let Some(symbol) = symbol
            && let Some(existing_scope_id) = symbol.opt_scope()
            && let Some(existing_scope) = self.cc.opt_get_scope(existing_scope_id)
        {
            self.push_scope(existing_scope);
            return;
        }

        // Create new scope with its own ID, then alloc with that ID
        let scope_val = Scope::new_with(node.id(), symbol, Some(self.interner()));
        let scope_id = scope_val.id().0;
        let scope = self.arena().alloc_with_id(scope_id, scope_val);
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
    #[inline]
    pub fn pop_until(&mut self, depth: usize) {
        self.scopes.pop_until(depth);
    }

    /// Get shared string interner
    #[inline]
    pub fn interner(&self) -> &'a InternPool {
        &self.cc.interner
    }

    /// Get global (module-level) scope
    #[inline]
    pub fn globals(&self) -> &'a Scope<'a> {
        self.globals
    }

    /// Get the scope stack
    #[inline]
    pub fn scopes(&self) -> &ScopeStack<'a> {
        &self.scopes
    }

    /// Get the current (top) scope on the stack
    #[inline]
    pub fn top(&self) -> Option<&'a Scope<'a>> {
        self.scopes.top()
    }

    /// Initialize a symbol with common properties
    fn init_symbol(&self, symbol: &'a Symbol, _name: &str, node: &HirNode<'a>, kind: SymKind) {
        if symbol.kind() == SymKind::Unknown {
            symbol.set_owner(node.id());
            symbol.set_kind(kind);
            symbol.set_unit_index(self.unit_index());
            symbol.set_crate_index(self.crate_index());
            symbol.add_defining(node.id());
            if let Some(parent) = self.top() {
                symbol.set_parent_scope(parent.id());
            }
        }
    }

    /// Find or insert symbol in current
    #[inline]
    pub fn lookup_or_insert(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let symbols = self
            .scopes
            .lookup_or_insert(name, node.id(), LookupOptions::current())?;
        let symbol = symbols.last().copied()?;
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
        // Use kind filter to avoid collisions between symbols of different kinds
        // e.g., crate "auth" and file "auth" should be separate symbols
        let options = LookupOptions::global().with_kind_set(SymKindSet::from_kind(kind));
        let symbols = self.scopes.lookup_or_insert(name, node.id(), options)?;
        let symbol = symbols.last().copied()?;
        self.init_symbol(symbol, name, node, kind);
        symbol.set_is_global(true);
        Some(symbol)
    }

    /// Always insert a new symbol in global scope, even if one with the same name exists.
    /// This is needed for per-crate module symbols (e.g., each crate's `tui` module
    /// should be a separate symbol, not shared).
    #[inline]
    pub fn insert_in_global(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let name_key = self.interner().intern(name);
        let new_symbol = Symbol::new(node.id(), name_key);
        let sym_id = new_symbol.id().0;
        let allocated = self.arena().alloc_with_id(sym_id, new_symbol);
        self.globals.insert(allocated);
        self.init_symbol(allocated, name, node, kind);
        allocated.set_is_global(true);
        Some(allocated)
    }

    /// Insert a new symbol into a specific scope.
    /// This is used for inserting module symbols into the crate scope for qualified path resolution
    /// (e.g., `crate_b::utils::helper` needs `utils` to be in `crate_b`'s scope).
    #[inline]
    pub fn insert_in_scope(
        &self,
        scope: &'a Scope<'a>,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let name_key = self.interner().intern(name);
        let new_symbol = Symbol::new(node.id(), name_key);
        let sym_id = new_symbol.id().0;
        let allocated = self.arena().alloc_with_id(sym_id, new_symbol);
        scope.insert(allocated);
        self.init_symbol(allocated, name, node, kind);
        Some(allocated)
    }

    /// Lookup symbols by name with options
    #[inline]
    pub fn lookup_symbols(&self, name: &str, kind_filters: SymKindSet) -> Option<Vec<&'a Symbol>> {
        let options = LookupOptions::current().with_kind_set(kind_filters);
        self.scopes.lookup_symbols(name, options)
    }

    #[inline]
    pub fn lookup_symbol(&self, name: &str, kind_filters: SymKindSet) -> Option<&'a Symbol> {
        let symbols = self.lookup_symbols(name, kind_filters)?;
        if symbols.len() > 1 {
            // 1. Prefer symbols from the current unit (same file)
            if let Some(local_sym) = symbols
                .iter()
                .find(|s| s.unit_index() == Some(self.unit_index))
            {
                return Some(*local_sym);
            }

            // 2. Prefer symbols from the same crate (same package root)
            let current_crate_root = self
                .cc
                .unit_metas
                .get(self.unit_index)
                .and_then(|m| m.package_root.as_ref());
            if let Some(current_root) = current_crate_root
                && let Some(same_crate_sym) = symbols.iter().find(|s| {
                    s.unit_index()
                        .and_then(|idx| self.cc.unit_metas.get(idx))
                        .and_then(|meta| meta.package_root.as_ref())
                        .is_some_and(|r| r == current_root)
                })
            {
                return Some(*same_crate_sym);
            }

            tracing::warn!(name, count = symbols.len(), "multiple symbols found");
        }
        symbols.last().copied()
    }
}

/// Collect symbols from all compilation units by invoking language-specific visitor.
///
/// At the collect pass, we can only know all the stuff in a single compilation unit due to
/// random order of collection. For symbols we can't resolve at the unit level, we create
/// placeholder symbols and resolve them in the later binding phase.
pub fn collect_symbols_with<'a, L: LanguageTrait>(
    cc: &'a CompileCtxt<'a>,
    config: &ResolverOption,
) -> &'a Scope<'a> {
    let total_start = Instant::now();
    let unit_count = cc.files.len();
    tracing::info!(unit_count, "starting symbol collection");

    let init_start = Instant::now();
    let scope_stack = L::collect_init(cc);
    let scope_stack_clone = scope_stack.clone();
    let init_time = init_start.elapsed();

    // Atomic counters for parallel timing
    let clone_time_ns = AtomicU64::new(0);
    let visit_time_ns = AtomicU64::new(0);

    let collect_unit = |i: usize| {
        let clone_start = Instant::now();
        let unit_scope_stack = scope_stack_clone.clone();
        clone_time_ns.fetch_add(clone_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        let unit = cc.compile_unit(i);

        let visit_start = Instant::now();
        let node = unit.hir_node(unit.file_root_id().unwrap());
        let unit_globals = L::collect_symbols(unit, node, unit_scope_stack, config);
        visit_time_ns.fetch_add(visit_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        if config.print_ir {
            use llmcc_core::printer::print_llmcc_ir;
            let _ = print_llmcc_ir(unit);
        }

        unit_globals
    };

    let parallel_start = Instant::now();
    let unit_globals_vec = if config.sequential {
        (0..cc.files.len()).map(collect_unit).collect::<Vec<_>>()
    } else {
        (0..cc.files.len())
            .into_par_iter()
            .map(collect_unit)
            .collect::<Vec<_>>()
    };
    let parallel_time = parallel_start.elapsed();

    let globals = scope_stack.globals();

    let merge_start = Instant::now();
    for unit_globals in unit_globals_vec.iter() {
        cc.merge_two_scopes(globals, unit_globals);
    }
    let merge_time = merge_start.elapsed();

    let total_time = total_start.elapsed();
    let clone_ms = clone_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let visit_ms = visit_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;

    let total_ms = total_time.as_secs_f64() * 1000.0;
    let init_ms = init_time.as_secs_f64() * 1000.0;
    let parallel_ms = parallel_time.as_secs_f64() * 1000.0;
    let merge_ms = merge_time.as_secs_f64() * 1000.0;

    tracing::info!(
        init_ms,
        parallel_ms,
        clone_ms,
        visit_ms,
        merge_ms,
        total_ms,
        "symbol collection complete"
    );
    globals
}

/// Fused IR build + symbol collection for better parallel efficiency.
///
/// This eliminates the gap between IR build and collection phases by doing both
/// in a single parallel pass. While the straggler file is still doing IR build,
/// other threads can finish both IR build AND collection for their files.
pub fn build_and_collect_symbols<'a, L: LanguageTrait>(
    cc: &'a CompileCtxt<'a>,
    ir_config: llmcc_core::ir_builder::IrBuildOption,
    resolver_config: &ResolverOption,
) -> Result<&'a Scope<'a>, llmcc_core::Error> {
    use llmcc_core::ir_builder::{build_llmcc_ir_inner, reset_ir_build_counters};
    use std::sync::atomic::Ordering;

    let total_start = Instant::now();
    reset_ir_build_counters();
    let unit_count = cc.files.len();
    tracing::info!(unit_count, "starting fused IR build + symbol collection");

    // Initialize scope stack for collection
    let init_start = Instant::now();
    let scope_stack = L::collect_init(cc);
    let scope_stack_clone = scope_stack.clone();
    let init_time = init_start.elapsed();

    // Atomic counters for timing
    let ir_build_ns = AtomicU64::new(0);
    let collect_ns = AtomicU64::new(0);

    // Fused per-file operation: IR build then collect
    let build_and_collect_unit = |i: usize| -> Result<&'a Scope<'a>, llmcc_core::Error> {
        // Phase 1: IR Build
        let ir_start = Instant::now();

        let file_path = cc.file_path(i).map(|p| p.to_string());
        let file_bytes = cc.files[i].content();

        let parse_tree = cc
            .get_parse_tree(i)
            .ok_or_else(|| format!("No parse tree for unit {i}"))?;

        let file_root_id =
            build_llmcc_ir_inner::<L>(file_path, file_bytes, parse_tree, &cc.arena, ir_config)?;

        // Register the file root ID immediately so collection can use it
        cc.set_file_root_id(i, file_root_id);

        ir_build_ns.fetch_add(ir_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        // Phase 2: Collection (immediately after IR build for this file)
        let collect_start = Instant::now();

        let unit_scope_stack = scope_stack_clone.clone();
        let unit = cc.compile_unit(i);

        let node = unit.hir_node(file_root_id);
        let unit_globals = L::collect_symbols(unit, node, unit_scope_stack, resolver_config);

        collect_ns.fetch_add(collect_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        if resolver_config.print_ir {
            use llmcc_core::printer::print_llmcc_ir;
            let _ = print_llmcc_ir(unit);
        }

        Ok(unit_globals)
    };

    // Run fused operation in parallel
    let parallel_start = Instant::now();
    let unit_globals_vec: Vec<Result<&'a Scope<'a>, llmcc_core::Error>> =
        if resolver_config.sequential {
            (0..cc.files.len()).map(build_and_collect_unit).collect()
        } else {
            (0..cc.files.len())
                .into_par_iter()
                .map(build_and_collect_unit)
                .collect()
        };
    let parallel_time = parallel_start.elapsed();

    // Unwrap results
    let unit_globals_vec: Vec<&'a Scope<'a>> = unit_globals_vec
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    let globals = scope_stack.globals();

    // Merge scopes (sequential, ~10-15ms)
    let merge_start = Instant::now();
    for unit_globals in unit_globals_vec.iter() {
        cc.merge_two_scopes(globals, unit_globals);
    }
    let merge_time = merge_start.elapsed();

    let total_time = total_start.elapsed();
    let ir_ms = ir_build_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let collect_ms = collect_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;

    let total_ms = total_time.as_secs_f64() * 1000.0;
    let init_ms = init_time.as_secs_f64() * 1000.0;
    let parallel_ms = parallel_time.as_secs_f64() * 1000.0;
    let merge_ms = merge_time.as_secs_f64() * 1000.0;

    tracing::info!(
        init_ms,
        parallel_ms,
        ir_ms,
        collect_ms,
        merge_ms,
        total_ms,
        "fused build+collect complete"
    );
    Ok(globals)
}
