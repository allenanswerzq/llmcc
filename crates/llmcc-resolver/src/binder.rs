use std::ptr;

use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternedStr;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{self, Scope, ScopeStack, Symbol, SymbolKind};
use llmcc_core::Node;
use llmcc_descriptor::{CallKind, CallTarget, TypeExpr};

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

    fn lookup_in_locals(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        unit_index: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        for scope in self.scopes[1..].iter().rev() {
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

    pub fn lookup_symbol_suffix(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        unit_index: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        self.lookup_in_locals(suffix, kind, unit_index)
            .or_else(|| self.lookup_in_globals(suffix, kind, unit_index))
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
        self.core
            .lookup_expr_symbols_with(expr, SymbolKind::Struct, &mut symbols);
        self.core
            .lookup_expr_symbols_with(expr, SymbolKind::Enum, &mut symbols);
        symbols
    }

    fn lookup_expr_symbols_with(
        &self,
        expr: &TypeExpr,
        kind: SymbolKind,
        symbols: &mut Vec<&'tcx Symbol>,
    ) {
        match expr {
            TypeExpr::Path { segments, generics } => {
                if let Some(symbol) = self.lookup_symbol(segments, kind, None) {
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

    pub fn add_call_dependencies(&self, target: &CallTarget) {
        match target {
            CallTarget::Symbol(symbol) => {
                let mut segments = symbol.qualifiers.clone();
                segments.push(symbol.name.clone());

                match symbol.kind {
                    CallKind::Method => {
                        if let Some(method_symbol) =
                            self.lookup_symbol(&segments, Some(SymbolKind::Function), None)
                        {
                            self.add_symbol_dependency(Some(method_symbol));
                        }
                    }
                    CallKind::Constructor => {
                        if let Some(struct_symbol) =
                            self.lookup_segments(&segments, Some(SymbolKind::Struct), None)
                        {
                            self.add_symbol_dependency(Some(struct_symbol));
                        } else if let Some(enum_symbol) =
                            self.lookup_segments(&segments, Some(SymbolKind::Enum), None)
                        {
                            self.add_symbol_dependency(Some(enum_symbol));
                        }
                    }
                    CallKind::Function | CallKind::Macro | CallKind::Unknown => {
                        if let Some(function_symbol) =
                            self.lookup_segments(&segments, Some(SymbolKind::Function), None)
                        {
                            self.add_symbol_dependency(Some(function_symbol));
                            if segments.len() > 1 {
                                self.add_type_dependency_for_segments(
                                    &segments,
                                    &[SymbolKind::Struct, SymbolKind::Enum],
                                );
                            }
                        } else if !segments.is_empty() {
                            self.add_type_dependency_for_segments(
                                &segments,
                                &[SymbolKind::Struct, SymbolKind::Enum],
                            );
                        }
                    }
                }
            }
            CallTarget::Chain(chain) => {
                if let Some(symbol) = self.resolve_path_text(&chain.root) {
                    self.add_symbol_dependency(Some(symbol));
                } else {
                    let segments: Vec<String> = chain
                        .root
                        .split("::")
                        .filter(|segment| !segment.is_empty())
                        .map(|segment| segment.trim().to_string())
                        .collect();
                    if segments.len() > 1 {
                        self.add_type_dependency_for_segments(
                            &segments,
                            &[SymbolKind::Struct, SymbolKind::Enum],
                        );
                    }
                }

                for segment in &chain.segments {
                    match segment.kind {
                        CallKind::Method | CallKind::Function => {
                            if let Some(symbol) = self.lookup_method(&segment.name) {
                                self.add_symbol_dependency(Some(symbol));
                            }
                        }
                        CallKind::Constructor => {
                            if let Some(symbol) = self.lookup_segments(
                                std::slice::from_ref(&segment.name),
                                Some(SymbolKind::Struct),
                                None,
                            ) {
                                self.add_symbol_dependency(Some(symbol));
                            }
                        }
                        CallKind::Macro | CallKind::Unknown => {}
                    }
                }
            }
            CallTarget::Dynamic { .. } => {}
        }
    }
}
