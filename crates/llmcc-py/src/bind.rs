#![allow(clippy::collapsible_if, clippy::needless_return)]

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SYM_KIND_ALL, SYM_KIND_TYPES, SymKind, SymKindSet, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::infer::infer_type;
use crate::token::AstVisitorPython;
use crate::token::LangPython;
use crate::util::{parse_module_name, parse_package_name};

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
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
        on_scope_enter: Option<ScopeEnterFn<'tcx>>,
    ) {
        tracing::trace!(
            "visiting scoped named node kind: {:?}, namespace id: {:?}, parent: {:?}",
            node.kind_id(),
            namespace.id(),
            parent.map(|p| p.format(Some(unit.interner()))),
        );
        let depth = scopes.scope_depth();

        // Python uses naming conventions for visibility:
        // - Names not starting with _ are public
        // - Names starting with _ are private (convention)
        // - Names starting with __ are name-mangled (stronger privacy)
        if let Some(sym) = sn.opt_symbol() {
            let name = unit.resolve_name(sym.name);
            if !name.starts_with('_') || (name.starts_with("__") && name.ends_with("__")) {
                sym.set_is_global(true);
                scopes.globals().insert(sym);
            }
        }

        let child_parent = sn.opt_symbol().or(parent);
        if !scopes.push_scope_node(sn) {
            // Scope wasn't set in collector, fall back to parent namespace
            self.visit_children(unit, node, scopes, namespace, child_parent);
            return;
        }
        if let Some(scope_enter) = on_scope_enter {
            scope_enter(unit, sn, scopes);
        }
        self.visit_children(unit, node, scopes, scopes.top(), child_parent);
        scopes.pop_until(depth);
    }
}

impl<'tcx> AstVisitorPython<'tcx, BinderScopes<'tcx>> for BinderVisitor<'tcx> {
    // AST: module (source file)
    fn visit_module(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let file_path = unit.file_path().unwrap_or("unknown");
        let depth = scopes.scope_depth();

        tracing::trace!("binding module: {}", file_path);

        // Push package scope if applicable
        if let Some(package_name) = parse_package_name(file_path)
            && let Some(symbol) =
                scopes.lookup_symbol(&package_name, SymKindSet::from_kind(SymKind::Module))
            && let Some(scope_id) = symbol.opt_scope()
        {
            tracing::trace!("pushing package scope {:?}", scope_id);
            scopes.push_scope(scope_id);
        }

        // Push module scope if applicable
        if let Some(module_name) = parse_module_name(file_path)
            && let Some(symbol) =
                scopes.lookup_symbol(&module_name, SymKindSet::from_kind(SymKind::Module))
            && let Some(scope_id) = symbol.opt_scope()
        {
            tracing::trace!("pushing module scope {:?}", scope_id);
            scopes.push_scope(scope_id);
        }

        // Push file scope
        let file_name = std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("_file");

        if let Some(file_sym) =
            scopes.lookup_symbol(file_name, SymKindSet::from_kind(SymKind::File))
            && let Some(scope_id) = file_sym.opt_scope()
        {
            tracing::trace!("pushing file scope {} {:?}", file_path, scope_id);
            scopes.push_scope(scope_id);

            let file_scope = unit.get_scope(scope_id);
            self.visit_children(unit, node, scopes, file_scope, Some(file_sym));
            scopes.pop_until(depth);
        }
    }

    // AST: identifier
    #[tracing::instrument(skip_all)]
    fn visit_identifier(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let Some(ident) = node.as_ident() else {
            return;
        };

        // Skip if already resolved
        if let Some(existing) = ident.opt_symbol()
            && existing.kind().is_resolved()
        {
            return;
        }

        // Try to resolve the identifier in current scopes
        if let Some(symbol) = scopes.lookup_symbol(&ident.name, SYM_KIND_ALL) {
            ident.set_symbol(symbol);
        }
    }

    // AST: function_definition
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

        // Bind return type annotation (e.g., `def foo() -> int:`)
        let Some(fn_sym) = sn.opt_symbol() else {
            return;
        };

        if let Some(return_type_node) = node.child_by_field(unit, LangPython::field_return_type) {
            if let Some(return_sym) = infer_type(unit, scopes, &return_type_node) {
                tracing::trace!(
                    "binding function return type '{}' to '{}'",
                    return_sym.format(Some(unit.interner())),
                    fn_sym.format(Some(unit.interner()))
                );
                fn_sym.set_type_of(return_sym.id());
            }
        }
    }

    // AST: class_definition
    fn visit_class_definition(
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

        // Handle superclasses
        let on_scope_enter: Option<ScopeEnterFn<'tcx>> =
            if let Some(superclasses) = node.child_by_field(unit, LangPython::field_superclasses) {
                Some(Box::new(move |unit, _sn, scopes| {
                    // Resolve each superclass
                    for &child_id in superclasses.child_ids() {
                        let child = unit.hir_node(child_id);
                        if let Some(ident) = child.find_ident(unit) {
                            if let Some(sym) = scopes.lookup_symbol(&ident.name, SYM_KIND_TYPES) {
                                ident.set_symbol(sym);
                            }
                        }
                    }
                }))
            } else {
                None
            };

        self.visit_scoped_named(unit, node, sn, scopes, namespace, parent, on_scope_enter);
    }

    // AST: attribute access (e.g., obj.attr)
    fn visit_attribute(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // First, visit the object part to resolve it
        if let Some(object_node) = node.child_by_field(unit, LangPython::field_object) {
            self.visit_node(unit, &object_node, scopes, namespace, parent);

            // Try to infer the type of the object
            if let Some(obj_type) = infer_type(unit, scopes, &object_node) {
                // If we know the object's type, try to resolve the attribute in that type's scope
                if let Some(attr_node) = node.child_by_field(unit, LangPython::field_attribute) {
                    if let Some(attr_ident) = attr_node.as_ident() {
                        if let Some(attr_sym) = scopes.lookup_member_symbol(
                            obj_type,
                            &attr_ident.name,
                            None,
                        ) {
                            attr_ident.set_symbol(attr_sym);
                        }
                    }
                }
            }
        }
    }

    // AST: call expression
    fn visit_call(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Visit the function being called
        if let Some(func_node) = node.child_by_field(unit, LangPython::field_function) {
            self.visit_node(unit, &func_node, scopes, namespace, parent);
        }

        // Visit arguments
        if let Some(args_node) = node.child_by_field(unit, LangPython::field_arguments) {
            self.visit_node(unit, &args_node, scopes, namespace, parent);
        }
    }

    // AST: lambda
    fn visit_lambda(
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
    }

    // AST: assignment
    fn visit_assignment(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Visit the right side first to resolve any references
        if let Some(right_node) = node.child_by_field(unit, LangPython::field_right) {
            self.visit_node(unit, &right_node, scopes, namespace, parent);

            // Try to infer type from right side and assign to left side variables
            if let Some(value_type) = infer_type(unit, scopes, &right_node) {
                if let Some(left_node) = node.child_by_field(unit, LangPython::field_left) {
                    // For simple identifier assignment
                    if let Some(ident) = left_node.as_ident() {
                        if let Some(sym) = ident.opt_symbol() {
                            if sym.type_of().is_none() {
                                sym.set_type_of(value_type.id());
                            }
                        }
                    }
                }
            }
        }

        // Visit the left side
        if let Some(left_node) = node.child_by_field(unit, LangPython::field_left) {
            self.visit_node(unit, &left_node, scopes, namespace, parent);
        }

        // Handle type annotation if present
        if let Some(type_node) = node.child_by_field(unit, LangPython::field_type) {
            if let Some(type_sym) = infer_type(unit, scopes, &type_node) {
                if let Some(left_node) = node.child_by_field(unit, LangPython::field_left) {
                    if let Some(ident) = left_node.as_ident() {
                        if let Some(sym) = ident.opt_symbol() {
                            sym.set_type_of(type_sym.id());
                        }
                    }
                }
            }
        }
    }

    // AST: return_statement
    fn visit_return_statement(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Visit the return expression
        self.visit_children(unit, node, scopes, namespace, parent);

        // Try to infer return type for the enclosing function
        if let Some(parent_sym) = parent {
            if parent_sym.kind() == SymKind::Function || parent_sym.kind() == SymKind::Method {
                // Get the first child that's not a keyword
                for &child_id in node.child_ids() {
                    let child = unit.hir_node(child_id);
                    if let Some(ret_type) = infer_type(unit, scopes, &child) {
                        if parent_sym.type_of().is_none() {
                            parent_sym.set_type_of(ret_type.id());
                        }
                        break;
                    }
                }
            }
        }
    }

    // AST: parameters - bind parameter types
    fn visit_parameters(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        for &child_id in node.child_ids() {
            let child = unit.hir_node(child_id);
            match child.kind_id() {
                LangPython::typed_parameter | LangPython::typed_default_parameter => {
                    // Get the type annotation
                    if let Some(type_node) = child.child_by_field(unit, LangPython::field_type) {
                        if let Some(type_sym) = infer_type(unit, scopes, &type_node) {
                            // Find the parameter name and set its type
                            if let Some(name_node) = child.child_by_field(unit, LangPython::field_name) {
                                if let Some(ident) = name_node.as_ident() {
                                    if let Some(sym) = ident.opt_symbol() {
                                        sym.set_type_of(type_sym.id());
                                    }
                                }
                            } else {
                                // For typed_parameter, the first identifier is the name
                                for &sub_id in child.child_ids() {
                                    let sub = unit.hir_node(sub_id);
                                    if sub.kind_id() == LangPython::identifier {
                                        if let Some(ident) = sub.as_ident() {
                                            if let Some(sym) = ident.opt_symbol() {
                                                sym.set_type_of(type_sym.id());
                                            }
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            self.visit_node(unit, &child, scopes, namespace, parent);
        }
    }
}

/// Entry point for symbol binding
pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    globals: &'tcx Scope<'tcx>,
    config: &ResolverOption,
) {
    let mut scopes = BinderScopes::new(unit, globals);
    let mut visitor = BinderVisitor::new(config.clone());
    visitor.visit_node(&unit, node, &mut scopes, globals, None);
}
