#![allow(clippy::collapsible_if, clippy::needless_return)]

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SYM_KIND_ALL, SYM_KIND_CALLABLE, SymKind, SymKindSet, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::infer::infer_type;
use crate::pattern::bind_pattern_types;
use crate::token::AstVisitorPython;
use crate::token::LangPython;

type ScopeEnterFn<'tcx> =
    Box<dyn FnOnce(&CompileUnit<'tcx>, &'tcx HirScope<'tcx>, &mut BinderScopes<'tcx>) + 'tcx>;

/// Visitor for resolving symbol bindings and establishing relationships.
#[derive(Debug)]
pub struct BinderVisitor<'tcx> {
    #[allow(dead_code)]
    config: ResolverOption,
    phantom: std::marker::PhantomData<&'tcx ()>,
}

impl<'tcx> BinderVisitor<'tcx> {
    fn new(config: ResolverOption) -> Self {
        Self {
            config,
            phantom: std::marker::PhantomData,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_scoped_named(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        sn: &'tcx HirScope<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
        on_scope_enter: Option<ScopeEnterFn<'tcx>>,
    ) {
        let depth = scopes.scope_depth();
        let child_parent = sn.opt_symbol().or(parent);

        // Skip if scope wasn't set
        if sn.opt_scope().is_none() {
            self.visit_children(unit, node, scopes, scopes.top(), child_parent);
            return;
        }

        scopes.push_scope_node(sn);
        if let Some(callback) = on_scope_enter {
            callback(unit, sn, scopes);
        }

        self.visit_children(unit, node, scopes, scopes.top(), child_parent);
        scopes.pop_until(depth);
    }
}

impl<'tcx> AstVisitorPython<'tcx, BinderScopes<'tcx>> for BinderVisitor<'tcx> {
    fn visit_module(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let depth = scopes.scope_depth();
        let meta = unit.unit_meta();

        if let Some(ref package_name) = meta.package_name
            && let Some(symbol) = scopes.lookup_symbol(package_name, SymKindSet::from_kind(SymKind::Crate))
            && let Some(scope_id) = symbol.opt_scope()
        {
            scopes.push_scope(scope_id);
        }

        if let Some(ref module_name) = meta.module_name
            && let Some(symbol) = scopes.lookup_symbol(module_name, SymKindSet::from_kind(SymKind::Module))
            && let Some(scope_id) = symbol.opt_scope()
        {
            scopes.push_scope(scope_id);
        }

        if let Some(ref file_name) = meta.file_name
            && let Some(file_sym) = scopes.lookup_symbol(file_name, SymKindSet::from_kind(SymKind::File))
            && let Some(scope_id) = file_sym.opt_scope()
        {
            scopes.push_scope(scope_id);
            let file_scope = unit.get_scope(scope_id);
            self.visit_children(unit, node, scopes, file_scope, Some(file_sym));
            scopes.pop_until(depth);
            return;
        }

        self.visit_children(unit, node, scopes, scopes.top(), None);
        scopes.pop_until(depth);
    }

    /// AST: identifier (variable, function name, type name, etc.)
    fn visit_identifier(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let ident = node.as_ident().unwrap();
        if let Some(existing) = ident.opt_symbol()
            && existing.kind().is_resolved()
        {
            return;
        }

        if let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_ALL) {
            ident.set_symbol(symbol);
        }
    }

    /// AST: class ClassName(bases): body
    fn visit_class_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(
                unit,
                node,
                sn,
                scopes,
                namespace,
                parent,
                Some(Box::new(|_unit, sn, scopes| {
                    // Bind 'self' and 'cls' to the class type
                    for key in ["self", "cls"] {
                        if let Some(self_sym) =
                            scopes.lookup_symbol(key, SymKindSet::from_kind(SymKind::TypeAlias))
                            && let Some(class_sym) = sn.opt_symbol()
                        {
                            self_sym.set_type_of(class_sym.id());
                            if let Some(class_scope) = class_sym.opt_scope() {
                                self_sym.set_scope(class_scope);
                            }
                        }
                    }
                })),
            );

            // Handle class inheritance (superclasses)
            if let Some(class_sym) = sn.opt_symbol()
                && let Some(superclasses) = node.child_by_field(unit, LangPython::field_superclasses)
            {
                for &child_id in superclasses.child_ids() {
                    let child = unit.hir_node(child_id);
                    if child.is_trivia() {
                        continue;
                    }
                    if let Some(base_sym) = infer_type(unit, scopes, &child) {
                        class_sym.add_nested_type(base_sym.id());
                        // Set up scope parent relationship for inheritance
                        if let Some(base_scope) = base_sym.opt_scope()
                            && let Some(class_scope) = class_sym.opt_scope()
                        {
                            let class_scope = unit.get_scope(class_scope);
                            let base_scope = unit.get_scope(base_scope);
                            class_scope.add_parent(base_scope);
                        }
                    }
                }
            }
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    /// AST: def function_name(params) -> return_type: body
    fn visit_function_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None);

            // Handle return type annotation
            if let Some(fn_sym) = sn.opt_symbol()
                && let Some(return_type_node) = node.child_by_field(unit, LangPython::field_return_type)
                && let Some(return_type) = infer_type(unit, scopes, &return_type_node)
            {
                fn_sym.set_type_of(return_type.id());
            }
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    /// AST: @decorator def/class
    fn visit_decorated_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Visit decorators first to resolve them
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            if child.kind_id() == LangPython::decorator {
                self.visit_node(unit, &child, scopes, namespace, parent);
            }
        }
        // Then visit the definition
        if let Some(def_node) = node.child_by_field(unit, LangPython::field_definition) {
            self.visit_node(unit, &def_node, scopes, namespace, parent);
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    /// AST: lambda params: body
    fn visit_lambda(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope()
            && let Some(scope) = sn.opt_scope()
        {
            scopes.push_scope(scope.id());
            self.visit_children(unit, node, scopes, namespace, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    /// AST: obj.attribute
    fn visit_attribute(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Try to resolve the attribute on the object's type
        if let Some(object_node) = node.child_by_field(unit, LangPython::field_object)
            && let Some(attr_node) = node.child_by_field(unit, LangPython::field_attribute)
            && let Some(attr_ident) = attr_node.as_ident()
        {
            // If the object has a known type, look up the attribute in that type's scope
            if let Some(obj_sym) = infer_type(unit, scopes, &object_node)
                && let Some(attr_sym) = scopes.lookup_member_symbol(obj_sym, attr_ident.name, None)
            {
                attr_ident.set_symbol(attr_sym);
            }
        }
    }

    /// AST: func(args) or Class(args)
    fn visit_call(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // For simple identifiers that weren't resolved, try to resolve as callable
        if let Some(func_node) = node.child_by_field(unit, LangPython::field_function)
            && func_node.kind_id() == LangPython::identifier
            && let Some(ident) = func_node.as_ident()
            && ident.opt_symbol().is_none()
            && let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_CALLABLE)
        {
            ident.set_symbol(symbol);
        }
    }

    /// AST: x = value or (a, b) = value
    fn visit_assignment(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Try to infer type from the right side and bind to left side
        if let Some(left_node) = node.child_by_field(unit, LangPython::field_left)
            && let Some(right_node) = node.child_by_field(unit, LangPython::field_right)
            && let Some(value_type) = infer_type(unit, scopes, &right_node)
        {
            bind_pattern_types(unit, scopes, &left_node, value_type);
        }
    }

    /// AST: x: Type = value (annotated assignment)
    fn visit_typed_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Bind type annotation to parameter
        if let Some(type_node) = node.child_by_field(unit, LangPython::field_type)
            && let Some(name_node) = node.child_by_field(unit, LangPython::field_name)
            && let Some(type_sym) = infer_type(unit, scopes, &type_node)
        {
            bind_pattern_types(unit, scopes, &name_node, type_sym);
        }
    }

    fn visit_typed_default_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_typed_parameter(unit, node, scopes, namespace, parent);
    }

    /// AST: for var in iterable: body
    fn visit_for_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Visit the iterable first to resolve it
        if let Some(right_node) = node.child_by_field(unit, LangPython::field_right) {
            self.visit_node(unit, &right_node, scopes, namespace, parent);
        }

        // Then visit the loop variable(s)
        if let Some(left_node) = node.child_by_field(unit, LangPython::field_left)
            && let Some(right_node) = node.child_by_field(unit, LangPython::field_right)
            && let Some(iter_type) = infer_type(unit, scopes, &right_node)
        {
            // Try to get element type from iterable (e.g., List[int] -> int)
            if let Some(nested) = iter_type.nested_types()
                && !nested.is_empty()
            {
                if let Some(elem_type) = unit.opt_get_symbol(nested[0]) {
                    bind_pattern_types(unit, scopes, &left_node, elem_type);
                }
            }
        }

        // Visit the body
        if let Some(body) = node.child_by_field(unit, LangPython::field_body) {
            self.visit_node(unit, &body, scopes, namespace, parent);
        }

        // Visit alternative (else clause)
        if let Some(alt) = node.child_by_field(unit, LangPython::field_alternative) {
            self.visit_node(unit, &alt, scopes, namespace, parent);
        }
    }

    /// AST: case pattern: body
    fn visit_case_clause(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope()
            && let Some(scope) = sn.opt_scope()
        {
            scopes.push_scope(scope.id());
            self.visit_children(unit, node, scopes, namespace, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    /// AST: except ExceptionType as name: body
    fn visit_except_clause(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope()
            && let Some(scope) = sn.opt_scope()
        {
            scopes.push_scope(scope.id());
            self.visit_children(unit, node, scopes, namespace, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    /// AST: [x for x in iterable] or similar comprehensions
    fn visit_list_comprehension(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope()
            && let Some(scope) = sn.opt_scope()
        {
            scopes.push_scope(scope.id());
            self.visit_children(unit, node, scopes, namespace, parent);
            scopes.pop_scope();
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_dictionary_comprehension(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_list_comprehension(unit, node, scopes, namespace, parent);
    }

    fn visit_set_comprehension(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_list_comprehension(unit, node, scopes, namespace, parent);
    }

    fn visit_generator_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
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
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_import_from_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    /// AST: type Alias = Type
    fn visit_type_alias_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Bind the alias to its target type
        if let Some(name_sym) = node.ident_symbol_by_field(unit, LangPython::field_name)
            && let Some(value_node) = node.child_by_field(unit, LangPython::field_value)
            && let Some(type_sym) = infer_type(unit, scopes, &value_node)
        {
            name_sym.set_type_of(type_sym.id());
        }
    }

    /// AST: with expr as name: body
    fn visit_with_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Try to bind the 'as' variable to the context manager's type
        if let Some(value_node) = node.child_by_field(unit, LangPython::field_value)
            && let Some(alias_node) = node.child_by_field(unit, LangPython::field_alias)
            && let Some(ctx_type) = infer_type(unit, scopes, &value_node)
        {
            bind_pattern_types(unit, scopes, &alias_node, ctx_type);
        }
    }

    /// AST: named expression (walrus operator) x := value
    fn visit_named_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Bind the name to the value's type
        if let Some(name_node) = node.child_by_field(unit, LangPython::field_name)
            && let Some(value_node) = node.child_by_field(unit, LangPython::field_value)
            && let Some(value_type) = infer_type(unit, scopes, &value_node)
        {
            bind_pattern_types(unit, scopes, &name_node, value_type);
        }
    }
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    globals: &'tcx Scope<'tcx>,
    config: &ResolverOption,
) {
    let mut scopes = BinderScopes::new(unit, globals);
    let mut visit = BinderVisitor::new(config.clone());
    visit.visit_node(&unit, node, &mut scopes, globals, None);
}
