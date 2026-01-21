#![allow(clippy::collapsible_if, clippy::needless_return)]

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirNode, HirScope};
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SYM_KIND_ALL, SymKindSet, Symbol};
use llmcc_resolver::{BinderScopes, ResolverOption};

use crate::token::AstVisitorPython;

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
            && let Some(symbol) = scopes.lookup_symbol(package_name, SymKindSet::from_kind(llmcc_core::symbol::SymKind::Crate))
            && let Some(scope_id) = symbol.opt_scope()
        {
            scopes.push_scope(scope_id);
        }

        if let Some(ref module_name) = meta.module_name
            && let Some(symbol) = scopes.lookup_symbol(module_name, SymKindSet::from_kind(llmcc_core::symbol::SymKind::Module))
            && let Some(scope_id) = symbol.opt_scope()
        {
            scopes.push_scope(scope_id);
        }

        if let Some(ref file_name) = meta.file_name
            && let Some(file_sym) = scopes.lookup_symbol(file_name, SymKindSet::from_kind(llmcc_core::symbol::SymKind::File))
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

    fn visit_class_definition(
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
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
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
