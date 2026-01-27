use std::collections::HashMap;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirIdent, HirNode, HirScope};
use llmcc_core::next_hir_id;
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{ScopeId, SymKind, SymKindSet, Symbol};
use llmcc_resolver::{CollectorScopes, ResolverOption};

use crate::LangPython;
use crate::token::AstVisitorPython;

/// Check if a function is in a method context (parent is a class).
/// This is used to distinguish between free functions and methods inside classes.
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

    /// Declare a symbol from a named field in the AST node
    fn declare_symbol(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        // try to find identifier by field first, if not found try scope's identifier
        let ident = node
            .ident_by_field(unit, field_id)
            .or_else(|| node.as_scope().and_then(|sn| sn.opt_ident()))?;

        let sym = scopes.lookup_or_insert(ident.name, node, kind)?;
        ident.set_symbol(sym);

        // Also set the ident on the scope so set_block_id can find it
        if let Some(sn) = node.as_scope() {
            sn.set_ident(ident);
        }

        Some(sym)
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

    /// Recursive worker for [`collect_pattern_identifiers`].
    fn collect_pattern_identifiers_impl(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
        symbols: &mut Vec<&'tcx Symbol>,
    ) {
        // Skip non-binding nodes
        if matches!(
            node.kind_id(),
            k if k == LangPython::attribute || k == LangPython::subscript
        ) {
            return;
        }

        if let Some(ident) = node.as_ident() {
            let name = ident.name.to_string();
            let sym = scopes.lookup_or_insert(&name, node, kind);

            if let Some(sym) = sym {
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

    /// Lookup a symbol by name, trying primary kind first, then inserting new
    fn lookup_or_convert(
        &mut self,
        unit: &CompileUnit<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        name: &str,
        node: &HirNode<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        if let Some(symbol) = scopes.lookup_symbol(name, SymKindSet::from_kind(kind)) {
            return Some(symbol);
        }

        if let Some(symbol) = scopes.lookup_or_insert(name, node, kind) {
            if symbol.opt_scope().is_none() {
                let scope = self.alloc_scope(unit, symbol);
                symbol.set_scope(scope.id());
            }
            return Some(symbol);
        }

        None
    }

    fn visit_with_scope(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        sym: &'tcx Symbol,
        sn: &'tcx HirScope<'tcx>,
        ident: &'tcx HirIdent<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        on_scope_enter: Option<ScopeEntryCallback<'tcx>>,
    ) {
        ident.set_symbol(sym);
        sn.set_ident(ident);

        let depth = scopes.scope_depth();
        let scope = if sym.opt_scope().is_none() {
            self.alloc_scope(unit, sym)
        } else {
            self.get_scope(sym.scope())
                .unwrap_or_else(|| self.alloc_scope(unit, sym))
        };

        sym.set_scope(scope.id());
        sn.set_scope(scope);
        scope.add_parent(namespace);

        scopes.push_scope(scope);
        if let Some(callback) = on_scope_enter {
            callback(node, scopes);
        }
        self.visit_children(unit, node, scopes, scope, Some(sym));
        scopes.pop_until(depth);
    }

    /// AST: Generic scoped-named item handler (class, function, etc.)
    #[allow(clippy::too_many_arguments)]
    fn visit_scoped_named(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
        kind: SymKind,
        field_id: u16,
        on_scope_enter: Option<ScopeEntryCallback<'tcx>>,
    ) {
        if let Some((sn, ident)) = node.scope_and_ident_by_field(unit, field_id)
            && let Some(sym) = self.lookup_or_convert(unit, scopes, ident.name, node, kind)
        {
            self.mark_global_if_top_level(sym, scopes, parent);
            self.visit_with_scope(unit, node, scopes, sym, sn, ident, namespace, on_scope_enter);
        }
    }

    fn mark_global_if_top_level(&self, sym: &'tcx Symbol, scopes: &CollectorScopes<'tcx>, parent: Option<&Symbol>) {
        if parent.is_none() {
            sym.set_is_global(true);
            scopes.globals().insert(sym);
            return;
        }

        if let Some(parent_sym) = parent
            && matches!(parent_sym.kind(), SymKind::File | SymKind::Module | SymKind::Crate)
        {
            sym.set_is_global(true);
            scopes.globals().insert(sym);
        }
    }
}

impl<'tcx> AstVisitorPython<'tcx, CollectorScopes<'tcx>> for CollectorVisitor<'tcx> {
    fn visit_module(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let depth = scopes.scope_depth();
        let meta = unit.unit_meta();

        // Track package scope for parent relationships
        let mut package_scope: Option<&'tcx Scope<'tcx>> = None;

        // Set up package (crate) scope from unit metadata
        if let Some(ref package_name) = meta.package_name
            && let Some(symbol) = scopes.lookup_or_insert_global(package_name, node, SymKind::Crate)
        {
            scopes.push_scope_with(node, Some(symbol));
            package_scope = scopes.top();
        }

        // For files in subdirectories, create a module scope for proper hierarchy traversal
        let mut module_wrapper_scope: Option<&'tcx Scope<'tcx>> = None;
        if let Some(ref module_name) = meta.module_name
            && let Some(module_sym) = scopes.lookup_or_insert_global(module_name, node, SymKind::Module)
        {
            let mod_scope = self.alloc_scope(unit, module_sym);
            if let Some(pkg_s) = package_scope {
                mod_scope.add_parent(pkg_s);
            }
            module_wrapper_scope = Some(mod_scope);
        }

        // Create file symbol and scope
        if let Some(ref file_name) = meta.file_name {
            let file_sym = scopes.lookup_or_insert(file_name, node, SymKind::File);
            if let Some(file_sym) = file_sym {
                let arena_name = unit.cc.arena().alloc_str(file_name);
                let ident = unit.cc.alloc_file_ident(next_hir_id(), arena_name, file_sym);
                ident.set_symbol(file_sym);

                let file_scope = self.alloc_scope(unit, file_sym);
                file_scope.add_parent(namespace);
                if let Some(mod_scope) = module_wrapper_scope {
                    file_scope.add_parent(mod_scope);
                }
                if let Some(pkg_s) = package_scope {
                    file_scope.add_parent(pkg_s);
                }

                if let Some(sn) = node.as_scope() {
                    sn.set_ident(ident);
                    sn.set_scope(file_scope);
                }

                scopes.push_scope(file_scope);
                self.visit_children(unit, node, scopes, file_scope, Some(file_sym));
                scopes.pop_until(depth);
                return;
            }
        }

        // Fallback: just visit children
        self.visit_children(unit, node, scopes, namespace, None);
        scopes.pop_until(depth);
    }

    /// AST: class ClassName(bases): body
    fn visit_class_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Insert 'self' and 'cls' type aliases when entering class scope
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Struct,
            LangPython::field_name,
            Some(Box::new(|node, scopes| {
                let _ = scopes.lookup_or_insert("self", node, SymKind::TypeAlias);
                let _ = scopes.lookup_or_insert("cls", node, SymKind::TypeAlias);
            })),
        );

        // Also add class to unit_globals for cross-module type resolution
        if let Some((_, ident)) = node.scope_and_ident_by_field(unit, LangPython::field_name)
            && let Some(sym) =
                scopes.lookup_symbol(ident.name, SymKindSet::from_kind(SymKind::Struct))
        {
            scopes.globals().insert(sym);
        }
    }

    /// AST: def function_name(params): body
    fn visit_function_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Determine if this is a method (inside class) or a free function
        let is_method = is_method_context(parent);
        let kind = if is_method {
            SymKind::Method
        } else {
            SymKind::Function
        };

        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            kind,
            LangPython::field_name,
            None,
        );

        // For free functions, also add to unit_globals for cross-module resolution
        if !is_method
            && let Some((_, ident)) = node.scope_and_ident_by_field(unit, LangPython::field_name)
            && let Some(sym) = scopes.lookup_symbol(ident.name, SymKindSet::from_kind(kind))
        {
            scopes.globals().insert(sym);
        }
    }

    /// AST: @decorator class/function
    fn visit_decorated_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Just visit children - the actual class/function definition will be handled
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: lambda params: body
    fn visit_lambda(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Create a scope for the lambda
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);
            scope.add_parent(namespace);

            scopes.push_scope(scope);

            // Collect lambda parameters
            if let Some(params) = node.child_by_field(unit, LangPython::field_parameters) {
                let _ = Self::collect_pattern_identifiers(unit, &params, scopes, SymKind::Variable);
            }

            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        }
    }

    /// AST: for var in iterable: body
    fn visit_for_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Collect loop variable(s)
        if let Some(left_node) = node.child_by_field(unit, LangPython::field_left) {
            let _ = Self::collect_pattern_identifiers(unit, &left_node, scopes, SymKind::Variable);
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: except ExceptionType as name: body
    fn visit_except_clause(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Create a scope for the except clause
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);
            scope.add_parent(namespace);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    /// AST: case pattern: body (Python 3.10+)
    fn visit_case_clause(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Create a scope for the case clause to hold pattern bindings
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);
            scope.add_parent(namespace);

            scopes.push_scope(scope);

            // Collect pattern bindings from case_pattern children
            for &child_id in node.child_ids() {
                let child = unit.hir_node(child_id);
                // case_pattern nodes contain the pattern bindings
                if !child.is_trivia() {
                    let _ = Self::collect_pattern_identifiers(unit, &child, scopes, SymKind::Variable);
                }
            }

            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    /// AST: list/dict/set/generator comprehension
    fn visit_list_comprehension(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Comprehensions have their own scope
        if let Some(sn) = node.as_scope() {
            let scope = unit.cc.alloc_scope(node.id());
            sn.set_scope(scope);
            scope.add_parent(namespace);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_dictionary_comprehension(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_list_comprehension(unit, node, scopes, namespace, parent);
    }

    fn visit_set_comprehension(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_list_comprehension(unit, node, scopes, namespace, parent);
    }

    fn visit_generator_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_list_comprehension(unit, node, scopes, namespace, parent);
    }

    /// AST: import module or from module import name
    fn visit_import_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Collect imported names as symbols
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            if child.kind_id() == LangPython::aliased_import {
                // import module as alias
                if let Some(alias_ident) = child.ident_by_field(unit, LangPython::field_alias) {
                    let _ = scopes.lookup_or_insert(alias_ident.name, node, SymKind::Module);
                } else if let Some(name_node) = child.child_by_field(unit, LangPython::field_name) {
                    if let Some(ident) = name_node.find_ident(unit) {
                        let _ = scopes.lookup_or_insert(ident.name, node, SymKind::Module);
                    }
                }
            } else if child.kind_id() == LangPython::dotted_name {
                // import module.submodule - get the first part
                if let Some(ident) = child.find_ident(unit) {
                    let _ = scopes.lookup_or_insert(ident.name, node, SymKind::Module);
                }
            }
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_import_from_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // from module import name, name2, name3
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            if child.kind_id() == LangPython::aliased_import {
                if let Some(alias_ident) = child.ident_by_field(unit, LangPython::field_alias) {
                    let _ = scopes.lookup_or_insert(alias_ident.name, node, SymKind::Variable);
                } else if let Some(name_node) = child.child_by_field(unit, LangPython::field_name) {
                    if let Some(ident) = name_node.find_ident(unit) {
                        let _ = scopes.lookup_or_insert(ident.name, node, SymKind::Variable);
                    }
                }
            } else if let Some(ident) = child.as_ident() {
                let _ = scopes.lookup_or_insert(ident.name, node, SymKind::Variable);
            }
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: x = value or x: Type = value
    fn visit_assignment(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Collect left side identifiers as variables
        if let Some(left_node) = node.child_by_field(unit, LangPython::field_left) {
            // Skip if this is an attribute or subscript assignment (not a new variable)
            if left_node.kind_id() != LangPython::attribute
                && left_node.kind_id() != LangPython::subscript
            {
                let _ = Self::collect_pattern_identifiers(unit, &left_node, scopes, SymKind::Variable);
            }
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: Named expression (walrus operator) x := value
    fn visit_named_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Get the name being assigned
        if let Some(name_node) = node.child_by_field(unit, LangPython::field_name) {
            if let Some(ident) = name_node.as_ident() {
                let _ = scopes.lookup_or_insert(ident.name, node, SymKind::Variable);
            }
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: global x, y, z
    fn visit_global_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Mark identifiers as global
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            if let Some(ident) = child.as_ident() {
                if let Some(sym) = scopes.lookup_or_insert(ident.name, node, SymKind::Variable) {
                    sym.set_is_global(true);
                    scopes.globals().insert(sym);
                }
            }
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: type Alias = Type (Python 3.12+)
    fn visit_type_alias_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) =
            self.declare_symbol(unit, node, scopes, SymKind::TypeAlias, LangPython::field_name)
        {
            self.mark_global_if_top_level(symbol, scopes, parent);
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    /// AST: param or param: Type or param: Type = default
    fn visit_typed_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Declare the parameter name
        if let Some(symbol) =
            self.declare_symbol(unit, node, scopes, SymKind::Variable, LangPython::field_name)
        {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        } else {
            // Fall back to collecting pattern identifiers
            let _ = Self::collect_pattern_identifiers(unit, node, scopes, SymKind::Variable);
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_typed_default_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_typed_parameter(unit, node, scopes, namespace, parent);
    }

    fn visit_default_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_typed_parameter(unit, node, scopes, namespace, parent);
    }

    /// AST: *args
    fn visit_list_splat_pattern(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = Self::collect_pattern_identifiers(unit, node, scopes, SymKind::Variable);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: **kwargs
    fn visit_dictionary_splat_pattern(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = Self::collect_pattern_identifiers(unit, node, scopes, SymKind::Variable);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: with expr as name: body
    fn visit_with_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Collect the 'as' binding if present
        if let Some(alias) = node.child_by_field(unit, LangPython::field_alias) {
            let _ = Self::collect_pattern_identifiers(unit, &alias, scopes, SymKind::Variable);
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: block { ... }
    fn visit_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Python blocks don't create new scopes like Rust blocks
        // Just visit children
        self.visit_children(unit, node, scopes, namespace, parent);
    }
}

pub fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scope_stack: ScopeStack<'tcx>,
    _config: &ResolverOption,
) -> &'tcx Scope<'tcx> {
    let cc = unit.cc;
    let arena = cc.arena();
    let unit_globals_val = Scope::new(llmcc_core::ir::HirId(unit.index));
    let scope_id = unit_globals_val.id().0;
    let unit_globals = arena.alloc_with_id(scope_id, unit_globals_val);
    let mut scopes = CollectorScopes::new(cc, unit.index, scope_stack, unit_globals);

    let mut visit = CollectorVisitor::new();
    visit.visit_node(&unit, node, &mut scopes, unit_globals, None);

    unit_globals
}
