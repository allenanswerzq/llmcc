#![allow(
    clippy::collapsible_if,
    clippy::needless_return,
    clippy::only_used_in_recursion
)]

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SYM_KIND_ALL, SYM_KIND_TYPES, SymKind, SymKindSet, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::infer::infer_type;
use crate::token::AstVisitorCpp;
use crate::token::LangCpp;

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
    ) {
        let depth = scopes.scope_depth();

        let child_parent = sn.opt_symbol().or(parent);

        // Skip if scope wasn't set
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

        scopes.pop_until(depth);
    }

    /// Bind the type of a field declaration
    fn bind_field_type(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
    ) {
        // Get the field symbol from the declarator
        if let Some(decl_node) = node.child_by_field(unit, LangCpp::field_declarator) {
            if let Some(field_sym) = decl_node.find_ident(unit).and_then(|i| i.opt_symbol()) {
                // Set field_of to parent struct/class
                if let Some(parent_sym) = namespace.opt_symbol() {
                    field_sym.set_field_of(parent_sym.id());
                }

                // Get the type and resolve it
                if let Some(type_node) = node.child_by_field(unit, LangCpp::field_type)
                    && let Some(field_type) = infer_type(unit, scopes, &type_node)
                {
                    field_sym.set_type_of(field_type.id());
                }
            }
        }
    }

    /// Bind the type annotation of a parameter
    fn bind_parameter_type(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
    ) {
        if let Some(decl_node) = node.child_by_field(unit, LangCpp::field_declarator) {
            if let Some(param_sym) = decl_node.find_ident(unit).and_then(|i| i.opt_symbol()) {
                // Get the type and resolve it
                if let Some(type_node) = node.child_by_field(unit, LangCpp::field_type)
                    && let Some(param_type) = infer_type(unit, scopes, &type_node)
                {
                    param_sym.set_type_of(param_type.id());
                }
            }
        }
    }

    /// Get the declarator name (handles nested declarators)
    #[allow(dead_code)]
    fn get_declarator_name<'a>(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &'a HirNode<'tcx>,
    ) -> Option<&'tcx llmcc_core::ir::HirIdent<'tcx>> {
        if let Some(ident) = node.find_ident(unit) {
            return Some(ident);
        }

        if let Some(decl) = node.child_by_field(unit, LangCpp::field_declarator) {
            return self.get_declarator_name(unit, &decl);
        }

        None
    }
}

impl<'tcx> AstVisitorCpp<'tcx, BinderScopes<'tcx>> for BinderVisitor<'tcx> {
    fn visit_translation_unit(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let file_path = unit.file_path().unwrap();
        let depth = scopes.scope_depth();
        let meta = unit.unit_meta();

        tracing::trace!("binding translation_unit: {}", file_path);

        // Push package scope if present
        if let Some(ref package_name) = meta.package_name
            && let Some(symbol) =
                scopes.lookup_symbol(package_name, SymKindSet::from_kind(SymKind::Crate))
            && let Some(scope_id) = symbol.opt_scope()
        {
            tracing::trace!("pushing package scope {:?}", scope_id);
            scopes.push_scope(scope_id);
        }

        // Push module scope if present
        if let Some(ref module_name) = meta.module_name
            && let Some(symbol) =
                scopes.lookup_symbol(module_name, SymKindSet::from_kind(SymKind::Module))
            && let Some(scope_id) = symbol.opt_scope()
        {
            tracing::trace!("pushing module scope {:?}", scope_id);
            scopes.push_scope(scope_id);
        }

        // Push file scope
        if let Some(ref file_name) = meta.file_name
            && let Some(file_sym) =
                scopes.lookup_symbol(file_name, SymKindSet::from_kind(SymKind::File))
            && let Some(scope_id) = file_sym.opt_scope()
        {
            tracing::trace!("pushing file scope {} {:?}", file_path, scope_id);
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

    // Primitive type (int, char, double, etc.)
    fn visit_primitive_type(
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

    // Field identifier
    fn visit_field_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_identifier(unit, node, scopes, namespace, parent);
    }

    // Namespace identifier
    fn visit_namespace_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_identifier(unit, node, scopes, namespace, parent);
    }

    // Namespace definition
    fn visit_namespace_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None);
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Class specifier
    fn visit_class_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        self.visit_scoped_named(
            unit,
            node,
            sn,
            scopes,
            namespace,
            parent,
            Some(Box::new(|_unit, sn, scopes| {
                // Set up 'this' pointer alias
                if let Some(class_sym) = sn.opt_symbol() {
                    if let Some(this_sym) =
                        scopes.lookup_symbol("this", SymKindSet::from_kind(SymKind::Variable))
                    {
                        this_sym.set_type_of(class_sym.id());
                    }
                }
            })),
        );

        // Process base class clause for inheritance
        if let Some(class_sym) = sn.opt_symbol() {
            for child_id in node.child_ids() {
                let child = unit.hir_node(*child_id);
                if child.kind_id() == LangCpp::base_class_clause {
                    // Find the base class type identifier
                    if let Some(type_ident) = child.find_ident(unit)
                        && let Some(base_sym) =
                            scopes.lookup_symbol(type_ident.name, SYM_KIND_TYPES)
                    {
                        class_sym.set_type_of(base_sym.id());
                        tracing::trace!(
                            "class {} extends {}",
                            class_sym.format(Some(unit.interner())),
                            base_sym.format(Some(unit.interner()))
                        );
                    }
                }
            }
        }
    }

    // Struct specifier
    fn visit_struct_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_class_specifier(unit, node, scopes, namespace, parent);
    }

    // Union specifier
    fn visit_union_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_class_specifier(unit, node, scopes, namespace, parent);
    }

    // Enum specifier
    fn visit_enum_specifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None);
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    // Function definition
    fn visit_function_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let Some(sn) = node.as_scope() else {
            self.visit_children(unit, node, scopes, namespace, parent);
            return;
        };

        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, None);

        // Bind return type
        if let Some(fn_sym) = sn.opt_symbol() {
            if let Some(type_node) = node.child_by_field(unit, LangCpp::field_type)
                && let Some(return_type) = infer_type(unit, scopes, &type_node)
            {
                fn_sym.set_type_of(return_type.id());
                tracing::trace!(
                    "binding function return type '{}' to '{}'",
                    return_type.format(Some(unit.interner())),
                    fn_sym.format(Some(unit.interner()))
                );
            }
        }
    }

    // Field declaration
    fn visit_field_declaration(
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

    // Parameter declaration
    fn visit_parameter_declaration(
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

    // Compound statement (block)
    fn visit_compound_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let depth = scopes.scope_depth();
        if let Some(sn) = node.as_scope() {
            scopes.push_scope_node(sn);
        }
        self.visit_children(unit, node, scopes, scopes.top(), parent);
        scopes.pop_until(depth);
    }

    // Qualified identifier (namespace::name)
    fn visit_qualified_identifier(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Process the scope part first
        if let Some(scope_node) = node.child_by_field(unit, LangCpp::field_scope) {
            self.visit_auto(unit, &scope_node, scopes, namespace, parent);

            // Try to get the scope's symbol and push it
            if let Some(scope_ident) = scope_node.find_ident(unit)
                && let Some(scope_sym) = scope_ident.opt_symbol()
                && let Some(scope_id) = scope_sym.opt_scope()
            {
                let depth = scopes.scope_depth();
                scopes.push_scope(scope_id);

                // Now resolve the name part in the scope context
                if let Some(name_ident) = node.ident_by_field(unit, LangCpp::field_name) {
                    if let Some(sym) = scopes.lookup_symbol(name_ident.name, SYM_KIND_ALL) {
                        name_ident.set_symbol(sym);
                    }
                }

                scopes.pop_until(depth);
                return;
            }
        }

        // Fall back to visiting children normally
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Call expression
    fn visit_call_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Visit the function part first
        if let Some(func_node) = node.child_by_field(unit, LangCpp::field_function) {
            self.visit_auto(unit, &func_node, scopes, namespace, parent);
        }

        // Visit arguments
        if let Some(args_node) = node.child_by_field(unit, LangCpp::field_arguments) {
            self.visit_auto(unit, &args_node, scopes, namespace, parent);
        }
    }

    // Field expression (obj.field or obj->field)
    fn visit_field_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Visit the argument (the object) first
        if let Some(arg_node) = node.child_by_field(unit, LangCpp::field_argument) {
            self.visit_auto(unit, &arg_node, scopes, namespace, parent);

            // Try to get the type of the object and look up the field in that type's scope
            if let Some(arg_type) = infer_type(unit, scopes, &arg_node)
                && let Some(type_scope) = arg_type.opt_scope()
            {
                let depth = scopes.scope_depth();
                scopes.push_scope(type_scope);

                if let Some(field_node) = node.child_by_field(unit, LangCpp::field_field) {
                    if let Some(field_ident) = field_node.find_ident(unit) {
                        if let Some(sym) = scopes.lookup_symbol(field_ident.name, SYM_KIND_ALL) {
                            field_ident.set_symbol(sym);
                        }
                    }
                }

                scopes.pop_until(depth);
                return;
            }
        }

        // Fall back to normal child visiting
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    // Template declaration
    fn visit_template_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Templates create a new scope for their type parameters
        let depth = scopes.scope_depth();

        // Visit template parameters
        if let Some(params_node) = node
            .children(unit)
            .iter()
            .find(|c| c.kind_id() == LangCpp::template_parameter_list)
        {
            self.visit_auto(unit, params_node, scopes, namespace, parent);
        }

        // Visit the templated declaration
        self.visit_children(unit, node, scopes, namespace, parent);

        scopes.pop_until(depth);
    }

    // Lambda expression
    fn visit_lambda_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let depth = scopes.scope_depth();
        if let Some(sn) = node.as_scope() {
            scopes.push_scope_node(sn);
        }
        self.visit_children(unit, node, scopes, scopes.top(), parent);
        scopes.pop_until(depth);
    }
}

/// Entry point for symbol binding
pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    config: &ResolverOption,
) {
    let mut scopes = BinderScopes::new(unit, namespace);
    let mut visitor = BinderVisitor::new(config.clone());
    visitor.visit_node(&unit, node, &mut scopes, namespace, None);
}
