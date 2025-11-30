use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternPool;
use llmcc_core::ir::{HirNode, HirScope};
use llmcc_core::scope::{LookupOptions, Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, Symbol};
use llmcc_core::{CompileCtxt, LanguageTraitImpl};

use rayon::prelude::*;

use crate::ResolverOption;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationDirection {
    Forward,
    Backward,
}

#[derive(Debug)]
pub struct BinderScopes<'a> {
    unit: CompileUnit<'a>,
    scopes: ScopeStack<'a>,
    relation_direction: RelationDirection,
}

impl<'a> BinderScopes<'a> {
    pub fn new(unit: CompileUnit<'a>, globals: &'a Scope<'a>) -> Self {
        let scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner);
        scopes.push(globals);

        Self {
            unit,
            scopes,
            relation_direction: RelationDirection::Forward,
        }
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
    pub fn set_forward_relation(&mut self) {
        self.relation_direction = RelationDirection::Forward;
    }

    #[inline]
    pub fn set_backward_relation(&mut self) {
        self.relation_direction = RelationDirection::Backward;
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
        tracing::trace!("pushing scope {:?}", id);
        let scope = self.unit.get_scope(id);
        self.scopes.push(scope);
    }

    /// Pushes a scope recursively with all its parent scopes.
    pub fn push_scope_recursive(&mut self, id: ScopeId) {
        tracing::trace!("pushing scope recursively {:?}", id);
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
        tracing::trace!("popping scope, stack depth: {}", self.scopes.depth());
        self.scopes.pop();
    }

    /// Pops scopes until reaching the specified depth.
    #[inline]
    pub fn pop_until(&mut self, depth: usize) {
        tracing::trace!(
            "popping scopes until depth {}, current: {}",
            depth,
            self.scopes.depth()
        );
        self.scopes.pop_until(depth);
    }

    /// Gets the global scope (always at index 0).
    #[inline]
    pub fn globals(&self) -> &'a Scope<'a> {
        self.scopes.globals()
    }

    /// Find or insert symbol in the current scope.
    #[inline]
    pub fn lookup_or_insert(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        tracing::trace!("looking up or inserting '{}' in current scope", name);
        let symbols = self
            .scopes
            .lookup_or_insert(name, node.id(), LookupOptions::current())?;
        let symbol = symbols.last().copied()?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Find or insert symbol with chaining enabled for shadowing support.
    #[inline]
    pub fn lookup_or_insert_chained(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        tracing::trace!(
            "looking up or inserting chained '{}' in current scope",
            name
        );
        let symbols = self
            .scopes
            .lookup_or_insert(name, node.id(), LookupOptions::chained())?;
        let symbol = symbols.last().copied()?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Find or insert symbol in the parent scope.
    #[inline]
    pub fn lookup_or_insert_parent(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        tracing::trace!("looking up or inserting '{}' in parent scope", name);
        let symbols = self
            .scopes
            .lookup_or_insert(name, node.id(), LookupOptions::parent())?;
        let symbol = symbols.last().copied()?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Find or insert symbol in the global scope.
    #[inline]
    pub fn lookup_or_insert_global(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        tracing::trace!("looking up or inserting '{}' in global scope", name);
        let symbols = self
            .scopes
            .lookup_or_insert(name, node.id(), LookupOptions::global())?;
        let symbol = symbols.last().copied()?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Find or insert symbol with custom lookup options.
    #[inline]
    pub fn lookup_or_insert_with(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
        options: LookupOptions,
    ) -> Option<&'a Symbol> {
        tracing::trace!("looking up or inserting '{}' with custom options", name);
        let symbols = self.scopes.lookup_or_insert(name, node.id(), options)?;
        let symbol = symbols.last().copied()?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Look up a symbol in the scope stack.
    #[inline]
    pub fn lookup_symbol(&self, name: &str) -> Option<&'a Symbol> {
        self.scopes
            .lookup_symbols(name, LookupOptions::current())?
            .into_iter()
            .last()
    }

    /// Look up a symbol only in the global scope.
    #[inline]
    pub fn lookup_global_symbol(&self, name: &str) -> Option<&'a Symbol> {
        self.scopes
            .lookup_symbols(name, LookupOptions::global())?
            .into_iter()
            .last()
    }

    /// Look up a member symbol in a type's scope.
    pub fn lookup_member_symbol(
        &self,
        obj_type_symbol: &'a Symbol,
        member_name: &str,
        kind_filter: Option<SymKind>,
    ) -> Option<&'a Symbol> {
        tracing::trace!("looking up member '{}' in type scope", member_name);
        let scope_id = obj_type_symbol.opt_scope()?;
        let scope = self.unit.get_scope(scope_id);

        // Create isolated scope stack for member lookup to avoid falling back to lexical scopes
        let scopes = ScopeStack::new(&self.unit.cc.arena, &self.unit.cc.interner);
        scopes.push_recursive(scope);

        let mut options = LookupOptions::current();
        if let Some(filter) = kind_filter {
            options = options.with_kind_filters(vec![filter]);
        }

        scopes
            .lookup_symbols(member_name, options)?
            .into_iter()
            .last()
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
    tracing::info!("starting symbol binding for {} units", cc.files.len());

    let bind_unit = |unit_index: usize| {
        tracing::debug!("binding symbols for unit {}", unit_index);
        let unit = cc.compile_unit(unit_index);
        let id = unit.file_root_id().unwrap();
        let node = unit.hir_node(id);
        L::bind_symbols(unit, node, globals, config);
    };

    if config.sequential {
        tracing::debug!("running symbol binding sequentially");
        (0..cc.files.len()).for_each(bind_unit);
    } else {
        tracing::debug!("running symbol binding in parallel");
        (0..cc.files.len()).into_par_iter().for_each(bind_unit);
    }

    tracing::info!("symbol binding complete");
}
