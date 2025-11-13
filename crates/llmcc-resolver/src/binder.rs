use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternedStr;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};
use llmcc_descriptor::{CallTarget, PathQualifier, TypeExpr};

use crate::call_target::CallTargetResolver;
use crate::collector::CollectionResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationDirection {
    Forward,
    Backward,
}

#[derive(Debug)]
pub struct BinderCore<'tcx, 'a> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    collection: &'a CollectionResult,
    relation_direction: RelationDirection,
}

impl<'tcx, 'a> BinderCore<'tcx, 'a> {
    pub fn new(
        unit: CompileUnit<'tcx>,
        globals: &'tcx Scope<'tcx>,
        collection: &'a CollectionResult,
    ) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner, &unit.cc.symbol_map);
        scopes.push(globals);

        Self {
            unit,
            scopes,
            collection,
            relation_direction: RelationDirection::Forward,
        }
    }

    #[inline]
    pub fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    #[inline]
    pub fn interner(&self) -> &llmcc_core::interner::InternPool {
        self.unit.interner()
    }

    #[inline]
    pub fn collection(&self) -> &'a CollectionResult {
        self.collection
    }

    #[inline]
    pub fn scopes(&self) -> &ScopeStack<'tcx> {
        &self.scopes
    }

    #[inline]
    pub fn scopes_mut(&mut self) -> &mut ScopeStack<'tcx> {
        &mut self.scopes
    }

    #[inline]
    pub fn scope_symbol(&self) -> Option<&'tcx Symbol> {
        self.scopes.scoped_symbol()
    }

    fn lookup_in_locals(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        unit_index: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        let scopes: Vec<_> = self.scopes.iter().collect();
        for scope in scopes.into_iter().skip(1).rev() {
            let symbols = scope.lookup_suffix_symbols(suffix, kind, unit_index);
            if symbols.len() == 1 {
                return Some(symbols[0]);
            } else {
                tracing::warn!(
                    "multiple local symbols found for suffix {:?} (kind={:?}, unit={:?}): {:?}",
                    suffix,
                    kind,
                    unit_index,
                    symbols
                        .iter()
                        .map(|symbol| symbol.fqn_name.read().clone())
                        .collect::<Vec<_>>()
                );
            }
        }
        None
    }

    fn lookup_in_globals(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        unit_index: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        if let Some(global_scope) = self.scopes.iter().next() {
            let symbols = global_scope.lookup_suffix_symbols(suffix, kind, unit_index);
            if symbols.len() == 1 {
                return Some(symbols[0]);
            } else {
                tracing::warn!(
                    "multiple global symbols found for suffix {:?} (kind={:?}, unit={:?}): {:?}",
                    suffix,
                    kind,
                    unit_index,
                    symbols
                        .iter()
                        .map(|symbol| symbol.fqn_name.read().clone())
                        .collect::<Vec<_>>()
                );
            }
        }
        None
    }

    /// Look up a symbol by its suffix parts, optionally filtering by kind and unit index.
    /// This method first searches in local scopes, then in global scope.
    fn lookup_symbol_suffix(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        unit_index: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        self.lookup_in_locals(suffix, kind, unit_index)
            .or_else(|| self.lookup_in_globals(suffix, kind, unit_index))
    }

    pub fn lookup_symbol_only_in_globals(
        &self,
        symbol: &[String],
        kind: Option<SymbolKind>,
        unit_index: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        if symbol.is_empty() {
            return None;
        }

        let suffix: Vec<_> = symbol
            .iter()
            .rev()
            .map(|segment| self.interner().intern(segment))
            .collect();

        self.lookup_in_globals(&suffix, kind, unit_index)
    }

    pub fn lookup_symbol(
        &self,
        symbol: &[String],
        kind: Option<SymbolKind>,
        unit_index: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        if symbol.is_empty() {
            return None;
        }

        let suffix: Vec<_> = symbol
            .iter()
            .rev()
            .map(|segment| self.interner().intern(segment))
            .collect();

        self.lookup_symbol_suffix(&suffix, kind, unit_index)
    }

    pub fn lookup_symbol_kind_priority(
        &self,
        symbol: &[String],
        kinds: &[SymbolKind],
        unit_index: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        if symbol.is_empty() {
            return None;
        }

        for &kind in kinds {
            if let Some(found) = self.lookup_symbol(symbol, Some(kind), unit_index) {
                return Some(found);
            }
        }

        None
    }

    pub fn lookup_symbol_with(
        &self,
        node: &HirNode<'tcx>,
        field_id: u16,
        kind: SymbolKind,
    ) -> Option<&'tcx Symbol> {
        let child = node.opt_child_by_field(self.unit(), field_id)?;
        let ident = child.as_ident()?;
        let key = self.interner().intern(&ident.name);
        self.lookup_symbol_suffix(&[key], Some(kind), None)
    }

    pub fn lookup_expr_symbols(&self, expr: &TypeExpr) -> Vec<&'tcx Symbol> {
        let mut symbols = Vec::new();
        self.lookup_expr_symbols_with(expr, SymbolKind::Struct, &mut symbols);
        self.lookup_expr_symbols_with(expr, SymbolKind::Enum, &mut symbols);
        self.lookup_expr_symbols_with(expr, SymbolKind::InferredType, &mut symbols);
        symbols
    }

    fn lookup_expr_symbols_with(
        &self,
        expr: &TypeExpr,
        kind: SymbolKind,
        symbols: &mut Vec<&'tcx Symbol>,
    ) {
        match expr {
            TypeExpr::Path {
                qualifier,
                generics,
            } => {
                if let Some(symbol) = self.lookup_symbol(qualifier.parts(), Some(kind), None) {
                    if !symbols.iter().any(|existing| existing.id == symbol.id) {
                        symbols.push(symbol);
                    }
                }

                for generic in generics {
                    self.lookup_expr_symbols_with(generic, kind, symbols);
                }
            }
            TypeExpr::Reference { inner, .. } => {
                self.lookup_expr_symbols_with(inner, kind, symbols);
            }
            TypeExpr::Tuple(items) => {
                for item in items {
                    self.lookup_expr_symbols_with(item, kind, symbols);
                }
            }
            TypeExpr::Callable { parameters, result } => {
                for parameter in parameters {
                    self.lookup_expr_symbols_with(parameter, kind, symbols);
                }
                if let Some(result) = result.as_deref() {
                    self.lookup_expr_symbols_with(result, kind, symbols);
                }
            }
            TypeExpr::ImplTrait { .. } | TypeExpr::Opaque { .. } | TypeExpr::Unknown(_) => {}
        }
    }

    pub fn propagate_child_dependencies(&self, parent: &'tcx Symbol, child: &'tcx Symbol) {
        let dependencies: Vec<_> = child.depends.read().clone();
        for dep_id in dependencies {
            if dep_id == parent.id {
                continue;
            }

            if let Some(dep_symbol) = self.unit().opt_get_symbol(dep_id) {
                if dep_symbol.kind() == SymbolKind::Function {
                    continue;
                }

                if dep_symbol.depends.read().contains(&parent.id) {
                    continue;
                }

                parent.add_dependency(dep_symbol);
            }
        }
    }

    pub fn lookup_call_symbols(&self, target: &CallTarget, symbols: &mut Vec<&'tcx Symbol>) {
        let resolver = CallTargetResolver::new(self);
        resolver.resolve(target, symbols);
    }

    /// Resolves the call target, then binds the inferred return type to a variable assignment.
    /// The resolved symbols are returned so callers can continue building dependency graphs.
    pub fn lookup_call_and_bind_variable(
        &self,
        target: &CallTarget,
        var_name: &str,
        scope: Option<&'tcx Scope<'tcx>>,
    ) -> Vec<&'tcx Symbol> {
        let mut symbols = Vec::new();
        self.lookup_call_symbols(target, &mut symbols);

        if symbols.is_empty() {
            return symbols;
        }

        self.bind_call_receivers_to_variable(var_name, &symbols);

        if let Some(scope) = scope {
            let var_key = vec![var_name.to_string()];
            if self
                .lookup_symbol(&var_key, Some(SymbolKind::Variable), None)
                .is_some()
            {
                for symbol in &symbols {
                    if let Some(receiver) = self.lookup_return_receivers(symbol).into_iter().next()
                    {
                        self.bind_variable_type_alias(scope, var_name, receiver);
                        break;
                    }
                }
            }
        }

        symbols
    }

    fn symbol_type_of(&self, symbol: &'tcx Symbol) -> Option<&'tcx Symbol> {
        symbol
            .type_of()
            .and_then(|sym_id| self.unit().opt_get_symbol(sym_id))
    }

    fn lookup_receiver_from_symbol(&self, symbol: &'tcx Symbol) -> Option<&'tcx Symbol> {
        let mut parts = symbol.path_segments();
        if parts.len() <= 1 {
            return None;
        }
        parts.pop();

        self.lookup_symbol_kind_priority(
            &parts,
            &[SymbolKind::Struct, SymbolKind::Enum, SymbolKind::Trait],
            None,
        )
    }

    pub fn lookup_return_receivers(&self, symbol: &'tcx Symbol) -> Vec<&'tcx Symbol> {
        match symbol.kind() {
            SymbolKind::Struct | SymbolKind::Enum | SymbolKind::Trait => vec![symbol],
            SymbolKind::Function => {
                if let Some(result) = self.symbol_type_of(symbol) {
                    return vec![result];
                }
                if let Some(receiver) = self.lookup_receiver_from_symbol(symbol) {
                    return vec![receiver];
                }
                Vec::new()
            }
            _ => self.symbol_type_of(symbol).into_iter().collect(),
        }
    }

    pub fn set_backward_relation(&mut self) {
        self.relation_direction = RelationDirection::Backward;
    }

    pub fn set_forward_relation(&mut self) {
        self.relation_direction = RelationDirection::Forward;
    }

    fn add_relation(&self, segments: &[String], scope_symbol: Option<&'tcx Symbol>) {
        if segments.is_empty() {
            return;
        }

        let Some(target_symbol) = self.lookup_symbol(segments, None, None) else {
            return;
        };

        if target_symbol.kind() == SymbolKind::Variable {
            return;
        }

        let Some(scope_symbol) = scope_symbol else {
            return;
        };

        if scope_symbol.kind() == SymbolKind::Variable {
            return;
        }

        match self.relation_direction {
            RelationDirection::Forward => scope_symbol.add_dependency(target_symbol),
            RelationDirection::Backward => target_symbol.add_dependency(scope_symbol),
        }
    }

    pub fn resolve_identifier_with<F>(&self, node: HirNode<'tcx>, parser: F)
    where
        F: FnOnce(&str) -> PathQualifier,
    {
        let text = self.unit.hir_text(&node);
        let qualifier = parser(&text);
        let segments: Vec<String> = qualifier
            .parts()
            .iter()
            .map(|segment| segment.trim().to_string())
            .filter(|segment| !segment.is_empty())
            .collect();
        let current = self.scope_symbol();
        self.add_relation(&segments, current);
    }

    /// Binds the return type of a function call to a variable in a variable assignment.
    pub fn bind_call_receivers_to_variable(
        &self,
        var_name: &str,
        resolved_symbols: &[&'tcx Symbol],
    ) {
        if resolved_symbols.is_empty() {
            return;
        }

        // Look up the variable in the current scope
        if let Some(variable) =
            self.lookup_symbol(&[var_name.to_string()], Some(SymbolKind::Variable), None)
        {
            // For each resolved symbol (the function that was called)
            for symbol in resolved_symbols {
                // Get what that function returns (its receiver type)
                for receiver in self.lookup_return_receivers(symbol) {
                    // Set the variable's type to the return type
                    variable.set_type_of(Some(receiver.id));

                    // Note: Language-specific binders will create the alias in their scope
                    // using bind_variable_type_alias after this method returns
                    break;
                }
                break;
            }
        }
    }

    /// Helper to insert a type alias for a variable in a given scope.
    pub fn bind_variable_type_alias(
        &self,
        scope: &'tcx Scope<'tcx>,
        var_name: &str,
        receiver_symbol: &'tcx Symbol,
    ) {
        let alias_parts = vec![var_name.to_string()];
        scope.insert_alias(&alias_parts, self.interner(), receiver_symbol);
    }
}
