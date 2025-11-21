use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternPool;
use llmcc_core::ir::HirNode;
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
    pub fn globals(&self) -> &'a Scope<'a> {
        self.scopes
            .iter()
            .first()
            .expect("global scope should always be present")
    }

    /// Find or insert symbol in the current scope.
    #[inline]
    pub fn lookup_or_insert(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert(name, node)?;
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
        let symbol = self.scopes.lookup_or_insert_chained(name, node)?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Find or insert symbol in the parent scope.
    pub fn lookup_or_insert_parent(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_parent(name, node)?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Find or insert symbol in the global scope.
    pub fn lookup_or_insert_global(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_global(name, node)?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    /// Full control API for symbol lookup and insertion with custom options.
    pub fn lookup_or_insert_with(
        &self,
        name: &str,
        node: &HirNode<'a>,
        kind: SymKind,
        options: LookupOptions,
    ) -> Option<&'a Symbol> {
        let symbol = self.scopes.lookup_or_insert_with(name, node, options)?;
        symbol.set_kind(kind);
        Some(symbol)
    }

    pub fn lookup_symbol(&self, name: &str) -> Option<&'a Symbol> {
        self.scopes.lookup_symbol(name)
    }

    pub fn lookup_symbol_with(
        &self,
        name: &str,
        kind_filters: Option<Vec<SymKind>>,
        unit_filters: Option<Vec<usize>>,
        fqn_filters: Option<Vec<&str>>,
    ) -> Option<&'a Symbol> {
        self.scopes
            .lookup_symbol_with(name, kind_filters, unit_filters, fqn_filters)
    }

    pub fn lookup_member_symbol(
        &self,
        obj_type_symbol: &'a Symbol,
        member_name: &str,
        kind_filter: Option<SymKind>,
    ) -> Option<&'a Symbol> {
        let scope_id = obj_type_symbol.scope()?;
        let scope = self.unit.get_scope(scope_id);
        let name_key = self.unit.interner().intern(member_name);
        let symbols = scope.lookup_symbols(name_key)?;
        symbols.into_iter().rev().find(|symbol| match kind_filter {
            None => true,
            Some(SymKind::Function) => {
                matches!(symbol.kind(), SymKind::Function | SymKind::Method)
            }
            Some(expected) => symbol.kind() == expected,
        })
    }
}

/// parallel binding symbols
pub fn bind_symbols_with<'a, L: LanguageTraitImpl>(
    cc: &'a CompileCtxt<'a>,
    globals: &'a Scope<'a>,
    config: &ResolverOption,
) {
    (0..cc.files.len()).into_par_iter().for_each(|unit_index| {
        let unit = cc.compile_unit(unit_index);
        let id = unit.file_root_id().unwrap();
        let node = unit.hir_node(id);
        let mut scopes = BinderScopes::new(unit, globals);
        L::bind_symbols(&unit, &node, &mut scopes, globals, config);
    })
}
