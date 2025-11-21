use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::symbol::Symbol;
use llmcc_resolver::BinderScopes;

use crate::token::LangRust;
use super::resolution::SymbolResolver;

pub struct SymbolLinker<'a, 'tcx> {
    pub unit: &'a CompileUnit<'tcx>,
    pub scopes: &'a mut BinderScopes<'tcx>,
}

impl<'a, 'tcx> SymbolLinker<'a, 'tcx> {
    pub fn new(unit: &'a CompileUnit<'tcx>, scopes: &'a mut BinderScopes<'tcx>) -> Self {
        Self { unit, scopes }
    }

    /// Records a symbol's declared type and dependency on that type.
    pub fn link_symbol_with_type(symbol: &Symbol, ty: &Symbol) {
        if symbol.type_of().is_none() {
            symbol.set_type_of(ty.id());
        }
        symbol.add_dependency(ty);
    }

    /// Helper that walks all identifier leaves inside a type expression.
    fn visit_type_identifiers<F>(&self, node: &HirNode<'tcx>, f: &mut F)
    where
        F: FnMut(String),
    {
        if let Some(ident) = node.as_ident() {
            f(SymbolResolver::normalize_identifier(&ident.name));
        }
        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            self.visit_type_identifiers(&child, f);
        }
    }

    pub fn link_type_references(
        &mut self,
        type_node: &HirNode<'tcx>,
        symbol: &Symbol,
        owner: Option<&Symbol>,
    ) {
        // We need to collect names first to avoid borrowing scopes mutably inside the closure
        // while we might need it elsewhere, but here we just lookup.
        // However, visit_type_identifiers is recursive.
        // Let's just implement the logic directly or use a collector.

        let mut names = Vec::new();
        self.visit_type_identifiers(type_node, &mut |name| names.push(name));

        for name in names {
            if let Some(target) = self.scopes.lookup_symbol(&name) {
                symbol.add_dependency(target);
                if let Some(owner) = owner {
                    owner.add_dependency(target);
                }
            }
        }
    }

    pub fn link_trait_bounds(
        &mut self,
        node: &HirNode<'tcx>,
        symbol: &Symbol,
        owner: Option<&Symbol>,
    ) {
        if let Some(bounds_node) = node.child_by_field(*self.unit, LangRust::field_bounds) {
            self.link_type_references(&bounds_node, symbol, owner);
        }
    }

    pub fn link_where_clause_dependencies(
        &mut self,
        node: &HirNode<'tcx>,
        owner: &Symbol,
    ) {
        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            if child.kind_id() == LangRust::where_predicate {
                if let Some(bounds_node) = child.child_by_field(*self.unit, LangRust::field_bounds) {
                    self.link_type_references(&bounds_node, owner, None);
                }
            } else if !child.children().is_empty() {
                self.link_where_clause_dependencies(&child, owner);
            }
        }
    }

    /// Reads the `type` child (if present) and associates it with the symbol.
    pub fn set_symbol_type_from_field(
        &mut self,
        node: &HirNode<'tcx>,
        symbol: &Symbol,
        owner: Option<&Symbol>,
        field_id: u16,
    ) {
        if let Some(type_node) = node.child_by_field(*self.unit, field_id) {
            let mut resolver = SymbolResolver::new(self.unit, self.scopes);
            if let Some(ty) = resolver.resolve_type_from_node(&type_node) {
                Self::link_symbol_with_type(symbol, ty);
                if let Some(owner) = owner {
                    owner.add_dependency(ty);
                }
            }
            self.link_type_references(&type_node, symbol, owner);
        }
    }

    /// Assign inferred type to pattern (for let bindings, parameters, etc.)
    #[allow(clippy::only_used_in_recursion)]
    pub fn assign_type_to_pattern(
        &mut self,
        pattern: &HirNode<'tcx>,
        ty: &'tcx Symbol,
    ) {
        if matches!(
            pattern.kind_id(),
            LangRust::scoped_identifier | LangRust::scoped_type_identifier
        ) {
            return;
        }

        match pattern.kind() {
            HirKind::Identifier => {
                if let Some(ident) = pattern.as_ident() {
                    if let Some(sym) = ident.opt_symbol() {
                        sym.set_type_of(ty.id());
                        sym.add_dependency(ty);
                    }
                }
            }
            _ => {
                // For complex patterns (tuple patterns, struct patterns), visit all identifiers
                for child_id in pattern.children() {
                    let child = self.unit.hir_node(*child_id);
                    self.assign_type_to_pattern(&child, ty);
                }
            }
        }
    }

    pub fn link_pattern_type_references(
        &mut self,
        pattern: &HirNode<'tcx>,
        type_node: &HirNode<'tcx>,
        owner: Option<&Symbol>,
    ) {
        match pattern.kind() {
            HirKind::Identifier => {
                if let Some(ident) = pattern.as_ident() {
                    if let Some(sym) = ident.opt_symbol() {
                        self.link_type_references(type_node, sym, owner);
                    }
                }
            }
            _ => {
                for child_id in pattern.children() {
                    let child = self.unit.hir_node(*child_id);
                    self.link_pattern_type_references(&child, type_node, owner);
                }
            }
        }
    }
}
