use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirNode, HirScope};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, SymKindSet, Symbol};
use llmcc_resolver::{CollectorScopes, ResolverOption};

use std::collections::HashMap;

use crate::LangPython;
use crate::token::AstVisitorPython;
use crate::util::{parse_module_name, parse_package_name};

/// Check if a function is in a method context (parent is a class).
fn is_method_context(parent: Option<&Symbol>) -> bool {
    parent.is_some_and(|p| matches!(p.kind(), SymKind::Struct))
}

/// Callback type for scope entry actions
type ScopeEntryCallback<'tcx> = Box<dyn FnOnce(&HirNode<'tcx>, &mut CollectorScopes<'tcx>) + 'tcx>;

#[derive(Debug)]
pub struct CollectorVisitor<'tcx> {
    scope_map: HashMap<ScopeId, &'tcx Scope<'tcx>>,
}

impl<'tcx> CollectorVisitor<'tcx> {
    fn new() -> Self {
        Self {
            scope_map: HashMap::new(),
        }
    }

    /// Find all identifiers in a pattern node (recursive)
    fn collect_pattern_identifiers(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
    ) -> Vec<&'tcx Symbol> {
        let mut symbols = Vec::new();
        Self::collect_pattern_identifiers_impl(unit, node, scopes, kind, &mut symbols);
        symbols
    }

    fn collect_pattern_identifiers_impl(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
        symbols: &mut Vec<&'tcx Symbol>,
    ) {
        // Skip attribute access - we only collect simple identifiers
        if node.kind_id() == LangPython::attribute {
            return;
        }

        if let Some(ident) = node.as_ident() {
            let name = ident.name.to_string();
            if let Some(sym) = scopes.lookup_or_insert(&name, node, kind) {
                ident.set_symbol(sym);
                symbols.push(sym);
            }
        }
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            Self::collect_pattern_identifiers_impl(unit, &child, scopes, kind, symbols);
        }
    }

    fn alloc_scope(&mut self, unit: &CompileUnit<'tcx>, symbol: &'tcx Symbol) -> &'tcx Scope<'tcx> {
        let scope = unit.cc.alloc_scope(symbol.owner());
        scope.set_symbol(symbol);
        self.scope_map.insert(scope.id(), scope);
        scope
    }

    fn get_scope(&self, scope_id: ScopeId) -> Option<&'tcx Scope<'tcx>> {
        self.scope_map.get(&scope_id).copied()
    }

    /// Lookup a symbol by name, trying primary kind first, then UnresolvedType, then inserting new
    fn lookup_or_convert(
        &mut self,
        unit: &CompileUnit<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        // Try looking up by primary kind
        if let Some(symbol) = scopes.lookup_symbol(name, SymKindSet::from_kind(kind)) {
            return Some(symbol);
        }

        // Try unresolved type if not found - convert to target kind
        if let Some(symbol) =
            scopes.lookup_symbol(name, SymKindSet::from_kind(SymKind::UnresolvedType))
        {
            symbol.set_kind(kind);
            return Some(symbol);
        }

        // Insert new symbol with primary kind
        if let Some(symbol) = scopes.lookup_or_insert(name, node, kind) {
            if symbol.opt_scope().is_none() {
                let scope = self.alloc_scope(unit, symbol);
                symbol.set_scope(scope.id());
            }
            return Some(symbol);
        }

        None
    }

    /// Visit node with a new scope
    #[allow(clippy::too_many_arguments)]
    fn visit_with_scope(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        sym: &'tcx Symbol,
        sn: &'tcx HirScope<'tcx>,
        ident: &'tcx HirIdent<'tcx>,
        on_scope_enter: Option<ScopeEntryCallback<'tcx>>,
    ) {
        ident.set_symbol(sym);
        sn.set_ident(ident);

        let scope = if sym.opt_scope().is_none() {
            self.alloc_scope(unit, sym)
        } else {
            match self.get_scope(sym.scope()) {
                Some(s) => s,
                None => self.alloc_scope(unit, sym),
            }
        };
        sym.set_scope(scope.id());
        sn.set_scope(scope);

        scopes.push_scope(scope);
        if let Some(callback) = on_scope_enter {
            callback(node, scopes);
        }
        self.visit_children(unit, node, scopes, scope, Some(sym));
        scopes.pop_scope();
    }
}

impl<'tcx> AstVisitorPython<'tcx, CollectorScopes<'tcx>> for CollectorVisitor<'tcx> {
    // AST: module (source file)
    fn visit_module(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let file_path = unit.file_path().unwrap_or("unknown");
        let start_depth = scopes.scope_depth();

        tracing::trace!("collecting module: {}", file_path);

        // Try to determine package/module structure
        if let Some(package_name) = parse_package_name(file_path) {
            if let Some(symbol) = self.lookup_or_convert(
                unit, scopes, &package_name, node, SymKind::Module,
            ) {
                symbol.set_is_global(true);
                scopes.globals().insert(symbol);
                if let Some(scope_id) = symbol.opt_scope()
                    && let Some(scope) = self.get_scope(scope_id)
                {
                    scopes.push_scope(scope);
                }
            }
        }

        if let Some(module_name) = parse_module_name(file_path) {
            if let Some(symbol) = self.lookup_or_convert(
                unit, scopes, &module_name, node, SymKind::Module,
            ) {
                symbol.set_is_global(true);
                scopes.globals().insert(symbol);
                if let Some(scope_id) = symbol.opt_scope()
                    && let Some(scope) = self.get_scope(scope_id)
                {
                    scopes.push_scope(scope);
                }
            }
        }

        // Create file-level scope
        let file_name = std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("_file");

        if let Some(file_sym) = self.lookup_or_convert(unit, scopes, file_name, node, SymKind::File)
        {
            file_sym.set_is_global(true);
            scopes.globals().insert(file_sym);

            // Set the ident on the module's scope node so the block graph can get the name
            if let Some(sn) = node.as_scope() {
                let ident = unit.cc.alloc_file_ident(node.id(), file_name, file_sym);
                sn.set_ident(ident);
            }

            if let Some(scope_id) = file_sym.opt_scope()
                && let Some(scope) = self.get_scope(scope_id)
            {
                scopes.push_scope(scope);
                self.visit_children(unit, node, scopes, scope, Some(file_sym));
            }
        }

        scopes.pop_until(start_depth);
    }

    // AST: function_definition
    fn visit_function_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        let Some(ident) = node.ident_by_field(unit, LangPython::field_name) else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        let kind = if is_method_context(parent) {
            SymKind::Method
        } else {
            SymKind::Function
        };

        let Some(sym) = scopes.lookup_or_insert(&ident.name, node, kind) else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        // Check for dunder methods -> treat as public
        let is_dunder = ident.name.starts_with("__") && ident.name.ends_with("__");
        if !ident.name.starts_with('_') || is_dunder {
            sym.set_is_global(true);
            scopes.globals().insert(sym);
        }

        self.visit_with_scope(unit, node, scopes, sym, sn, ident, None);
    }

    // AST: class_definition
    fn visit_class_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        let Some(ident) = node.ident_by_field(unit, LangPython::field_name) else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        let Some(sym) = scopes.lookup_or_insert(&ident.name, node, SymKind::Struct) else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        if !ident.name.starts_with('_') {
            sym.set_is_global(true);
            scopes.globals().insert(sym);
        }

        self.visit_with_scope(unit, node, scopes, sym, sn, ident, None);
    }

    // AST: parameters
    fn visit_parameters(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            match child.kind_id() {
                LangPython::identifier => {
                    if let Some(ident) = child.as_ident() {
                        scopes.lookup_or_insert(&ident.name, &child, SymKind::Variable);
                    }
                }
                LangPython::default_parameter
                | LangPython::typed_parameter
                | LangPython::typed_default_parameter => {
                    self.visit_node(unit, &child, scopes, namespace, parent);
                }
                LangPython::list_splat_pattern | LangPython::dictionary_splat_pattern => {
                    Self::collect_pattern_identifiers(unit, &child, scopes, SymKind::Variable);
                }
                _ => {
                    self.visit_node(unit, &child, scopes, namespace, parent);
                }
            }
        }
    }

    // AST: default_parameter (e.g., x=10)
    fn visit_default_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(name_node) = node.child_by_field(unit, LangPython::field_name) {
            if let Some(ident) = name_node.as_ident() {
                scopes.lookup_or_insert(&ident.name, node, SymKind::Variable);
            }
        }
    }

    // AST: typed_parameter (e.g., x: int)
    fn visit_typed_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            if child.kind_id() == LangPython::identifier {
                if let Some(ident) = child.as_ident() {
                    if let Some(sym) = scopes.lookup_or_insert(&ident.name, node, SymKind::Variable) {
                        ident.set_symbol(sym);
                    }
                    break;
                }
            }
        }
    }

    // AST: typed_default_parameter (e.g., x: int = 10)
    fn visit_typed_default_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(name_node) = node.child_by_field(unit, LangPython::field_name) {
            if let Some(ident) = name_node.as_ident() {
                if let Some(sym) = scopes.lookup_or_insert(&ident.name, node, SymKind::Variable) {
                    ident.set_symbol(sym);
                }
            }
        }
    }

    // AST: assignment (e.g., x = 10, a, b = 1, 2)
    fn visit_assignment(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(left_node) = node.child_by_field(unit, LangPython::field_left) {
            Self::collect_pattern_identifiers(unit, &left_node, scopes, SymKind::Variable);
        }

        if let Some(right_node) = node.child_by_field(unit, LangPython::field_right) {
            self.visit_node(unit, &right_node, scopes, namespace, parent);
        }
    }

    // AST: lambda
    fn visit_lambda(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Create a scope for the lambda (similar to Rust closure handling)
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);

            // Link scope to parent namespace
            scope.add_parent(namespace);

            scopes.push_scope(scope);

            // Collect lambda parameters
            if let Some(params) = node.child_by_field(unit, LangPython::field_parameters) {
                let _ = Self::collect_pattern_identifiers(unit, &params, scopes, SymKind::Variable);
            }

            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // AST: import_statement
    fn visit_import_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            match child.kind_id() {
                LangPython::dotted_name => {
                    if let Some(first_ident) = child.children(unit).first()
                        && let Some(ident) = first_ident.as_ident()
                    {
                        scopes.lookup_or_insert(&ident.name, node, SymKind::Module);
                    }
                }
                LangPython::aliased_import => {
                    self.visit_aliased_import(unit, &child, scopes, namespace, parent);
                }
                _ => {}
            }
        }
    }

    // AST: import_from_statement
    fn visit_import_from_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            match child.kind_id() {
                LangPython::identifier => {
                    if let Some(ident) = child.as_ident() {
                        scopes.lookup_or_insert(&ident.name, node, SymKind::Module);
                    }
                }
                LangPython::dotted_name => {
                    // Handle dotted_name for imported names (e.g., `from typing import List`)
                    // Only extract from field_name, not field_module_name
                    if child.field_id() == LangPython::field_name {
                        if let Some(first_ident) = child.children(unit).first()
                            && let Some(ident) = first_ident.as_ident()
                        {
                            scopes.lookup_or_insert(&ident.name, node, SymKind::Module);
                        }
                    }
                }
                LangPython::aliased_import => {
                    self.visit_aliased_import(unit, &child, scopes, namespace, parent);
                }
                _ => {}
            }
        }
    }

    // AST: aliased_import (e.g., foo as f)
    fn visit_aliased_import(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(alias_node) = node.child_by_field(unit, LangPython::field_alias) {
            if let Some(ident) = alias_node.as_ident() {
                scopes.lookup_or_insert(&ident.name, node, SymKind::Module);
            }
        } else if let Some(name_node) = node.child_by_field(unit, LangPython::field_name) {
            if let Some(ident) = name_node.find_ident(unit) {
                scopes.lookup_or_insert(&ident.name, node, SymKind::Module);
            }
        }
    }

    // AST: type_alias_statement (e.g., type Alias = int)
    fn visit_type_alias_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(name_node) = node.child_by_field(unit, LangPython::field_name) {
            if let Some(ident) = name_node.as_ident() {
                if let Some(sym) = scopes.lookup_or_insert(&ident.name, node, SymKind::TypeAlias) {
                    sym.set_is_global(true);
                    scopes.globals().insert(sym);
                }
            }
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // AST: for_statement - declare loop variable
    fn visit_for_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(left_node) = node.child_by_field(unit, LangPython::field_left) {
            Self::collect_pattern_identifiers(unit, &left_node, scopes, SymKind::Variable);
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // AST: except_clause - declare exception variable
    fn visit_except_clause(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            if child.kind_id() == LangPython::as_pattern {
                Self::collect_pattern_identifiers(unit, &child, scopes, SymKind::Variable);
            }
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // AST: with_statement - declare context manager variable
    fn visit_with_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            if child.kind_id() == LangPython::with_clause {
                for &item_id in child.child_ids() {
                    let item = unit.hir_node(item_id);
                    if item.kind_id() == LangPython::with_item {
                        for &sub_id in item.child_ids() {
                            let sub = unit.hir_node(sub_id);
                            if sub.kind_id() == LangPython::as_pattern {
                                Self::collect_pattern_identifiers(
                                    unit, &sub, scopes, SymKind::Variable,
                                );
                            }
                        }
                    }
                }
            }
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }
}

/// Entry point for symbol collection
pub fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scope_stack: ScopeStack<'tcx>,
    _config: &ResolverOption,
) -> &'tcx Scope<'tcx> {
    let cc = unit.cc;
    let arena = cc.arena();
    let unit_globals_val = Scope::new(HirId(unit.index));
    let scope_id = unit_globals_val.id().0;
    let unit_globals = arena.alloc_with_id(scope_id, unit_globals_val);
    let mut scopes = CollectorScopes::new(cc, unit.index, scope_stack, unit_globals);

    let mut visitor = CollectorVisitor::new();
    visitor.visit_node(&unit, node, &mut scopes, unit_globals, None);

    unit_globals
}
