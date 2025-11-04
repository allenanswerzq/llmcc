use std::ptr;

use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternedStr;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};
use llmcc_core::Node;
use llmcc_descriptor::{CallKind, CallTarget, TypeExpr};

#[derive(Debug)]
pub struct BinderCore<'tcx, 'a, C> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    collection: &'a C,
}

impl<'tcx, 'a, C> BinderCore<'tcx, 'a, C> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>, collection: &'a C) -> Self {
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
    pub fn collection(&self) -> &'a C {
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

    pub fn lookup_symbol_suffix(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        file_hint: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        let file_index = file_hint.unwrap_or(self.unit.index);

        self.lookup_in_local_scopes(suffix, kind, Some(file_index))
            .or_else(|| self.lookup_in_local_scopes(suffix, kind, None))
            .or_else(|| self.lookup_in_global_scope(suffix, kind, Some(file_index)))
            .or_else(|| self.lookup_in_global_scope(suffix, kind, None))
    }

    pub fn add_symbol_dependency(&self, target: Option<&'tcx Symbol>) {
        if let (Some(current), Some(target)) = (self.current_symbol(), target) {
            current.add_dependency(target);
        }
    }

    pub fn add_symbol_dependency_by_field(
        &self,
        node: &HirNode<'tcx>,
        field_id: u16,
        expected: SymbolKind,
    ) -> Option<&'tcx Symbol> {
        let symbol = self.find_symbol_from_field(node, field_id, expected);
        self.add_symbol_dependency(symbol);
        symbol
    }

    pub fn lookup_segments(
        &self,
        segments: &[String],
        kind: Option<SymbolKind>,
        file_hint: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        if segments.is_empty() {
            return None;
        }

        let suffix: Vec<_> = segments
            .iter()
            .rev()
            .map(|segment| self.interner().intern(segment))
            .collect();

        self.lookup_symbol_suffix(&suffix, kind, file_hint)
    }

    pub fn lookup_segments_with_priority(
        &self,
        segments: &[String],
        preferred_kinds: &[SymbolKind],
        file_hint: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        if segments.is_empty() {
            return None;
        }

        for &kind in preferred_kinds {
            if let Some(symbol) = self.lookup_segments(segments, Some(kind), file_hint) {
                return Some(symbol);
            }
        }

        self.lookup_segments(segments, None, file_hint)
    }

    pub fn lookup_method(&self, method: &str) -> Option<&'tcx Symbol> {
        let direct_segments = vec![method.to_string()];
        if let Some(symbol) =
            self.lookup_segments_with_priority(&direct_segments, &[SymbolKind::Function], None)
        {
            return Some(symbol);
        }

        let owner_fqns: Vec<String> = self
            .scopes()
            .iter()
            .rev()
            .filter_map(|scope| scope.symbol().map(|symbol| symbol.fqn_name.read().clone()))
            .collect();

        for fqn in owner_fqns {
            if fqn.is_empty() {
                continue;
            }

            let mut segments: Vec<String> = fqn
                .split("::")
                .filter(|segment| !segment.is_empty())
                .map(|segment| segment.to_string())
                .collect();
            segments.push(method.to_string());

            if let Some(symbol) =
                self.lookup_segments_with_priority(&segments, &[SymbolKind::Function], None)
            {
                return Some(symbol);
            }
        }

        None
    }

    pub fn resolve_path_text(&self, text: &str) -> Option<&'tcx Symbol> {
        if text.is_empty() {
            return None;
        }

        let cleaned = text.split('<').next().unwrap_or(text);
        let segments: Vec<String> = cleaned
            .split("::")
            .map(|segment| segment.trim().to_string())
            .filter(|segment| !segment.is_empty())
            .collect();

        self.lookup_segments_with_priority(&segments, &[], None)
    }

    pub fn add_type_dependency_for_segments(
        &self,
        segments: &[String],
        preferred_kinds: &[SymbolKind],
    ) {
        if segments.len() <= 1 {
            return;
        }

        let base_segments = &segments[..segments.len() - 1];
        if let Some(symbol) =
            self.lookup_segments_with_priority(base_segments, preferred_kinds, None)
        {
            self.add_symbol_dependency(Some(symbol));
        }
    }

    pub fn add_call_target_dependencies(&self, target: &CallTarget) {
        match target {
            CallTarget::Symbol(symbol) => {
                let mut segments = symbol.qualifiers.clone();
                segments.push(symbol.name.clone());

                match symbol.kind {
                    CallKind::Method => {
                        if let Some(method_symbol) = self.lookup_method(&symbol.name) {
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

    pub fn resolve_type_expr_symbol(
        &self,
        expr: &TypeExpr,
        preferred_kinds: &[SymbolKind],
    ) -> Option<&'tcx Symbol> {
        let segments = expr.path_segments()?.to_vec();
        self.lookup_segments_with_priority(&segments, preferred_kinds, None)
    }

    pub fn resolve_symbol_type_expr_with<F>(
        &self,
        node: &HirNode<'tcx>,
        builder: F,
        preferred_kinds: &[SymbolKind],
    ) -> Option<&'tcx Symbol>
    where
        F: Fn(CompileUnit<'tcx>, Node<'tcx>) -> TypeExpr,
    {
        let expr = builder(self.unit(), node.inner_ts_node());
        self.resolve_type_expr_symbol(&expr, preferred_kinds)
    }

    pub fn collect_type_expr_symbols(
        &self,
        expr: &TypeExpr,
        preferred_kinds: &[SymbolKind],
        symbols: &mut Vec<&'tcx Symbol>,
    ) {
        match expr {
            TypeExpr::Path { segments, generics } => {
                if let Some(symbol) =
                    self.lookup_segments_with_priority(segments, preferred_kinds, None)
                {
                    if !symbols.iter().any(|existing| existing.id == symbol.id) {
                        symbols.push(symbol);
                    }
                }

                for generic in generics {
                    self.collect_type_expr_symbols(generic, preferred_kinds, symbols);
                }
            }
            TypeExpr::Reference { inner, .. } => {
                self.collect_type_expr_symbols(inner, preferred_kinds, symbols);
            }
            TypeExpr::Tuple(items) => {
                for item in items {
                    self.collect_type_expr_symbols(item, preferred_kinds, symbols);
                }
            }
            TypeExpr::Callable { parameters, result } => {
                for parameter in parameters {
                    self.collect_type_expr_symbols(parameter, preferred_kinds, symbols);
                }
                if let Some(result) = result.as_deref() {
                    self.collect_type_expr_symbols(result, preferred_kinds, symbols);
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

    pub fn find_symbol_from_field(
        &self,
        node: &HirNode<'tcx>,
        field_id: u16,
        expected: SymbolKind,
    ) -> Option<&'tcx Symbol> {
        let child = node.opt_child_by_field(self.unit(), field_id)?;
        let ident = child.as_ident()?;
        let key = self.interner().intern(&ident.name);
        self.lookup_symbol_suffix(&[key], Some(expected), None)
    }

    fn lookup_in_local_scopes(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        file: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        let global_scope = self.scopes.iter().next();
        for scope in self.scopes.iter().rev() {
            if let Some(global) = global_scope {
                if ptr::eq(scope, global) {
                    continue;
                }
            }

            let symbols = scope.lookup_suffix_symbols(suffix);
            if let Some(symbol) = self.select_matching_symbol(&symbols, kind, file) {
                return Some(symbol);
            }
        }

        None
    }

    fn lookup_in_global_scope(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        file: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        if let Some(global_scope) = self.scopes.iter().next() {
            let symbols = global_scope.lookup_suffix_symbols(suffix);
            self.select_matching_symbol(&symbols, kind, file)
        } else {
            None
        }
    }

    fn select_matching_symbol(
        &self,
        candidates: &[&'tcx Symbol],
        kind: Option<SymbolKind>,
        file: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        if candidates.is_empty() {
            return None;
        }

        for &symbol in candidates {
            let kind_ok = kind.is_none_or(|expected| symbol.kind() == expected);
            let file_ok = file.is_none_or(|expected| symbol.unit_index() == Some(expected));
            if kind_ok && file_ok {
                return Some(symbol);
            }
        }

        if let Some(expected_kind) = kind {
            for &symbol in candidates {
                if symbol.kind() == expected_kind {
                    return Some(symbol);
                }
            }
            return None;
        }

        if let Some(expected_file) = file {
            for &symbol in candidates {
                if symbol.unit_index() == Some(expected_file) {
                    return Some(symbol);
                }
            }
        }

        candidates.first().copied()
    }
}
