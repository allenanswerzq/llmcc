use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol};

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

    fn symbol_from_field(&mut self, node: &HirNode<'tcx>, field_id: u16) -> Option<&'tcx Symbol> {
        let child = node.opt_child_by_field(self.unit, field_id)?;
        let ident = child.as_ident()?;
        self.scopes.find_ident(ident)
    }

    fn record_symbol_dependency(&mut self, symbol: Option<&'tcx Symbol>) {
        if let (Some(current), Some(target)) = (self.current_symbol(), symbol) {
            current.add_dependency(target);
        }
    }

    fn record_symbol_dependency_by_field(&mut self, node: &HirNode<'tcx>, field_id: u16) {
        let symbol = self.symbol_from_field(node, field_id);
        self.record_symbol_dependency(symbol);
    }

    fn resolve_impl_target(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let type_node = node.opt_child_by_field(self.unit, LangRust::field_type)?;
        let segments = self.type_segments(&type_node)?;
        self.resolve_symbol_by_segments(&segments)
    }

    fn resolve_symbol_by_segments(&mut self, segments: &[String]) -> Option<&'tcx Symbol> {
        if segments.is_empty() {
            return None;
        }

        let suffix: Vec<_> = segments
            .iter()
            .rev()
            .map(|segment| self.interner().intern(segment))
            .collect();

        // Try global lookup first.
        if let Some(symbol) = self.scopes.find_global_suffix_once(&suffix) {
            return Some(symbol);
        }

        // Fallback: single-segment name may be available in the current stack.
        if segments.len() == 1 {
            if let Some(current_scope_symbol) = self.scopes.scoped_symbol() {
                if current_scope_symbol.name == segments[0] {
                    return Some(current_scope_symbol);
                }
            }
        }

        None
    }

    fn resolve_method_symbol(&mut self, method: &str) -> Option<&'tcx Symbol> {
        let interner = self.interner();
        let method_key = interner.intern(method);

        // Direct global lookup (e.g. associated function exposed publicly).
        if let Some(symbol) = self.scopes.find_global_suffix_once(&[method_key]) {
            return Some(symbol);
        }

        // Walk up the scope stack looking for an enclosing symbol (impl/trait/struct)
        // and try to resolve `enclosing::method`.
        for scope in self.scopes.iter().rev() {
            if let Some(owner_symbol) = scope.symbol() {
                let fqn = owner_symbol.fqn_name.borrow();
                if fqn.is_empty() {
                    continue;
                }
                let mut suffix = vec![method_key];
                let mut owner_segments: Vec<_> = fqn
                    .split("::")
                    .filter(|segment| !segment.is_empty())
                    .map(|segment| interner.intern(segment))
                    .collect();
                owner_segments.reverse();
                suffix.extend(owner_segments);

                if let Some(symbol) = scope.lookup_suffix_once(&suffix) {
                    return Some(symbol);
                }

                if let Some(symbol) = scope.lookup_suffix_once(&[method_key]) {
                    return Some(symbol);
                }

                if let Some(symbol) = self.scopes.find_global_suffix_once(&suffix) {
                    return Some(symbol);
                }
            }
        }

        None
    }

    fn resolve_type_symbol(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let segments = self.type_segments(node)?;
        self.resolve_symbol_by_segments(&segments)
    }

    fn type_segments(&mut self, node: &HirNode<'tcx>) -> Option<Vec<String>> {
        let ts_node = node.inner_ts_node();
        let expr = parse_type_expr(self.unit, ts_node);
        extract_path_segments(&expr)
    }

    fn record_call_target(&mut self, target: &CallTarget) {
        match target {
            CallTarget::Path { segments, .. } => {
                if let Some(symbol) = self.resolve_symbol_by_segments(segments) {
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
        self.resolve_symbol_by_segments(&segments)
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
        let symbol = self.symbol_from_field(&node, LangRust::field_name);
        self.push_scope_with_symbol(node, symbol);
    }

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.symbol_from_field(&node, LangRust::field_name);
        self.push_scope_with_symbol(node, symbol);
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.symbol_from_field(&node, LangRust::field_name);
        self.push_scope_with_symbol(node, symbol);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.symbol_from_field(&node, LangRust::field_name);
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
        let symbol = self.symbol_from_field(&node, LangRust::field_name);
        self.push_scope_with_symbol(node, symbol);
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.symbol_from_field(&node, LangRust::field_name);
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

        let symbol = self.scopes.find_ident(ident).or_else(|| {
            let key = self.interner().intern(&ident.name);
            self.scopes.find_global_suffix_once(&[key])
        });
        self.record_symbol_dependency(symbol);
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
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
