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
        self.scopes.iter().last().unwrap()
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
    #[inline]
    pub fn scope_depth(&self) -> usize {
        self.scopes.depth()
    }

    /// Pushes a new scope created from a symbol onto the stack.
    pub fn push_scope(&mut self, id: ScopeId) {
        let scope = self.unit.get_scope(id);
        self.scopes.push(scope);
    }

    pub fn push_scope_recursive(&mut self, id: ScopeId) {
        let scope = self.unit.get_scope(id);
        self.scopes.push_recursive(scope);
    }

    /// Pushes the scope represented by `sn`, recursing when the HIR already points
    /// at an existing nested scope (e.g., structs/impls store their own scope nodes).
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
        self.scopes.pop();
    }

    /// Pops scopes until reaching the specified depth.
    pub fn pop_until(&mut self, depth: usize) {
        self.scopes.pop_until(depth);
    }

    /// Gets the global scope.
    #[inline]
    pub fn globals(&self) -> &'a Scope<'a> {
        self.scopes
            .iter()
            .first()
            .expect("global scope should always be present")
    }

    /// Lookup symbol using another symbol's name (local) and FQN (global).
    pub fn lookup_symbol(&self, name_sym: &'a Symbol, option: LookupOptions) -> Option<&'a Symbol> {
        self.scopes.lookup_symbol(name_sym, option)
    }

    /// Look up a symbol only in the global scope using FQN.
    pub fn lookup_globals(&self, name_sym: &'a Symbol) -> Option<&'a Symbol> {
        self.scopes.lookup_symbol(name_sym, LookupOptions::global())
    }

    /// Lookup global symbols by name.
    pub fn lookup_globals_with(&self, name: &str, kind_filter: Option<SymKind>) -> Option<Vec<&'a Symbol>> {
        let globals = self.scopes.first();
        let name_key = self.unit.cc.interner.intern(name);
        globals.lookup_symbols(name_key).filter(|syms| {
            if let Some(k) = kind_filter {
                syms.iter().any(|s| s.kind() == k)
            } else {
                true
            }
        })
    }

    /// Lookup symbol with kind filters.
    pub fn lookup_symbol_with(
        &self,
        name_sym: &'a Symbol,
        kind_filters: Vec<SymKind>,
    ) -> Option<&'a Symbol> {
        let mut option = LookupOptions::current();
        if !kind_filters.is_empty() {
            option = option.with_kind_filters(kind_filters);
        }
        self.scopes.lookup_symbol(name_sym, option)
    }

    /// String-based symbol lookup with kind filters (for identifiers without pre-set symbols)
    pub fn lookup_symbol_by_name(
        &self,
        name: &str,
        kind_filters: Vec<SymKind>,
    ) -> Option<&'a Symbol> {
        let mut option = LookupOptions::current();
        if !kind_filters.is_empty() {
            option = option.with_kind_filters(kind_filters);
        }
        self.scopes.lookup_symbol_by_name(name, option)
    }

    /// Lookup member symbol using owner's scope.
    pub fn lookup_member_symbol(
        &self,
        obj_type_symbol: &'a Symbol,
        member_sym: &'a Symbol,
        kind_filter: Option<SymKind>,
    ) -> Option<&'a Symbol> {
        let scope_id = obj_type_symbol.opt_scope()?;
        let scope = self.unit.get_scope(scope_id);

        // Create isolated scope stack for member lookup
        let scopes = ScopeStack::new(&self.unit.cc.arena, &self.unit.cc.interner);
        scopes.push_recursive(scope);

        let mut option = LookupOptions::current();
        if let Some(k) = kind_filter {
            let kinds = if k == SymKind::Function {
                vec![SymKind::Function, SymKind::Method]
            } else {
                vec![k]
            };
            option = option.with_kind_filters(kinds);
        }

        scopes.lookup_symbol(member_sym, option)
    }
}

/// Bind symbols, optionally in parallel based on config.
pub fn bind_symbols_with<'a, L: LanguageTraitImpl>(
    cc: &'a CompileCtxt<'a>,
    globals: &'a Scope<'a>,
    config: &ResolverOption,
) {
    let bind_unit = |unit_index: usize| {
        let unit = cc.compile_unit(unit_index);
        let id = unit.file_root_id().unwrap();
        let node = unit.hir_node(id);
        L::bind_symbols(unit, node, globals, config);
    };

    if config.sequential {
        (0..cc.files.len()).for_each(bind_unit);
    } else {
        (0..cc.files.len()).into_par_iter().for_each(bind_unit);
    }
}
