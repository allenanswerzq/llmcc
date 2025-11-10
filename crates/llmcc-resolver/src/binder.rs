use std::ptr;

use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternedStr;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};
use llmcc_descriptor::{CallChainRoot, CallKind, CallTarget, TypeExpr};

use crate::collector::CollectionResult;

#[derive(Debug)]
pub struct BinderCore<'tcx, 'a> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    collection: &'a CollectionResult,
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
    pub fn current_symbol(&self) -> Option<&'tcx Symbol> {
        self.scopes.scoped_symbol()
    }

    pub fn lookup_in_locals(
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

    pub fn lookup_in_globals(
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
    pub fn lookup_symbol_suffix(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        unit_index: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        self.lookup_in_locals(suffix, kind, unit_index)
            .or_else(|| self.lookup_in_globals(suffix, kind, unit_index))
    }

    pub fn lookup_symbol_in_globals(
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

    pub fn lookup_symbol_fqn(&self, symbol: &[String], kind: SymbolKind) -> Option<&'tcx Symbol> {
        if symbol.is_empty() {
            return None;
        }

        let fqn = symbol.join("::");
        if fqn.is_empty() {
            return None;
        }

        let symbol_map = self.unit().cc.symbol_map.read();
        symbol_map
            .values()
            .find(|sym| sym.kind() == kind && sym.fqn_name.read().as_str() == fqn)
            .copied()
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
        self.lookup_expr_symbols_with(expr, SymbolKind::DynamicType, &mut symbols);
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
        match target {
            CallTarget::Symbol(call) => {
                let mut parts = call.qualifiers.clone();
                parts.push(call.name.clone());

                match call.kind {
                    CallKind::Method => {
                        if let Some(method_symbol) =
                            self.lookup_symbol(&parts, Some(SymbolKind::Function), None)
                        {
                            self.push_symbol_unique(symbols, method_symbol);
                        }
                        self.push_type_from_qualifiers(symbols, &call.qualifiers);
                    }
                    CallKind::Constructor => {
                        if let Some(sym) = self.lookup_symbol_kind_priority(
                            &parts,
                            &[SymbolKind::Struct, SymbolKind::Enum],
                            None,
                        ) {
                            self.push_symbol_unique(symbols, sym);
                        }
                    }
                    CallKind::Macro => {
                        let symbol = self
                            .lookup_symbol(&parts, Some(SymbolKind::Macro), None)
                            .or_else(|| self.lookup_symbol_fqn(&parts, SymbolKind::Macro));
                        if let Some(sym) = symbol {
                            self.push_symbol_unique(symbols, sym);
                        }
                    }
                    CallKind::Function | CallKind::Unknown => {
                        let symbol = self
                            .lookup_symbol(&parts, Some(SymbolKind::Function), None)
                            .or_else(|| self.lookup_symbol_fqn(&parts, SymbolKind::Function));
                        if let Some(sym) = symbol {
                            self.push_symbol_unique(symbols, sym);
                        }
                        self.push_type_from_qualifiers(symbols, &call.qualifiers);
                    }
                }
            }
            CallTarget::Chain(chain) => {
                match &chain.root {
                    CallChainRoot::Expr(expr) => {
                        if let Some(symbol) = self.lookup_simple_path(expr) {
                            self.push_symbol_unique(symbols, symbol);
                        }
                    }
                    CallChainRoot::Invocation(invocation) => {
                        self.lookup_call_symbols(invocation.target.as_ref(), symbols);
                    }
                }

                for segment in &chain.parts {
                    match segment.kind {
                        CallKind::Constructor => {
                            if let Some(sym) = self.lookup_symbol(
                                &[segment.name.clone()],
                                Some(SymbolKind::Struct),
                                None,
                            ) {
                                self.push_symbol_unique(symbols, sym);
                            }
                            if let Some(sym) = self.lookup_symbol(
                                &[segment.name.clone()],
                                Some(SymbolKind::Enum),
                                None,
                            ) {
                                self.push_symbol_unique(symbols, sym);
                            }
                        }
                        CallKind::Function => {
                            let symbol = self
                                .lookup_symbol(
                                    &[segment.name.clone()],
                                    Some(SymbolKind::Function),
                                    None,
                                )
                                .or_else(|| {
                                    self.lookup_symbol_fqn(
                                        &[segment.name.clone()],
                                        SymbolKind::Function,
                                    )
                                });
                            if let Some(sym) = symbol {
                                self.push_symbol_unique(symbols, sym);
                            }
                        }
                        CallKind::Macro => {
                            let symbol = self
                                .lookup_symbol(
                                    &[segment.name.clone()],
                                    Some(SymbolKind::Macro),
                                    None,
                                )
                                .or_else(|| {
                                    self.lookup_symbol_fqn(
                                        &[segment.name.clone()],
                                        SymbolKind::Macro,
                                    )
                                });
                            if let Some(sym) = symbol {
                                self.push_symbol_unique(symbols, sym);
                            }
                        }
                        CallKind::Method => {
                            let symbol = self
                                .lookup_symbol(
                                    &[segment.name.clone()],
                                    Some(SymbolKind::Function),
                                    None,
                                )
                                .or_else(|| {
                                    self.lookup_symbol_fqn(
                                        &[segment.name.clone()],
                                        SymbolKind::Function,
                                    )
                                });
                            if let Some(sym) = symbol {
                                self.push_symbol_unique(symbols, sym);
                            }
                        }
                        CallKind::Unknown => {}
                    }
                }
            }
            CallTarget::Dynamic { .. } => {}
        }
    }

    fn push_symbol_unique(&self, symbols: &mut Vec<&'tcx Symbol>, symbol: &'tcx Symbol) {
        if symbols.iter().any(|existing| ptr::eq(*existing, symbol)) {
            return;
        }
        symbols.push(symbol);
    }

    fn push_type_from_qualifiers(&self, symbols: &mut Vec<&'tcx Symbol>, qualifiers: &[String]) {
        if qualifiers.is_empty() {
            return;
        }

        if let Some(sym) = self.lookup_symbol(qualifiers, Some(SymbolKind::Struct), None) {
            self.push_symbol_unique(symbols, sym);
        }

        if let Some(sym) = self.lookup_symbol(qualifiers, Some(SymbolKind::Enum), None) {
            self.push_symbol_unique(symbols, sym);
        }

        if let Some(sym) = self.lookup_symbol(qualifiers, Some(SymbolKind::Trait), None) {
            self.push_symbol_unique(symbols, sym);
        }
    }

    fn lookup_simple_path(&self, expr: &str) -> Option<&'tcx Symbol> {
        if expr.is_empty()
            || expr.contains('(')
            || expr.contains(')')
            || expr.contains('.')
            || expr.contains(' ')
            || matches!(expr, "self" | "Self" | "super")
        {
            return None;
        }

        if !expr.contains("::") {
            return None;
        }

        let parts: Vec<String> = expr
            .split("::")
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_string())
            .collect();

        if parts.is_empty() {
            return None;
        }

        self.lookup_symbol(&parts, None, None)
    }
}
