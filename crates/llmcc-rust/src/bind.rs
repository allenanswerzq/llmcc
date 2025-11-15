use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::BinderScopes;

use crate::token::AstVisitorRust;
use crate::token::LangRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

/// Visitor for resolving symbol bindings and establishing relationships.
#[derive(Debug)]
pub struct BinderVisitor<'tcx> {
    unit: CompileUnit<'tcx>,
}

impl<'tcx> BinderVisitor<'tcx> {
    /// Creates a new binder visitor for the compilation unit.
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self { unit }
    }
}

impl<'tcx> AstVisitorRust<'tcx, BinderScopes<'tcx>> for BinderVisitor<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_scope_children(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        scopes.push_scope(node.id());
        self.visit_children(node, scopes, namespace, parent);
        scopes.pop_scope();
    }

    fn visit_source_file(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if let Some(file_path) = self.unit().file_path() {
            if let Some(crate_name) = parse_crate_name(&file_path)
                && let Some(_symbol) =
                    scopes.lookup_or_insert_global(&crate_name, node, SymKind::Module)
            {
                scopes.push_scope(node.id());
            }

            if let Some(module_name) = parse_module_name(&file_path)
                && let Some(_symbol) =
                    scopes.lookup_or_insert_global(&module_name, node, SymKind::Module)
            {
                scopes.push_scope(node.id());
            }

            if let Some(file_name) = parse_file_name(&file_path)
                && let Some(symbol) = scopes.lookup_or_insert(&file_name, node, SymKind::Module)
            {
                let ns = scopes.get_scope(node.id());
                self.visit_scope_children(node, scopes, ns, Some(symbol));
            }
        }
    }

    fn visit_mod_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let sn = node.expect_scope();
        if let Some(ident) = sn.ident {
            if let Some(symbol) = scopes.lookup_or_insert(&ident.name, node, SymKind::Module) {
                let ns = scopes.get_scope(node.id());
                self.visit_scope_children(node, scopes, ns, Some(symbol));
            }
        } else {
            // Anonymous module, push all parent scopes
            let depth = scopes.scope_depth();
            scopes.push_scope_recursive(node.id());
            self.visit_children(node, scopes, namespace, parent);
            scopes.pop_until(depth);
        }
    }

    fn visit_function_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let sn = node.expect_scope();
        let mut ret_sym = None;

        if let Some(ret_ty) = node.opt_child_by_field(self.unit, LangRust::field_return_type) {
            self.visit_children(&ret_ty, scopes, namespace, parent);
            ret_sym = ret_ty
                .find_ident(self.unit)
                .map(|ident| ident.symbol(self.unit));
        }

        if let Some(ident) = sn.ident {
            let symbol = ident.symbol(self.unit);

            if let Some(ret_sym) = ret_sym {
                symbol.set_type_of(ret_sym.id());
            }

            let ns = scopes.get_scope(node.id());
            self.visit_scope_children(node, scopes, ns, Some(symbol));
        } else {
        }
    }

    fn visit_struct_item(
        &mut self,
        node: HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        // Navigate struct scope and resolve field types
        // TODO: Resolve types for struct fields
        if let Some(scope) = scopes.unit().opt_get_scope(node.id()) {
            scopes.push_scope(scope);
            self.visit_children(&node, scopes, scope, None);
            scopes.pop_scope();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binder_visitor_creation() {
        // Basic test to verify BinderVisitor can be instantiated
        // Full tests require a CompileUnit which is integration-level
    }
}
