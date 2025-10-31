use std::path::Path;
use std::ptr;

use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternedStr;
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
    preferred_crate: Option<String>,
}

impl<'tcx> SymbolBinder<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner, &unit.cc.symbol_map);
        scopes.push(globals);
        let preferred_crate = unit
            .file_path()
            .or_else(|| unit.file().path())
            .and_then(Self::extract_crate_key);

        Self {
            unit,
            scopes,
            preferred_crate,
        }
    }

    fn interner(&self) -> &llmcc_core::interner::InternPool {
        self.unit.interner()
    }

    fn current_symbol(&self) -> Option<&'tcx Symbol> {
        self.scopes.scoped_symbol()
    }

    fn visit_children_scope(&mut self, node: HirNode<'tcx>, symbol: Option<&'tcx Symbol>) {
        let depth = self.scopes.depth();
        if let Some(symbol) = symbol {
            if let Some(parent) = self.scopes.scoped_symbol() {
                parent.add_dependency(symbol);
            }
        }

        // NOTE: scope should already be created during symbol collection, here we just
        // follow the tree structure again
        let scope = self.unit.opt_get_scope(node.hir_id());

        if let Some(scope) = scope {
            self.scopes.push_with_symbol(scope, symbol);
            self.visit_children(&node);
            self.scopes.pop_until(depth);
        } else {
            self.visit_children(&node);
        }
    }

    fn lookup_symbol_suffix(
        &mut self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
    ) -> Option<&'tcx Symbol> {
        let file_index = self.unit.index;
        self.lookup_in_local_scopes(suffix, kind, Some(file_index))
            .or_else(|| self.lookup_in_local_scopes(suffix, kind, None))
            .or_else(|| self.lookup_in_global_scope(suffix, kind, Some(file_index)))
            .or_else(|| self.lookup_in_global_scope(suffix, kind, None))
    }

    fn find_symbol_from_field(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
        expected: SymbolKind,
    ) -> Option<&'tcx Symbol> {
        let child = node.opt_child_by_field(self.unit, field_id)?;
        let ident = child.as_ident()?;
        let key = self.interner().intern(&ident.name);
        self.lookup_symbol_suffix(&[key], Some(expected))
    }

    fn add_symbol_relation(&mut self, symbol: Option<&'tcx Symbol>) {
        if let (Some(current), Some(target)) = (self.current_symbol(), symbol) {
            current.add_dependency(target);
        }
    }

    fn add_symbol_relation_by_field(&mut self, node: &HirNode<'tcx>, field_id: u16) {
        let symbol = self.find_symbol_from_field(node, field_id, SymbolKind::EnumVariant);
        self.add_symbol_relation(symbol);
    }

    fn resolve_impl_target(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let type_node = node.opt_child_by_field(self.unit, LangRust::field_type)?;
        let segments = self.type_segments(&type_node)?;
        self.resolve_symbol(&segments, None)
    }

    fn resolve_symbol(
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

        self.lookup_symbol_suffix(&suffix, kind)
    }

    fn resolve_method_symbol(&mut self, method: &str) -> Option<&'tcx Symbol> {
        if let Some(symbol) = self.resolve_symbol(&[method.to_string()], Some(SymbolKind::Function))
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
                    .map(|symbol| symbol.fqn_name.read().unwrap().clone())
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
            if let Some(symbol) = self.resolve_symbol(&segments, Some(SymbolKind::Function)) {
                return Some(symbol);
            }
        }

        None
    }

    fn resolve_type_symbol(&mut self, node: &HirNode<'tcx>) -> Option<&'tcx Symbol> {
        let segments = self.type_segments(node)?;
        self.resolve_symbol(&segments, Some(SymbolKind::Struct))
            .or_else(|| self.resolve_symbol(&segments, Some(SymbolKind::Enum)))
    }

    fn type_segments(&mut self, node: &HirNode<'tcx>) -> Option<Vec<String>> {
        let ts_node = node.inner_ts_node();
        let expr = parse_type_expr(self.unit, ts_node);
        extract_path_segments(&expr)
    }

    fn parse_type_expr_from_node(&self, node: &HirNode<'tcx>) -> TypeExpr {
        let ts_node = node.inner_ts_node();
        parse_type_expr(self.unit, ts_node)
    }

    fn collect_type_expr_symbols(&mut self, expr: &TypeExpr, symbols: &mut Vec<&'tcx Symbol>) {
        match expr {
            TypeExpr::Path { segments, generics } => {
                let struct_symbol = self.resolve_symbol(segments, Some(SymbolKind::Struct));
                let enum_symbol = if struct_symbol.is_none() {
                    self.resolve_symbol(segments, Some(SymbolKind::Enum))
                } else {
                    None
                };
                let any_symbol = if struct_symbol.is_none() && enum_symbol.is_none() {
                    self.resolve_symbol(segments, None)
                } else {
                    None
                };

                let symbol = struct_symbol.or(enum_symbol).or(any_symbol);
                if let Some(symbol) = symbol {
                    if !symbols.iter().any(|existing| existing.id == symbol.id) {
                        symbols.push(symbol);
                    }
                }
                for generic in generics {
                    self.collect_type_expr_symbols(generic, symbols);
                }
            }
            TypeExpr::Reference { inner, .. } => {
                self.collect_type_expr_symbols(inner, symbols);
            }
            TypeExpr::Tuple(items) => {
                for item in items {
                    self.collect_type_expr_symbols(item, symbols);
                }
            }
            TypeExpr::ImplTrait { .. } | TypeExpr::Unknown(_) => {}
        }
    }

    fn type_symbols_from_node(&mut self, node: &HirNode<'tcx>) -> Vec<&'tcx Symbol> {
        let expr = self.parse_type_expr_from_node(node);
        let mut symbols = Vec::new();
        self.collect_type_expr_symbols(&expr, &mut symbols);
        symbols
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

        if let Some(kind) = kind {
            if let Some(file) = file {
                if let Some(symbol) = candidates
                    .iter()
                    .copied()
                    .find(|symbol| symbol.kind() == kind && symbol.unit_index() == Some(file))
                {
                    return Some(symbol);
                }
            }

            if let Some(preferred) = self.preferred_crate.as_ref() {
                if let Some(symbol) = candidates
                    .iter()
                    .copied()
                    .find(|symbol| symbol.kind() == kind && self.symbol_in_crate(symbol, preferred))
                {
                    return Some(symbol);
                }
            }

            if let Some(symbol) = candidates
                .iter()
                .copied()
                .find(|symbol| symbol.kind() == kind)
            {
                return Some(symbol);
            }

            // if candidates.iter().all(|symbol| symbol.kind() != kind) {
            //     let details: Vec<_> = candidates
            //         .iter()
            //         .map(|symbol| {
            //             (
            //                 symbol.name.as_str().to_string(),
            //                 symbol.kind(),
            //                 symbol.unit_index(),
            //             )
            //         })
            //         .collect();
            // }

            return None;
        }

        if let Some(file) = file {
            if let Some(symbol) = candidates
                .iter()
                .copied()
                .find(|symbol| symbol.unit_index() == Some(file))
            {
                return Some(symbol);
            }
        }

        if let Some(preferred) = self.preferred_crate.as_ref() {
            if let Some(symbol) = candidates
                .iter()
                .copied()
                .find(|symbol| self.symbol_in_crate(symbol, preferred))
            {
                return Some(symbol);
            }
        }

        candidates.first().copied()
    }

    fn symbol_in_crate(&self, symbol: &'tcx Symbol, crate_key: &str) -> bool {
        let Some(unit_index) = symbol.unit_index() else {
            return false;
        };

        let path = self.unit.cc.file_path(unit_index);
        let Some(path) = path else {
            return false;
        };

        Self::extract_crate_key(path).as_deref() == Some(crate_key)
    }

    fn extract_crate_key(path: &str) -> Option<String> {
        let p = Path::new(path);
        for (idx, component) in p.components().enumerate() {
            if component.as_os_str() == "crates" {
                return p
                    .components()
                    .nth(idx + 1)
                    .map(|c| c.as_os_str().to_string_lossy().to_string());
            }
        }

        p.parent()
            .and_then(|parent| parent.file_name())
            .map(|name| name.to_string_lossy().to_string())
    }

    fn record_call_target(&mut self, target: &CallTarget) {
        match target {
            CallTarget::Path { segments, .. } => {
                if let Some(symbol) = self.resolve_symbol(segments, Some(SymbolKind::Function)) {
                    if symbol.kind() == SymbolKind::Function {
                        self.add_symbol_relation(Some(symbol));
                        self.add_type_dependency_for_segments(segments);
                    }
                } else if segments.len() > 1 {
                    self.add_type_dependency_for_segments(segments);
                }
            }
            CallTarget::Method { method, .. } => {
                if let Some(symbol) = self.resolve_method_symbol(method) {
                    self.add_symbol_relation(Some(symbol));
                }
            }
            CallTarget::Chain { base, segments } => {
                if let Some(symbol) = self.resolve_path_text(base) {
                    self.add_symbol_relation(Some(symbol));
                }
                for segment in segments {
                    if let Some(symbol) = self.resolve_method_symbol(&segment.method) {
                        self.add_symbol_relation(Some(symbol));
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
        self.resolve_symbol(&segments, None)
    }

    fn add_type_dependency_for_segments(&mut self, segments: &[String]) {
        if segments.len() <= 1 {
            return;
        }

        let base_segments = &segments[..segments.len() - 1];
        let struct_sym = self.resolve_symbol(base_segments, Some(SymbolKind::Struct));
        let enum_sym = if struct_sym.is_none() {
            self.resolve_symbol(base_segments, Some(SymbolKind::Enum))
        } else {
            None
        };

        if let Some(sym) = struct_sym.or(enum_sym) {
            self.add_symbol_relation(Some(sym));
        }
    }

    fn propagate_child_dependencies(&mut self, parent: &'tcx Symbol, child: &'tcx Symbol) {
        let dependencies: Vec<_> = child.depends.read().unwrap().clone();
        for dep_id in dependencies {
            if dep_id == parent.id {
                continue;
            }

            if let Some(dep_symbol) = self.unit.opt_get_symbol(dep_id) {
                if dep_symbol.kind() == SymbolKind::Function {
                    continue;
                }

                if dep_symbol.depends.read().unwrap().contains(&parent.id) {
                    continue;
                }

                parent.add_dependency(dep_symbol);
            }
        }
    }
}

impl<'tcx> AstVisitorRust<'tcx> for SymbolBinder<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(node, None);
    }

    fn visit_mod_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Module);
        self.visit_children_scope(node, symbol);
    }

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Struct);
        self.visit_children_scope(node, symbol);
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Enum);
        self.visit_children_scope(node, symbol);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Function);
        let parent_symbol = self.current_symbol();

        // Also extract return type as a dependency
        if let Some(func_symbol) = symbol {
            if let Some(return_type_node) =
                node.opt_child_by_field(self.unit, LangRust::field_return_type)
            {
                let expr = self.parse_type_expr_from_node(&return_type_node);
                let mut symbols = Vec::new();
                self.collect_type_expr_symbols(&expr, &mut symbols);
                for return_type_sym in symbols {
                    func_symbol.add_dependency(return_type_sym);
                }
            }

            // If this function is inside an impl block, it depends on the impl's target struct/enum
            // The current_symbol() when visiting impl children is the target struct/enum
            if let Some(parent_symbol) = parent_symbol {
                let kind = parent_symbol.kind();
                if matches!(kind, SymbolKind::Struct | SymbolKind::Enum) {
                    func_symbol.add_dependency(parent_symbol);
                }
            }
        }

        self.visit_children_scope(node, symbol);

        if let (Some(parent_symbol), Some(func_symbol)) = (parent_symbol, symbol) {
            if matches!(
                parent_symbol.kind(),
                SymbolKind::Struct | SymbolKind::Enum | SymbolKind::Impl
            ) {
                self.propagate_child_dependencies(parent_symbol, func_symbol);
            }
        }
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.resolve_impl_target(&node);

        // If this is a trait impl (impl Trait for Type), make the trait depend on the type
        let trait_node = node.inner_ts_node();
        if let Some(trait_ts) = trait_node.child_by_field_name("trait") {
            let trait_segments = extract_segments_from_ts_node(self.unit, trait_ts);
            if let Some(trait_symbol) = self.resolve_symbol(&trait_segments, None) {
                if let Some(target_symbol) = symbol {
                    trait_symbol.add_dependency(target_symbol);
                }
            }
        }

        self.visit_children_scope(node, symbol);
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Trait);
        self.visit_children_scope(node, symbol);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(node, None);
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        // Extract the type annotation if present and add it as a dependency
        if let Some(type_node) = node.opt_child_by_field(self.unit, LangRust::field_type) {
            for type_symbol in self.type_symbols_from_node(&type_node) {
                self.add_symbol_relation(Some(type_symbol));
            }
        }

        // Visit children to handle method calls and other expressions in the initializer
        self.visit_children(&node);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_const_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Const);
        self.visit_children_scope(node, symbol);
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self.find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Static);
        self.visit_children_scope(node, symbol);
    }

    fn visit_enum_variant(&mut self, node: HirNode<'tcx>) {
        self.add_symbol_relation_by_field(&node, LangRust::field_name);
        self.visit_children(&node);
    }

    fn visit_field_declaration(&mut self, node: HirNode<'tcx>) {
        // Extract the type annotation from the field and add it as a dependency
        if let Some(type_node) = node.opt_child_by_field(self.unit, LangRust::field_type) {
            for type_symbol in self.type_symbols_from_node(&type_node) {
                self.add_symbol_relation(Some(type_symbol));
            }
        }
        self.visit_children(&node);
    }

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        let enclosing = self
            .current_symbol()
            .map(|symbol| symbol.fqn_name.read().unwrap().clone());
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
        // Try to resolve as a function first (for calls), then as a struct (for types), then anything else
        let target = self
            .resolve_symbol(&segments, Some(SymbolKind::Function))
            .or_else(|| self.resolve_symbol(&segments, Some(SymbolKind::Struct)))
            .or_else(|| self.resolve_symbol(&segments, Some(SymbolKind::Enum)));
        self.add_symbol_relation(target);
        self.visit_children(&node);
    }

    fn visit_type_identifier(&mut self, node: HirNode<'tcx>) {
        if self.identifier_is_call_function(&node) {
            self.visit_children(&node);
            return;
        }
        if let Some(symbol) = self.resolve_type_symbol(&node) {
            // Skip struct/enum dependencies if this identifier is followed by parentheses (bare constructor call).
            // This is detected by checking if the parent is a call_expression.
            if matches!(symbol.kind(), SymbolKind::Struct | SymbolKind::Enum) {
                // Check parent - if it's a call_expression, this might be a bare constructor call
                if let Some(parent_id) = node.parent() {
                    let parent = self.unit.hir_node(parent_id);
                    if parent.kind_id() == LangRust::call_expression {
                        // Parent is a call expression - skip adding this dependency
                        // record_call_target will handle it
                        self.visit_children(&node);
                        return;
                    }
                }
            }
            self.add_symbol_relation(Some(symbol));
        }
        self.visit_children(&node);
    }

    fn visit_identifier(&mut self, node: HirNode<'tcx>) {
        let Some(ident) = node.as_ident() else {
            self.visit_children(&node);
            return;
        };

        let key = self.interner().intern(&ident.name);

        let symbol = if let Some(local) = self.scopes.find_symbol_local(&ident.name) {
            Some(local)
        } else {
            self.scopes
                .find_scoped_suffix_with_filters(&[key], None, Some(self.unit.index))
                .or_else(|| {
                    self.scopes
                        .find_scoped_suffix_with_filters(&[key], None, None)
                })
                .or_else(|| {
                    self.scopes
                        .find_global_suffix_with_filters(&[key], None, Some(self.unit.index))
                })
                .or_else(|| {
                    self.scopes
                        .find_global_suffix_with_filters(&[key], None, None)
                })
        };

        // Don't add struct/enum identifiers through visit_identifier. They should be added through
        // visit_type_identifier (for type annotations) instead, or through special handling in contexts
        // like let declarations.
        if let Some(sym) = symbol {
            if !matches!(sym.kind(), SymbolKind::Struct | SymbolKind::Enum) {
                self.add_symbol_relation(Some(sym));
            }
        }
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

fn extract_segments_from_ts_node<'tcx>(
    unit: CompileUnit<'tcx>,
    node: tree_sitter::Node<'tcx>,
) -> Vec<String> {
    let text = unit.file().get_text(node.start_byte(), node.end_byte());
    text.split("::")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn node_text_simple<'tcx>(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> String {
    unit.file().get_text(node.start_byte(), node.end_byte())
}
