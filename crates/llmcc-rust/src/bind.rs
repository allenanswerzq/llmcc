use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};

use crate::descriptor::function::parse_type_expr;
use crate::descriptor::{CallDescriptor, CallTarget, TypeExpr};
use crate::token::{AstVisitorRust, LangRust};

/// `SymbolBinder` connects symbols with the items they reference so that later
/// stages (or LLM consumers) can reason about dependency relationships.
#[derive(Debug)]
struct SymbolBinder<'tcx> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
}

impl<'tcx> SymbolBinder<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner);
        scopes.push(globals);
        Self { unit, scopes }
    }

    fn interner(&self) -> &llmcc_core::interner::InternPool {
        self.unit.interner()
    }

    fn current_symbol(&self) -> Option<&'tcx Symbol> {
        self.scopes.scoped_symbol()
    }

    fn push_scope_with_symbol(&mut self, node: HirNode<'tcx>, symbol: Option<&'tcx Symbol>) {
        let depth = self.scopes.depth();
        let scope = self
            .unit
            .opt_get_scope(node.hir_id())
            .unwrap_or_else(|| self.unit.alloc_scope(node.hir_id()));

        if let Some(symbol) = symbol {
            if let Some(parent) = self.scopes.scoped_symbol() {
                parent.add_dependency(symbol);
            }
            scope.set_symbol(Some(symbol));
        }

        self.scopes.push_with_symbol(scope, symbol);
        self.visit_children(&node);
        self.scopes.pop_until(depth);

        if let Some(symbol) = symbol {
            scope.set_symbol(Some(symbol));
        }
    }

    fn symbol_from_field(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
        expected: SymbolKind,
    ) -> Option<&'tcx Symbol> {
        let child = node.opt_child_by_field(self.unit, field_id)?;
        let ident = child.as_ident()?;
        let key = self.interner().intern(&ident.name);
        self.scopes
            .lookup_scoped_suffix_with_filters(&[key], Some(expected), Some(self.unit.index))
            .or_else(|| {
                self.scopes
                    .lookup_scoped_suffix_with_filters(&[key], Some(expected), None)
            })
            .or_else(|| {
                self.scopes.find_global_suffix_once_with_filters(
                    &[key],
                    Some(expected),
                    Some(self.unit.index),
                )
            })
            .or_else(|| {
                self.scopes
                    .find_global_suffix_once_with_filters(&[key], Some(expected), None)
            })
            .or_else(|| {
                self.scopes.find_symbol_local(&ident.name).map(|symbol| {
                    symbol.set_kind(expected);
                    symbol
                })
            })
    }

    fn record_symbol_dependency(&mut self, symbol: Option<&'tcx Symbol>) {
        if let (Some(current), Some(target)) = (self.current_symbol(), symbol) {
            current.add_dependency(target);
        }
    }

    fn record_symbol_dependency_by_field(&mut self, node: &HirNode<'tcx>, field_id: u16) {
        let symbol = self.symbol_from_field(node, field_id, SymbolKind::EnumVariant);
        self.record_symbol_dependency(symbol);
    }

    fn resolve_impl_target(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let type_node = node.opt_child_by_field(self.unit, LangRust::field_type)?;
        let segments = self.type_segments(&type_node)?;
        self.resolve_symbol_by_segments(&segments, None)
    }

    fn resolve_symbol_by_segments(
        &mut self,
        segments: &[String],
        kind: Option<SymbolKind>,
    ) -> Option<&'tcx Symbol> {
        if segments.is_empty() {
            return None;
        }

        let suffix: Vec<_> = segments
            .iter()
            .rev()
            .map(|segment| self.interner().intern(segment))
            .collect();

        let file_index = self.unit.index;

        self.scopes
            .lookup_scoped_suffix_with_filters(&suffix, kind, Some(file_index))
            .or_else(|| {
                self.scopes
                    .lookup_scoped_suffix_with_filters(&suffix, kind, None)
            })
            .or_else(|| {
                self.scopes
                    .find_global_suffix_once_with_filters(&suffix, kind, Some(file_index))
            })
            .or_else(|| {
                self.scopes
                    .find_global_suffix_once_with_filters(&suffix, kind, None)
            })
            .or_else(|| {
                if kind.is_some() {
                    self.scopes
                        .lookup_scoped_suffix_with_filters(&suffix, None, Some(file_index))
                        .or_else(|| {
                            self.scopes
                                .lookup_scoped_suffix_with_filters(&suffix, None, None)
                        })
                        .or_else(|| {
                            self.scopes.find_global_suffix_once_with_filters(
                                &suffix,
                                None,
                                Some(file_index),
                            )
                        })
                        .or_else(|| {
                            self.scopes
                                .find_global_suffix_once_with_filters(&suffix, None, None)
                        })
                } else {
                    None
                }
            })
    }

    fn resolve_method_symbol(&mut self, method: &str) -> Option<&'tcx Symbol> {
        if let Some(symbol) =
            self.resolve_symbol_by_segments(&[method.to_string()], Some(SymbolKind::Function))
        {
            return Some(symbol);
        }

        let owner_fqns: Vec<String> = self
            .scopes
            .iter()
            .rev()
            .filter_map(|scope| {
                scope
                    .symbol()
                    .map(|symbol| symbol.fqn_name.borrow().clone())
            })
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
                self.resolve_symbol_by_segments(&segments, Some(SymbolKind::Function))
            {
                return Some(symbol);
            }
        }

        None
    }

    fn resolve_type_symbol(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let segments = self.type_segments(node)?;
        self.resolve_symbol_by_segments(&segments, Some(SymbolKind::Struct))
            .or_else(|| self.resolve_symbol_by_segments(&segments, Some(SymbolKind::Enum)))
            .or_else(|| self.resolve_symbol_by_segments(&segments, None))
    }

    fn type_segments(&mut self, node: &HirNode<'tcx>) -> Option<Vec<String>> {
        let ts_node = node.inner_ts_node();
        let expr = parse_type_expr(self.unit, ts_node);
        extract_path_segments(&expr)
    }

    fn record_call_target(&mut self, target: &CallTarget) {
        match target {
            CallTarget::Path { segments, .. } => {
                if let Some(symbol) = self
                    .resolve_symbol_by_segments(segments, Some(SymbolKind::Function))
                    .or_else(|| self.resolve_symbol_by_segments(segments, None))
                {
                    self.record_symbol_dependency(Some(symbol));
                }
            }
            CallTarget::Method { method, .. } => {
                if let Some(symbol) = self.resolve_method_symbol(method) {
                    self.record_symbol_dependency(Some(symbol));
                }
            }
            CallTarget::Chain { base, segments } => {
                if let Some(symbol) = self.resolve_path_text(base) {
                    self.record_symbol_dependency(Some(symbol));
                }
                for segment in segments {
                    if let Some(symbol) = self.resolve_method_symbol(&segment.method) {
                        self.record_symbol_dependency(Some(symbol));
                    }
                }
            }
            CallTarget::Unknown(_) => {}
        }
    }

    fn resolve_path_text(&mut self, text: &str) -> Option<&'tcx Symbol> {
        if text.is_empty() {
            return None;
        }
        let cleaned = text.split('<').next().unwrap_or(text);
        let segments: Vec<String> = cleaned
            .split("::")
            .map(|segment| segment.trim().to_string())
            .filter(|segment| !segment.is_empty())
            .collect();
        self.resolve_symbol_by_segments(&segments, None)
    }
}

impl<'tcx> AstVisitorRust<'tcx> for SymbolBinder<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.push_scope_with_symbol(node, None);
    }

    fn visit_mod_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.symbol_from_field(&node, LangRust::field_name, SymbolKind::Module);
        self.push_scope_with_symbol(node, symbol);
    }

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.symbol_from_field(&node, LangRust::field_name, SymbolKind::Struct);
        self.push_scope_with_symbol(node, symbol);
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.symbol_from_field(&node, LangRust::field_name, SymbolKind::Enum);
        self.push_scope_with_symbol(node, symbol);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.symbol_from_field(&node, LangRust::field_name, SymbolKind::Function);
        self.push_scope_with_symbol(node, symbol);
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.resolve_impl_target(&node);
        self.push_scope_with_symbol(node, symbol);
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        self.push_scope_with_symbol(node, None);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.push_scope_with_symbol(node, None);
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_const_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.symbol_from_field(&node, LangRust::field_name, SymbolKind::Const);
        self.push_scope_with_symbol(node, symbol);
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.symbol_from_field(&node, LangRust::field_name, SymbolKind::Static);
        self.push_scope_with_symbol(node, symbol);
    }

    fn visit_enum_variant(&mut self, node: HirNode<'tcx>) {
        self.record_symbol_dependency_by_field(&node, LangRust::field_name);
        self.visit_children(&node);
    }

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        let enclosing = self
            .current_symbol()
            .map(|symbol| symbol.fqn_name.borrow().clone());
        let descriptor = CallDescriptor::from_call(self.unit, &node, enclosing);
        self.record_call_target(&descriptor.target);
        self.visit_children(&node);
    }

    fn visit_scoped_identifier(&mut self, node: HirNode<'tcx>) {
        if self.identifier_is_call_function(&node) {
            self.visit_children(&node);
            return;
        }
        let text = node_text_simple(self.unit, &node);
        let segments: Vec<String> = text
            .split("::")
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.trim().to_string())
            .collect();
        let target = self.resolve_symbol_by_segments(&segments, None);
        self.record_symbol_dependency(target);
        self.visit_children(&node);
    }

    fn visit_type_identifier(&mut self, node: HirNode<'tcx>) {
        if let Some(symbol) = self.resolve_type_symbol(&node) {
            self.record_symbol_dependency(Some(symbol));
        }
        self.visit_children(&node);
    }

    fn visit_identifier(&mut self, node: HirNode<'tcx>) {
        let Some(ident) = node.as_ident() else {
            self.visit_children(&node);
            return;
        };

        if self.identifier_is_call_function(&node) {
            self.visit_children(&node);
            return;
        }

        let symbol = if let Some(local) = self.scopes.find_symbol_local(&ident.name) {
            Some(local)
        } else {
            let key = self.interner().intern(&ident.name);
            self.scopes
                .lookup_scoped_suffix_with_filters(&[key], None, Some(self.unit.index))
                .or_else(|| {
                    self.scopes
                        .lookup_scoped_suffix_with_filters(&[key], None, None)
                })
                .or_else(|| {
                    self.scopes.find_global_suffix_once_with_filters(
                        &[key],
                        None,
                        Some(self.unit.index),
                    )
                })
                .or_else(|| {
                    self.scopes
                        .find_global_suffix_once_with_filters(&[key], None, None)
                })
        };
        self.record_symbol_dependency(symbol);
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

impl<'tcx> SymbolBinder<'tcx> {
    fn identifier_is_call_function(&self, node: &HirNode<'tcx>) -> bool {
        let Some(parent_id) = node.parent() else {
            return false;
        };
        let parent = self.unit.hir_node(parent_id);
        if parent.kind_id() != LangRust::call_expression {
            return false;
        }
        let parent_ts = parent.inner_ts_node();
        let Some(function_ts) = parent_ts.child_by_field_name("function") else {
            return false;
        };
        let node_ts = node.inner_ts_node();
        function_ts.start_byte() == node_ts.start_byte()
            && function_ts.end_byte() == node_ts.end_byte()
    }
}

pub fn bind_symbols<'tcx>(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) {
    let root = unit.file_start_hir_id().unwrap();
    let node = unit.hir_node(root);
    let mut binder = SymbolBinder::new(unit, globals);
    binder.visit_node(node);
}

fn extract_path_segments(expr: &TypeExpr) -> Option<Vec<String>> {
    match expr {
        TypeExpr::Path { segments, .. } => Some(segments.clone()),
        TypeExpr::Reference { inner, .. } => extract_path_segments(inner),
        TypeExpr::Tuple(items) if items.len() == 1 => extract_path_segments(&items[0]),
        _ => None,
    }
}

fn node_text_simple<'tcx>(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> String {
    unit.file().get_text(node.start_byte(), node.end_byte())
}
