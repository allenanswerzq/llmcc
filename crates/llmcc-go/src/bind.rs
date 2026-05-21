use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SYM_KIND_ALL, SYM_KIND_TYPES, SymKind, SymKindSet, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::LangGo;
use crate::token::AstVisitorGo;

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

    fn visit_scoped_named(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        sn: &'tcx HirScope<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let depth = scopes.scope_depth();
        let child_parent = sn.opt_symbol().or(parent);

        if !scopes.push_scope_node(sn) {
            self.visit_children(unit, node, scopes, scopes.top(), child_parent);
            return;
        }

        self.visit_children(unit, node, scopes, scopes.top(), child_parent);
        self.bind_return_type(unit, node, scopes);
        scopes.pop_until(depth);
    }

    fn bind_return_type(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
    ) {
        let Some(ret_node) = node.child_by_field(unit, LangGo::field_result) else {
            return;
        };
        let Some(ret_type) = crate::infer::infer_type(unit, scopes, &ret_node) else {
            return;
        };
        if let Some(func_sym) = node.ident_symbol_by_field(unit, LangGo::field_name) {
            func_sym.set_type_of(ret_type.id());
        }
    }

    fn bind_named_type(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
    ) {
        if let Some(field_sym) = node.ident_symbol_by_field(unit, LangGo::field_name) {
            if let Some(parent_sym) = namespace.opt_symbol()
                && matches!(field_sym.kind(), SymKind::Field)
            {
                field_sym.set_field_of(parent_sym.id());
            }

            if let Some(type_node) = node.child_by_field(unit, LangGo::field_type)
                && let Some(type_sym) = crate::infer::infer_type(unit, scopes, &type_node)
            {
                field_sym.set_type_of(type_sym.id());
            }
        }
    }
}

impl<'tcx> AstVisitorGo<'tcx, BinderScopes<'tcx>> for BinderVisitor<'tcx> {
    fn visit_source_file(
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
            && let Some(symbol) =
                scopes.lookup_symbol(package_name, SymKindSet::from_kind(SymKind::Crate))
            && let Some(scope_id) = symbol.opt_scope()
        {
            scopes.push_scope(scope_id);
        }

        if let Some(ref module_name) = meta.module_name
            && let Some(symbol) =
                scopes.lookup_symbol(module_name, SymKindSet::from_kind(SymKind::Module))
            && let Some(scope_id) = symbol.opt_scope()
        {
            scopes.push_scope(scope_id);
        }

        if let Some(scope_name) = crate::package_scope_name(meta)
            && let Some(symbol) =
                scopes.lookup_symbol(&scope_name, SymKindSet::from_kind(SymKind::Namespace))
            && let Some(scope_id) = symbol.opt_scope()
        {
            scopes.push_scope(scope_id);
        }

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

    fn visit_field_identifier(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        let ident = node.as_ident().unwrap();
        if ident.opt_symbol().is_some() {
            return;
        }
        if let Some(symbol) = scopes.lookup_symbol(ident.name, SYM_KIND_ALL) {
            ident.set_symbol(symbol);
        }
    }

    fn visit_type_spec(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, parent);
        }
    }

    fn visit_function_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, parent);
        }
    }

    fn visit_method_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            self.visit_scoped_named(unit, node, sn, scopes, parent);
        }
    }

    fn visit_parameter_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.bind_named_type(unit, node, scopes, namespace);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_field_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.bind_named_type(unit, node, scopes, namespace);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_var_spec(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.bind_named_type(unit, node, scopes, namespace);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_const_spec(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.bind_named_type(unit, node, scopes, namespace);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_call_expression(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(function_node) = node.child_by_field(unit, LangGo::field_function) {
            self.visit_node(unit, &function_node, scopes, namespace, parent);
        }
        self.visit_children(unit, node, scopes, namespace, parent);
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
