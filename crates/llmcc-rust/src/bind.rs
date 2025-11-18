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
    phantom: std::marker::PhantomData<&'tcx ()>,
}

impl<'tcx> BinderVisitor<'tcx> {
    fn new() -> Self {
        Self {
            phantom: std::marker::PhantomData,
        }
    }
}

impl<'tcx> AstVisitorRust<'tcx, BinderScopes<'tcx>> for BinderVisitor<'tcx> {
    fn visit_source_file(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let file_path = unit.file_path().expect("no file path found to compile");

        // Process crate scope
        if let Some(scope_id) = parse_crate_name(file_path)
            .and_then(|crate_name| {
                scopes.lookup_or_insert_global(&crate_name, node, SymKind::Crate)
            })
            .and_then(|symbol| symbol.scope())
        {
            scopes.push_scope(scope_id);
        }

        // Process module and file scopes
        if let Some((_file_name, scope_id)) = parse_module_name(file_path)
            .and_then(|module_name| {
                scopes
                    .lookup_or_insert_global(&module_name, node, SymKind::Module)
                    .and_then(|symbol| symbol.scope())
                    .inspect(|&scope_id| {
                        scopes.push_scope(scope_id);
                    })
            })
            .and_then(|_| {
                parse_file_name(file_path).and_then(|file_name| {
                    scopes
                        .lookup_or_insert(&file_name, node, SymKind::File)
                        .and_then(|file_sym| file_sym.scope())
                        .map(|scope_id| (file_name, scope_id))
                })
            })
        {
            scopes.push_scope(scope_id);
            self.visit_children(unit, node, scopes, namespace, parent);
            scopes.pop_scope();
        }
    }

    fn visit_mod_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        if node.child_by_field(*unit, LangRust::field_body).is_none() {
            return;
        }

        let sn = node.as_scope().unwrap();
        if let Some(_ident) = sn.opt_ident() {
            scopes.push_scope_recursive(sn.scope().id());
        } else {
            scopes.push_scope(sn.scope().id());
        }
    }

    fn visit_function_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut BinderScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Process return type if present
        if let Some(ret_ty) = node.child_by_field(*unit, LangRust::field_return_type) {
            self.visit_node(unit, &ret_ty, scopes, namespace, parent);
        }

        // Get the scope node
        let sn = node.as_scope().unwrap();

        // Find or create symbol for the return type
        let ty = node
            .find_identifier_for_field(*unit, LangRust::field_return_type)
            .and_then(|ty_id| {
                let ty_node = unit.hir_node(ty_id);
                scopes.lookup_or_insert_global(
                    &ty_node.as_ident().unwrap().name,
                    &ty_node,
                    SymKind::Type,
                )
            })
            .unwrap_or_else(|| {
                // Default to void/unit type if no return type found
                scopes
                    .lookup_or_insert_global("void_fn", node, SymKind::Type)
                    .expect("void_fn type should exist")
            });

        // Set the return type for the function symbol
        if let Some(ident) = sn.opt_ident() {
            let func_sym = scopes.lookup_or_insert_global(&ident.name, node, SymKind::Function);
            if let Some(func_sym) = func_sym {
                // debug_assert_eq!(func_sym.id(), ident.symbol().id);
                if func_sym.type_of().is_none() {
                    func_sym.set_type_of(ty.id());
                }
                scopes.push_scope_recursive(sn.scope().id());
            }
        } else {
            scopes.push_scope(sn.scope().id());
        }
    }

    fn visit_struct_item(
        &mut self,
        _unit: &CompileUnit<'tcx>,
        _node: &HirNode<'tcx>,
        _scopes: &mut BinderScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
    }
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scopes: &mut BinderScopes<'tcx>,
    namespace: &'tcx Scope<'tcx>,
) {
    BinderVisitor::new().visit_node(&unit, node, scopes, namespace, None);
}
