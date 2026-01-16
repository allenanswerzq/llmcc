#![allow(clippy::collapsible_if, clippy::needless_return)]

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{
    SYM_KIND_ALL, SYM_KIND_CALLABLE, SYM_KIND_TYPES, SymKind, SymKindSet, Symbol,
};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::token::AstVisitorTypeScript;
use crate::token::LangTypeScript;

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

    #[tracing::instrument(skip_all)]
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
        bind_return: bool,
    ) {
        let depth = scopes.scope_depth();

        // Any export modifier makes the symbol global
        // TODO: Check for export keyword in parent

        let child_parent = sn.opt_symbol().or(parent);

        // Skip if scope wasn't set (incomplete TypeScript parsing)
        if sn.opt_scope().is_none() {
            self.visit_children(unit, node, scopes, scopes.top(), child_parent);
            return;
        }
        scopes.push_scope_node(sn);

        // Run the scope enter callback if provided
        if let Some(callback) = on_scope_enter {
            callback(unit, sn, scopes);
        }

        self.visit_children(unit, node, scopes, scopes.top(), child_parent);

        // Bind return type while scope is still valid (for async unwrapping)
        if bind_return {
            self.bind_return_type(unit, node, scopes);
        }

        scopes.pop_until(depth);
    }

    /// Bind the type annotation of a field (property_signature or public_field_definition)
    fn bind_field_type(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
    ) {
        // Get the field symbol from its name
        if let Some(field_sym) = node.ident_symbol_by_field(unit, LangTypeScript::field_name) {
            // Set field_of to parent struct/trait
            if let Some(parent_sym) = namespace.opt_symbol() {
                field_sym.set_field_of(parent_sym.id());
            }

            // Get the type annotation and resolve it
            if let Some(type_node) = node.child_by_field(unit, LangTypeScript::field_type)
                && let Some(field_type) = crate::infer::infer_type(unit, scopes, &type_node)
            {
                field_sym.set_type_of(field_type.id());
            }
        }
    }

    /// Bind the type annotation of a parameter (required_parameter or optional_parameter)
    fn bind_parameter_type(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
    ) {
        // Parameters use "pattern" field for the name identifier
        if let Some(param_sym) = node.ident_symbol_by_field(unit, LangTypeScript::field_pattern) {
            // Get the type annotation and resolve it
            if let Some(type_node) = node.child_by_field(unit, LangTypeScript::field_type)
                && let Some(param_type) = crate::infer::infer_type(unit, scopes, &type_node)
            {
                param_sym.set_type_of(param_type.id());
            }
        }
    }

    /// Bind the type annotation for a rest parameter (...args: T[]).
    /// The rest_pattern is inside required_parameter, but the type annotation is on required_parameter.
    fn bind_rest_parameter_type(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
    ) {
        // Get the identifier from the rest_pattern (could be direct child or find_ident)
        if let Some(ident) = node.find_ident(unit)
            && let Some(param_sym) = ident.opt_symbol()
        {
            // The type annotation is on the parent required_parameter
            if let Some(parent_id) = node.parent()
                && let Some(parent_node) = unit.opt_hir_node(parent_id)
                && parent_node.kind_id() == LangTypeScript::required_parameter
                && let Some(type_node) =
                    parent_node.child_by_field(unit, LangTypeScript::field_type)
                && let Some(param_type) = crate::infer::infer_type(unit, scopes, &type_node)
            {
                param_sym.set_type_of(param_type.id());
            }
        }
    }

    /// Bind the return type annotation of a function with async unwrapping.
    /// For async functions with `Promise<T>` return types, sets the identifier node
    /// to point to the unwrapped type T's symbol instead of the Promise symbol.
    /// This works because each identifier NODE is unique, even though symbols are shared.
    fn bind_return_type(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
    ) {
        // Get the return type node
        if let Some(ret_type_node) = node.child_by_field(unit, LangTypeScript::field_return_type) {
            // Infer the return type with unwrapping (Promise<T> -> T)
            if let Some(unwrapped_type) = crate::infer::infer_type(unit, scopes, &ret_type_node) {
                // Find the first identifier in the return type and set it to the unwrapped type
                if let Some(ident) = ret_type_node.find_ident(unit) {
                    // Only update if the unwrapped type is different from current
                    if let Some(current_sym) = ident.opt_symbol() {
                        if unwrapped_type.id() != current_sym.id() {
                            // Set the identifier NODE to point to the unwrapped type symbol
                            ident.set_symbol(unwrapped_type);
                        }
                    } else {
                        ident.set_symbol(unwrapped_type);
                    }
                }
            }
        }
    }
}

impl<'tcx> AstVisitorTypeScript<'tcx, BinderScopes<'tcx>> for BinderVisitor<'tcx> {
    fn visit_program(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let file_path = unit.file_path().unwrap();
        let _ = file_path; // Used for debugging
        let depth = scopes.scope_depth();
        let meta = unit.unit_meta();

        // Push package scope if present
        if let Some(ref package_name) = meta.package_name
            && let Some(symbol) =
                scopes.lookup_symbol(package_name, SymKindSet::from_kind(SymKind::Crate))
            && let Some(scope_id) = symbol.opt_scope()
        {
            scopes.push_scope(scope_id);
        }

        // Push module scope if present
        if let Some(ref module_name) = meta.module_name
            && let Some(symbol) =
                scopes.lookup_symbol(module_name, SymKindSet::from_kind(SymKind::Module))
            && let Some(scope_id) = symbol.opt_scope()
        {
            scopes.push_scope(scope_id);
        }

        // Push file scope
        if let Some(ref file_name) = meta.file_name
            && let Some(file_sym) =
                scopes.lookup_symbol(file_name, SymKindSet::from_kind(SymKind::File))
            && let Some(scope_id) = file_sym.opt_scope()
        {
            scopes.push_scope(scope_id);

            let file_scope = unit.get_scope(scope_id);
            self.visit_children(unit, node, scopes, file_scope, Some(file_sym));
            scopes.pop_until(depth);
            return;
        }

        self.visit_children(unit, node, scopes, scopes.top(), None);
    }

    // Identifier binding
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

    // Type identifier binding
    fn visit_type_identifier(
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

        if let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_TYPES) {
            ident.set_symbol(symbol);
        }
    }

    // Predefined type (string, number, boolean, etc.)
    fn visit_predefined_type(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let ident = node.as_ident().unwrap();
        if let Some(symbol) =
            scopes.lookup_global(ident.name, SymKindSet::from_kind(SymKind::Primitive))
        {
            ident.set_symbol(symbol);
        }
    }

    // Class declaration
    fn visit_class_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None, false);

            // After visiting, process class_heritage to find extends and implements clauses
            if let Some(class_sym) = sn.opt_symbol() {
                for child_id in node.child_ids() {
                    let child = unit.hir_node(*child_id);

                    // Process decorators (@Component, @Injectable, etc.)
                    if child.kind_id() == LangTypeScript::decorator {
                        // Decorator can be:
                        // - Simple: @Log -> decorator > identifier
                        // - Called: @Injectable() -> decorator > call_expression > identifier
                        // Find the decorator function identifier
                        if let Some(decorator_ident) = child.find_ident(unit)
                            && let Some(decorator_sym) =
                                scopes.lookup_symbol(decorator_ident.name, SYM_KIND_CALLABLE)
                        {
                            class_sym.add_decorator(decorator_sym.id());
                        }
                    } else if child.kind_id() == LangTypeScript::class_heritage {
                        // Look for extends_clause and implements_clause inside class_heritage
                        for heritage_child_id in child.child_ids() {
                            let heritage_child = unit.hir_node(*heritage_child_id);
                            if heritage_child.kind_id() == LangTypeScript::extends_clause {
                                // Find the extended class type
                                // extends_clause has "value" field pointing to the type
                                // Store as type_of (class can only extend one class)
                                if let Some(value_node) =
                                    heritage_child.child_by_field(unit, LangTypeScript::field_value)
                                {
                                    if let Some(type_ident) = value_node.find_ident(unit)
                                        && let Some(type_sym) =
                                            scopes.lookup_symbol(type_ident.name, SYM_KIND_TYPES)
                                    {
                                        // Store extends as type_of (single inheritance)
                                        class_sym.set_type_of(type_sym.id());
                                    }
                                }
                            } else if heritage_child.kind_id() == LangTypeScript::implements_clause
                            {
                                // Process implements_clause: class Foo implements Bar, Baz
                                // Store as nested_types (can implement multiple interfaces)
                                for type_child_id in heritage_child.child_ids() {
                                    let type_node = unit.hir_node(*type_child_id);
                                    // Try to resolve each implemented interface
                                    if let Some(type_ident) = type_node.find_ident(unit)
                                        && let Some(type_sym) =
                                            scopes.lookup_symbol(type_ident.name, SYM_KIND_TYPES)
                                    {
                                        class_sym.add_nested_type(type_sym.id());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn visit_abstract_class_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_class_declaration(unit, node, scopes, namespace, parent);
    }

    // Interface declaration
    fn visit_interface_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None, false);

            // After visiting, process extends_clause to add inheritance relationships
            if let Some(iface_sym) = sn.opt_symbol() {
                // Find extends_type_clause children and resolve extended types
                for child_id in node.child_ids() {
                    let child = unit.hir_node(*child_id);
                    if child.kind_id() == LangTypeScript::extends_type_clause {
                        // Process extends_type_clause: interface Foo extends Bar, Baz
                        for type_child_id in child.child_ids() {
                            let type_node = unit.hir_node(*type_child_id);
                            // Try to resolve the extended type
                            if let Some(type_ident) = type_node.find_ident(unit)
                                && let Some(type_sym) =
                                    scopes.lookup_symbol(type_ident.name, SYM_KIND_TYPES)
                            {
                                iface_sym.add_nested_type(type_sym.id());
                            }
                        }
                    }
                }
            }
        }
    }

    // Type alias declaration
    fn visit_type_alias_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None, false);
        }
    }

    // Enum declaration
    fn visit_enum_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None, false);
        }
    }

    // Property signature (interface fields)
    fn visit_property_signature(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
        self.bind_field_type(unit, node, scopes, namespace);
    }

    // Public field definition (class fields)
    fn visit_public_field_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
        self.bind_field_type(unit, node, scopes, namespace);
    }

    // Required parameter
    fn visit_required_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
        self.bind_parameter_type(unit, node, scopes);

        // Parameter decorators: `handle(@inject req: string)`
        // Attach decorator dependency to the enclosing function/method so it shows as `@tdep`.
        if let Some(parent_sym) = parent {
            for child in node.children(unit) {
                if child.kind_id() == LangTypeScript::decorator {
                    if let Some(decorator_ident) = child.find_ident(unit)
                        && let Some(decorator_sym) =
                            scopes.lookup_symbol(decorator_ident.name, SYM_KIND_CALLABLE)
                    {
                        parent_sym.add_decorator(decorator_sym.id());
                    }
                }
            }
        }
    }

    // Rest pattern (e.g., ...args in function(...args: T[]))
    fn visit_rest_pattern(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
        self.bind_rest_parameter_type(unit, node, scopes);
    }

    // Optional parameter
    fn visit_optional_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);
        self.bind_parameter_type(unit, node, scopes);
    }

    // Type parameter (e.g., T in function<T extends HasLength>)
    fn visit_type_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Get the type parameter name symbol
        if let Some(type_param_sym) = node.ident_symbol_by_field(unit, LangTypeScript::field_name) {
            // Look for constraint (T extends SomeType)
            if let Some(constraint_node) =
                node.child_by_field(unit, LangTypeScript::field_constraint)
            {
                // The constraint contains the bound type (e.g., HasLength)
                if let Some(bound_type) = crate::infer::infer_type(unit, scopes, &constraint_node) {
                    type_param_sym.set_type_of(bound_type.id());
                }
            }
        }
    }

    // Function declaration
    fn visit_function_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None, true);
        }
    }

    // Function signature (declare function externalFn(x: number): string;)
    fn visit_function_signature(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None, true);
        }
    }

    // Generator function declaration (function* name() { ... })
    fn visit_generator_function_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None, true);
        }
    }

    // Method definition
    fn visit_method_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None, true);
        }
    }

    // Variable declarator: bind type annotation to variable symbol
    fn visit_variable_declarator(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Get the name field which can be an identifier or a destructuring pattern
        let Some(name_node) = node.child_by_field(unit, LangTypeScript::field_name) else {
            return;
        };

        let name_kind = name_node.kind_id();

        // Handle destructuring patterns: let [a, b] = ... or let { x, y } = ...
        if name_kind == LangTypeScript::array_pattern || name_kind == LangTypeScript::object_pattern
        {
            // Try to get type from explicit annotation: let [a, b]: [number, number] = ...
            if let Some(type_node) = node.child_by_field(unit, LangTypeScript::field_type)
                && let Some(type_sym) = crate::infer::infer_type(unit, scopes, &type_node)
            {
                crate::pattern::bind_pattern_types(unit, scopes, &name_node, type_sym);
                return;
            }

            // Try to infer type from value: let [a, b] = [1, 2]
            if let Some(value_node) = node.child_by_field(unit, LangTypeScript::field_value)
                && let Some(type_sym) = crate::infer::infer_type(unit, scopes, &value_node)
            {
                crate::pattern::bind_pattern_types(unit, scopes, &name_node, type_sym);
            }
            return;
        }

        // Handle simple identifier: let a = ...
        if let Some(var_sym) = name_node.ident_symbol(unit) {
            // Try to get type from explicit annotation: let a: number
            if let Some(type_node) = node.child_by_field(unit, LangTypeScript::field_type)
                && let Some(type_sym) = crate::infer::infer_type(unit, scopes, &type_node)
            {
                var_sym.set_type_of(type_sym.id());
                return;
            }

            // Try to infer type from value: let a = 42
            if let Some(value_node) = node.child_by_field(unit, LangTypeScript::field_value)
                && let Some(type_sym) = crate::infer::infer_type(unit, scopes, &value_node)
            {
                var_sym.set_type_of(type_sym.id());
            }
        }
    }

    // Arrow function
    fn visit_arrow_function(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None, true);
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Call expression - bind function calls
    fn visit_call_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Try to bind the function being called
        if let Some(func_node) = node.child_by_field(unit, LangTypeScript::field_function) {
            if let Some(ident) = func_node.as_ident() {
                if let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_CALLABLE) {
                    ident.set_symbol(symbol);
                }
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // New expression - bind constructor calls
    fn visit_new_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Try to bind the constructor (class being instantiated)
        if let Some(constructor_node) = node.child_by_field(unit, LangTypeScript::field_constructor)
        {
            if let Some(ident) = constructor_node.as_ident() {
                if let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_TYPES) {
                    ident.set_symbol(symbol);
                }
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Member expression
    fn visit_member_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Visit object to bind it
        if let Some(obj_node) = node.child_by_field(unit, LangTypeScript::field_object) {
            self.visit_node(unit, &obj_node, scopes, namespace, parent);
        }

        // Property binding would require type inference
        // For now, just visit children
    }

    // Assignment expression - handle destructuring assignments
    fn visit_assignment_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_children(unit, node, scopes, namespace, parent);

        // Check if the left side is a destructuring pattern
        let Some(left) = node.child_by_field(unit, LangTypeScript::field_left) else {
            return;
        };

        let left_kind = left.kind_id();
        if left_kind != LangTypeScript::array_pattern && left_kind != LangTypeScript::object_pattern
        {
            return;
        }

        // Try to infer type from the right side and bind to pattern
        if let Some(right) = node.child_by_field(unit, LangTypeScript::field_right)
            && let Some(type_sym) = crate::infer::infer_type(unit, scopes, &right)
        {
            crate::pattern::bind_pattern_types(unit, scopes, &left, type_sym);
        }
    }
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    config: &ResolverOption,
) {
    let mut scopes = BinderScopes::new(unit, namespace);
    let mut visit = BinderVisitor::new(config.clone());
    visit.visit_node(&unit, node, &mut scopes, namespace, None);
}
