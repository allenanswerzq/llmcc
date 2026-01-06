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
        tracing::trace!("pushing scope {:?}", scope.id());
        self.scopes.push(scope);
    }

    /// Push scope recursively with all parent scopes
    #[inline]
    pub fn push_scope_recursive(&mut self, scope: &'a Scope<'a>) {
        tracing::trace!("pushing scope recursively {:?}", scope.id());
        self.scopes.push_recursive(scope);
    }

    /// Create and push a new scope with optional associated symbol.
    /// If the symbol already has a scope, use that scope instead of creating a new one.
    #[inline]
    pub fn push_scope_with(&mut self, node: &HirNode<'a>, symbol: Option<&'a Symbol>) {
        // Check if symbol already has a scope (from previous unit processing)
        if let Some(symbol) = symbol
            && let Some(existing_scope_id) = symbol.opt_scope()
        {
            // Reuse the existing scope
            if let Some(existing_scope) = self.cc.opt_get_scope(existing_scope_id) {
                tracing::trace!(
                    "reusing existing scope {:?} for symbol {}",
                    existing_scope_id,
                    symbol.format(Some(self.interner())),
                );
                self.push_scope(existing_scope);
                return;
            }
        }

        // Create new scope with its own ID, then alloc with that ID
        let scope_val = Scope::new_with(node.id(), symbol, Some(self.interner()));
        let scope_id = scope_val.id().0;
        let scope = self.arena().alloc_with_id(scope_id, scope_val);
        if let Some(symbol) = symbol {
            tracing::trace!(
                "set symbol scope {} to {:?}",
                symbol.format(Some(self.interner())),
                scope.id(),
            );
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
        tracing::trace!("popping scope, stack depth: {}", self.scopes.depth());
        self.scopes.pop();
    }

    /// Pop scopes until reaching target depth
    #[inline]
    pub fn pop_until(&mut self, depth: usize) {
        tracing::trace!(
            "popping scopes until depth {}, current: {}",
            depth,
            self.scopes.depth()
        );
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
    fn init_symbol(&self, symbol: &'a Symbol, name: &str, node: &HirNode<'a>, kind: SymKind) {
        if symbol.kind() == SymKind::Unknown {
            symbol.set_owner(node.id());
            symbol.set_kind(kind);
            symbol.set_unit_index(self.unit_index());
            symbol.add_defining(node.id());
            if let Some(parent) = self.top() {
                symbol.set_parent_scope(parent.id());
            }
            tracing::trace!("init_symbol: {} id={:?}", name, symbol.id());
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

    /// Lookup symbols by name with options
    #[inline]
    pub fn lookup_symbols(&self, name: &str, kind_filters: SymKindSet) -> Option<Vec<&'a Symbol>> {
        tracing::trace!("lookup symbols '{}' with filters {:?}", name, kind_filters);
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
            let current_crate_root = self.cc.unit_metas.get(self.unit_index).and_then(|m| m.package_root.as_ref());
            if let Some(current_root) = current_crate_root {
                if let Some(same_crate_sym) = symbols.iter().find(|s| {
                    s.unit_index()
                        .and_then(|idx| self.cc.unit_metas.get(idx))
                        .and_then(|meta| meta.package_root.as_ref())
                        .is_some_and(|r| r == current_root)
                }) {
                    tracing::trace!(
                        "preferring same-crate symbol for '{}' from crate root '{:?}'",
                        name,
                        current_root
                    );
                    return Some(*same_crate_sym);
                }
            }

            tracing::warn!(
                "multiple symbols found for '{}', returning the last one",
                name
            );
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

    tracing::info!(
        "starting symbol collection for totaol {} units",
        cc.files.len()
    );

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
        tracing::debug!(
            "collecting symbols for unit {} ({})",
            i,
            unit.file_path().unwrap_or("unknown")
        );

        let visit_start = Instant::now();
        let node = unit.hir_node(unit.file_root_id().unwrap());
        let unit_globals = L::collect_symbols(unit, node, unit_scope_stack, config);
        visit_time_ns.fetch_add(visit_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        if config.print_ir {
            use llmcc_core::printer::print_llmcc_ir;
            tracing::debug!("=== IR for unit {} ===", i);
            let _ = print_llmcc_ir(unit);
        }

        unit_globals
    };

    let parallel_start = Instant::now();
    let unit_globals_vec = if config.sequential {
        tracing::debug!("running symbol collection sequentially");
        (0..cc.files.len()).map(collect_unit).collect::<Vec<_>>()
    } else {
        tracing::debug!("running symbol collection in parallel");
        (0..cc.files.len())
            .into_par_iter()
            .map(collect_unit)
            .collect::<Vec<_>>()
    };
    let parallel_time = parallel_start.elapsed();

    let globals = scope_stack.globals();

    // No sorting needed: DashMap provides O(1) lookup by ID

    let merge_start = Instant::now();
    tracing::debug!(
        "merging {} unit scopes into global scope",
        unit_globals_vec.len()
    );
    for (i, unit_globals) in unit_globals_vec.iter().enumerate() {
        tracing::trace!("merging unit {} global scope", i);
        cc.merge_two_scopes(globals, unit_globals);
    }
    let merge_time = merge_start.elapsed();

    let total_time = total_start.elapsed();
    let clone_ms = clone_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let visit_ms = visit_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;

    tracing::info!(
        "collection breakdown: init={:.2}ms, parallel={:.2}ms (clone={:.2}ms, visit={:.2}ms), merge={:.2}ms, total={:.2}ms",
        init_time.as_secs_f64() * 1000.0,
        parallel_time.as_secs_f64() * 1000.0,
        clone_ms,
        visit_ms,
        merge_time.as_secs_f64() * 1000.0,
        total_time.as_secs_f64() * 1000.0,
    );

    tracing::info!("symbol collection complete");
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
) -> Result<&'a Scope<'a>, llmcc_core::DynError> {
    use llmcc_core::ir_builder::{build_llmcc_ir_inner, reset_ir_build_counters};
    use std::sync::atomic::Ordering;

    let total_start = Instant::now();
    reset_ir_build_counters();

    tracing::info!(
        "starting fused IR build + symbol collection for {} units",
        cc.files.len()
    );

    // Initialize scope stack for collection
    let init_start = Instant::now();
    let scope_stack = L::collect_init(cc);
    let scope_stack_clone = scope_stack.clone();
    let init_time = init_start.elapsed();

    // Atomic counters for timing
    let ir_build_ns = AtomicU64::new(0);
    let collect_ns = AtomicU64::new(0);

    // Fused per-file operation: IR build then collect
    let build_and_collect_unit = |i: usize| -> Result<&'a Scope<'a>, llmcc_core::DynError> {
        // Phase 1: IR Build
        let ir_start = Instant::now();

        let file_path = cc.file_path(i).map(|p| p.to_string());
        let file_bytes = cc.files[i].content();

        tracing::debug!(
            "start fusing build+collect for unit {} ({}:{} bytes)",
            i,
            file_path.as_deref().unwrap_or("unknown"),
            file_bytes.len()
        );

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

        tracing::debug!(
            "fused build+collect for unit {} ({})",
            i,
            unit.file_path().unwrap_or("unknown")
        );

        let node = unit.hir_node(file_root_id);
        let unit_globals = L::collect_symbols(unit, node, unit_scope_stack, resolver_config);

        collect_ns.fetch_add(collect_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        if resolver_config.print_ir {
            use llmcc_core::printer::print_llmcc_ir;
            tracing::debug!("=== IR for unit {} ===", i);
            let _ = print_llmcc_ir(unit);
        }

        Ok(unit_globals)
    };

    // Run fused operation in parallel
    let parallel_start = Instant::now();
    let unit_globals_vec: Vec<Result<&'a Scope<'a>, llmcc_core::DynError>> =
        if resolver_config.sequential {
            tracing::debug!("running fused build+collect sequentially");
            (0..cc.files.len()).map(build_and_collect_unit).collect()
        } else {
            tracing::debug!("running fused build+collect in parallel");
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
    tracing::debug!(
        "merging {} unit scopes into global scope",
        unit_globals_vec.len()
    );
    for (i, unit_globals) in unit_globals_vec.iter().enumerate() {
        tracing::trace!("merging unit {} global scope", i);
        cc.merge_two_scopes(globals, unit_globals);
    }
    let merge_time = merge_start.elapsed();

    let total_time = total_start.elapsed();
    let ir_ms = ir_build_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let collect_ms = collect_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;

    tracing::info!(
        "fused build+collect breakdown: init={:.2}ms, parallel={:.2}ms (ir_cpu={:.2}ms, collect_cpu={:.2}ms), merge={:.2}ms, total={:.2}ms",
        init_time.as_secs_f64() * 1000.0,
        parallel_time.as_secs_f64() * 1000.0,
        ir_ms,
        collect_ms,
        merge_time.as_secs_f64() * 1000.0,
        total_time.as_secs_f64() * 1000.0,
    );

    tracing::info!("fused build+collect complete");
    Ok(globals)
}
