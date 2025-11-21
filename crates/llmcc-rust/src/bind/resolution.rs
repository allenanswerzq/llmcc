use std::vec;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::LangRust;

pub struct SymbolResolver<'a, 'tcx> {
    pub unit: &'a CompileUnit<'tcx>,
    pub scopes: &'a mut BinderScopes<'tcx>,
}

impl<'a, 'tcx> SymbolResolver<'a, 'tcx> {
    pub fn new(unit: &'a CompileUnit<'tcx>, scopes: &'a mut BinderScopes<'tcx>) -> Self {
        Self { unit, scopes }
    }

    /// Finds (or reuses) the symbol declared in a specific field.
    pub fn symbol_from_field(
        &self,
        node: &HirNode<'tcx>,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        if let Some(ident) = node.as_ident() {
            if let Some(existing) = self.scopes.lookup_symbol(&ident.name) {
                return Some(existing);
            }
            ident.opt_symbol()
        } else {
            let ident = node.child_identifier_by_field(*self.unit, field_id)?;
            if let Some(existing) = self.scopes.lookup_symbol(&ident.name) {
                return Some(existing);
            }
            ident.opt_symbol()
        }
    }

    /// Extracts a human-readable identifier from the given node.
    pub fn identifier_name(&self, node: &HirNode<'tcx>) -> Option<String> {
        if let Some(ident) = node.as_ident() {
            return Some(Self::normalize_identifier(&ident.name));
        }
        if let Some(ident) = node.find_identifier(*self.unit) {
            return Some(Self::normalize_identifier(&ident.name));
        }
        if node.kind_id() == LangRust::super_token {
            return Some("super".to_string());
        }
        if node.kind_id() == LangRust::crate_token {
            return Some("crate".to_string());
        }
        None
    }

    /// Normalizes a fully qualified name by returning only the last component.
    pub fn normalize_identifier(name: &str) -> String {
        name.rsplit("::").next().unwrap_or(name).to_string()
    }

    /// Returns the first child node; handy for wrappers like `(expr)` or `await`.
    pub fn first_child_node(&self, node: &HirNode<'tcx>) -> Option<HirNode<'tcx>> {
        let child_id = node.children().first()?;
        Some(self.unit.hir_node(*child_id))
    }

    /// Looks up a callable symbol (function or macro) by name.
    pub fn lookup_callable_symbol(&self, name: &str) -> Option<&'tcx Symbol> {
        if let Some(symbol) = self.scopes.lookup_symbol_with(
            name,
            Some(vec![SymKind::Function, SymKind::Macro]),
            None,
            None,
        ) {
            return Some(symbol);
        }
        None
    }

    /// Resolves `macro_invocation` nodes to their macro symbol (e.g., `log!`).
    pub fn resolve_macro_symbol(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let macro_node = node.child_by_field(*self.unit, LangRust::field_macro)?;
        if macro_node.kind_id() == LangRust::scoped_identifier {
            return self.resolve_scoped_identifier_symbol(&macro_node, None);
        }
        let name = self.identifier_name(&macro_node)?;
        self.lookup_callable_symbol(&name)
    }

    /// Resolves a type node (like `Vec<Foo>`) into the `Foo`/`Vec` symbols.
    pub fn resolve_type_from_node(&mut self, type_node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        if type_node.kind_id() == LangRust::scoped_identifier
            || type_node.kind_id() == LangRust::scoped_type_identifier
        {
            if let Some(sym) = self.resolve_scoped_identifier_symbol(type_node, None) {
                return Some(sym);
            }
        }

        let ident = type_node.find_identifier(*self.unit)?;

        if let Some(existing) = self.scopes.lookup_symbol_with(
            &ident.name,
            Some(vec![
                SymKind::Struct,
                SymKind::Enum,
                SymKind::Trait,
                SymKind::TypeAlias,
                SymKind::TypeParameter,
                SymKind::Primitive,
                SymKind::UnresolvedType,
            ]),
            None,
            None,
        ) {
            return Some(existing);
        }

        self.scopes.lookup_or_insert_global(&ident.name, type_node, SymKind::UnresolvedType)
    }

    /// Resolves the `crate` keyword to the crate root symbol.
    pub fn resolve_crate_root(&self) -> Option<&'tcx Symbol> {
        self.scopes.scopes().iter().into_iter().find_map(|s| {
            if let Some(sym) = s.symbol()
                && sym.kind() == SymKind::Crate
            {
                return Some(sym);
            }
            None
        })
    }

    /// Resolves the `super` keyword relative to a given anchor symbol or the current scope.
    pub fn resolve_super_relative_to(&self, anchor: Option<&Symbol>) -> Option<&'tcx Symbol> {
        let stack = self.scopes.scopes().iter();

        // Determine the starting point in the scope stack.
        let base_index = if let Some(anchor_sym) = anchor {
            // Case 1: Anchor provided (e.g., `foo::super`).
            // Find the index of the scope that corresponds to `foo`.
            let anchor_scope_id = anchor_sym.scope()?;
            stack.iter().rposition(|s| s.id() == anchor_scope_id)?
        } else {
            // Case 2: No anchor (e.g., `super::foo`).
            // Find the index of the nearest module-like scope in the current stack.
            // This represents the "current module".
            stack.iter().enumerate().rev().find_map(|(i, s)| {
                if let Some(sym) = s.symbol()
                    && matches!(
                        sym.kind(),
                        SymKind::Module | SymKind::File | SymKind::Crate | SymKind::Namespace
                    )
                {
                    return Some(i);
                }
                None
            })?
        };

        // Search upwards from the base index to find the parent module.
        stack.iter().take(base_index).rev().find_map(|s| {
            if let Some(sym) = s.symbol()
                && matches!(
                    sym.kind(),
                    SymKind::Module | SymKind::File | SymKind::Crate | SymKind::Namespace
                )
            {
                return Some(sym);
            }
            None
        })
    }

    /// Resolves a scoped identifier (e.g. `Foo::bar`) to a symbol.
    pub fn resolve_scoped_identifier_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        caller: Option<&Symbol>,
    ) -> Option<&'tcx Symbol> {
        let children = node.children_nodes(self.unit);
        let non_trivia: Vec<_> = children
            .iter()
            .filter(|n| !matches!(n.kind(), HirKind::Text | HirKind::Comment))
            .collect();

        if non_trivia.len() < 2 {
            return None;
        }

        let path_node = non_trivia.first()?;
        let name_node = non_trivia.last()?;
        let name = self.identifier_name(name_node)?;

        let path_symbol = if path_node.kind_id() == LangRust::scoped_identifier {
            self.resolve_scoped_identifier_symbol(path_node, caller)?
        } else if path_node.kind_id() == LangRust::super_token {
            self.resolve_super_relative_to(None)?
        } else if path_node.kind_id() == LangRust::crate_token {
            self.resolve_crate_root()?
        } else {
            let path_name = self.identifier_name(path_node)?;
            let sym = self.scopes.lookup_symbol(&path_name)?;
            if let Some(c) = caller {
                c.add_dependency(sym);
            }
            sym
        };

        if name_node.kind_id() == LangRust::super_token {
            return self.resolve_super_relative_to(Some(path_symbol));
        }

        self.scopes.lookup_member_symbol(path_symbol, &name, None)
    }

    pub fn is_self_type(&self, symbol: &Symbol) -> bool {
        self.unit.interner().resolve_owned(symbol.name).as_deref() == Some("Self")
    }
}
