//! Symbol collection for parallel per-unit symbol table building.
use llmcc_core::LanguageTrait;
use llmcc_core::context::CompileCtxt;
use llmcc_core::interner::InternPool;
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
        tracing::trace!("pushing scope {:?}", scope.id());
        self.scopes.push(scope);
    }

    /// Push scope recursively with all parent scopes
    #[inline]
    pub fn push_scope_recursive(&mut self, scope: &'a Scope<'a>) {
        tracing::trace!("pushing scope recursively {:?}", scope.id());
        self.scopes.push_recursive(scope);
    }

    /// Create and push a new scope with optional associated symbol
    #[inline]
    pub fn push_scope_with(&mut self, node: &HirNode<'a>, symbol: Option<&'a Symbol>) {
        let scope = self
            .arena
            .alloc(Scope::new_with(node.id(), symbol, Some(self.interner)));
        if let Some(symbol) = symbol {
            tracing::trace!(
                "set symbol scope {} to {:?}",
                symbol.format(Some(self.interner)),
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
        self.interner
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
            symbol.add_defining(node.id());
            if let Some(parent) = self.top() {
                symbol.set_parent_scope(parent.id());
            }
            tracing::trace!(
                "initialized symbol '{}'",
                symbol.format(Some(self.interner))
            );
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
        tracing::trace!("lookup or insert scope stack '{}' in current", name);
        let symbols = self
            .scopes
            .lookup_or_insert(name, node.id(), LookupOptions::current())?;
        let symbol = symbols.last().copied()?;
        self.init_symbol(symbol, name, node, kind);
        tracing::trace!("found symbol '{}'", symbol.format(Some(self.interner)));
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
        tracing::trace!("lookup or insert scope stack '{}' in global scope", name);
        let symbols = self
            .scopes
            .lookup_or_insert(name, node.id(), LookupOptions::global())?;
        let symbol = symbols.last().copied()?;
        self.init_symbol(symbol, name, node, kind);
        symbol.set_is_global(true);
        Some(symbol)
    }

    /// Lookup symbols by name with options
    #[inline]
    pub fn lookup_symbols(
        &self,
        name: &str,
        kind_filters: Vec<SymKind>,
    ) -> Option<Vec<&'a Symbol>> {
        tracing::trace!(
            "lookup symbols '{}' with filters {:?}",
            name,
            kind_filters.clone()
        );
        let options = LookupOptions::current().with_kind_filters(kind_filters);
        self.scopes.lookup_symbols(name, options)
    }

    #[inline]
    pub fn lookup_symbol(&self, name: &str, kind_filters: Vec<SymKind>) -> Option<&'a Symbol> {
        let symbols = self.lookup_symbols(name, kind_filters)?;
        if symbols.len() > 1 {
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
    tracing::info!(
        "starting symbol collection for totaol {} units",
        cc.files.len()
    );

    let scope_stack = L::collect_init(cc);
    let scope_stack_clone = scope_stack.clone();

    let collect_unit = move |i: usize| {
        tracing::debug!("collecting symbols for unit {}", i);
        let unit_scope_stack = scope_stack_clone.clone();
        let unit = cc.compile_unit(i);
        let node = unit.hir_node(unit.file_root_id().unwrap());
        let unit_globals = L::collect_symbols(unit, node, unit_scope_stack, config);

        if config.print_ir {
            use llmcc_core::printer::print_llmcc_ir;
            tracing::debug!("=== IR for unit {} ===", i);
            let _ = print_llmcc_ir(unit);
        }

        unit_globals
    };

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

    let globals = scope_stack.globals();

    tracing::debug!(
        "merging {} unit scopes into global scope",
        unit_globals_vec.len()
    );
    for (i, unit_globals) in unit_globals_vec.iter().enumerate() {
        tracing::trace!("merging unit {} global scope", i);
        cc.merge_two_scopes(globals, unit_globals);
    }

    tracing::debug!("sorting scopes and symbols");
    cc.arena.scope_sort_by(|scope| scope.id());
    cc.arena.symbol_sort_by(|symbol| symbol.id());

    tracing::info!("symbol collection complete");
    globals
}
