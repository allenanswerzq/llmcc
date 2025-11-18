use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternPool;
use llmcc_core::ir::HirNode;
use llmcc_core::scope::{LookupOptions, Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, Symbol};
use llmcc_core::{CompileCtxt, LanguageTraitImpl};

use rayon::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationDirection {
    Forward,
    Backward,
}

#[derive(Debug)]
pub struct BinderScopes<'tcx> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    relation_direction: RelationDirection,
}

impl<'tcx> BinderScopes<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner);
        scopes.push(globals);

        Self {
            unit,
            scopes,
            relation_direction: RelationDirection::Forward,
        }
    }

    #[inline]
    pub fn unit(&self) -> CompileUnit<'tcx> {
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
    pub fn scopes(&self) -> &ScopeStack<'tcx> {
        &self.scopes
    }

    #[inline]
    pub fn scopes_mut(&mut self) -> &mut ScopeStack<'tcx> {
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

    /// Pushes a new scope created from a symbol onto the stack.
    pub fn push_scope(&mut self, id: ScopeId) {
        // NOTE: this is the biggest difference from CollectorScopes, we would expect
        // the scope must already exist in the CompileUnit
        let scope = self.unit.get_scope(id);
        self.scopes.push(scope);
    }

    pub fn push_scope_recursive(&mut self, id: ScopeId) {
        // NOTE: this is the biggest difference from CollectorScopes, we would expect
        // the scope must already exist in the CompileUnit
        let scope = self.unit.get_scope(id);
        self.scopes.push_recursive(scope);
    }

    /// Pops the current scope from the stack.
    #[inline]
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Pops scopes until reaching the specified depth.
    pub fn pop_until(&mut self, depth: usize) {
        self.scopes.pop_until(depth);
    }

    /// Gets the global scope.
    ///
    #[inline]
    pub fn globals(&self) -> &'tcx Scope<'tcx> {
        self.scopes
            .iter()
            .next()
            .expect("global scope should always be present")
    }

    /// Find or insert symbol in the current scope.
    #[inline]
    pub fn lookup_or_insert(
        &self,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let symbol = self.scopes.lookup_or_insert(name, node)?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Find or insert symbol with chaining enabled for shadowing support.
    #[inline]
    pub fn lookup_or_insert_chained(
        &self,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let symbol = self.scopes.lookup_or_insert_chained(name, node)?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Find or insert symbol in the parent scope.
    pub fn lookup_or_insert_parent(
        &self,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let symbol = self.scopes.lookup_or_insert_parent(name, node)?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Find or insert symbol in the global scope.
    pub fn lookup_or_insert_global(
        &self,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let symbol = self.scopes.lookup_or_insert_global(name, node)?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Full control API for symbol lookup and insertion with custom options.
    pub fn lookup_or_insert_with(
        &self,
        name: Option<&str>,
        node: &HirNode<'tcx>,
        kind: SymKind,
        options: LookupOptions,
    ) -> Option<&'tcx Symbol> {
        let symbol = self.scopes.lookup_or_insert_with(name, node, options)?;
        symbol.set_kind(kind);
        Some(symbol)
    }
}

#[derive(Default)]
pub struct BinderOption;

/// parallel binding symbols
pub fn bind_symbols_with<'a, L: LanguageTraitImpl>(
    cc: &'a CompileCtxt<'a>,
    globals: &'a Scope<'a>,
    _config: BinderOption,
) {
    (0..cc.files.len()).into_par_iter().for_each(|unit_index| {
        let unit = cc.compile_unit(unit_index);
        let id = unit.file_root_id().unwrap();
        let node = unit.hir_node(id);
        let mut scopes = BinderScopes::new(unit, globals);
        L::bind_symbols_impl(&unit, &node, &mut scopes, globals);
    })
}
