use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::CollectorScopes;

use crate::token::AstVisitorRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

#[derive(Debug)]
#[allow(dead_code)]
pub struct DeclVisitor<'tcx> {
    unit: CompileUnit<'tcx>,
}

#[allow(dead_code)]
impl<'tcx> DeclVisitor<'tcx> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self { unit }
    }
}

impl<'tcx> AstVisitorRust<'tcx, CollectorScopes<'tcx>> for DeclVisitor<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    #[rustfmt::skip]
    fn visit_source_file(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let file_path = self.unit.file_path().expect("no file path found to compile");

        if let Some(crate_name) = parse_crate_name(&file_path)
            && let Some(symbol) = scopes.lookup_or_insert_global(&crate_name, *node, SymKind::Module)
        {
            scopes.push_scope_with(*node, Some(symbol));
        }

        if let Some(module_name) = parse_module_name(&file_path)
            && let Some(symbol) = scopes.lookup_or_insert_global(&module_name, *node, SymKind::Module)
        {
            scopes.push_scope_with(*node, Some(symbol));
        }

        if let Some(file_name) = parse_file_name(&file_path)
            && let Some(symbol) = scopes.lookup_or_insert(&file_name, *node, SymKind::Module)
            && let Some(sn) = node.as_scope()
        {
            let ident = self.unit.alloc_hir_ident(file_name.clone(), symbol);
            sn.set_ident(ident);

            if let Some(file_sym) = scopes.lookup_or_insert(&file_name, *node, SymKind::File) {
                ident.set_symbol(file_sym);
                file_sym.add_defining(node.id());

                let scope = self.unit.alloc_hir_scope(file_sym);
                file_sym.set_scope(Some(scope.id()));
                sn.set_scope(scope);
                scopes.push_scope(scope);
            }

            self.visit_children(node, scopes, namespace, parent);
        }
    }

    #[allow(unused_variables)]
    fn visit_mod_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
    }

    #[allow(unused_variables)]
    fn visit_function_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // TODO: Implement function item collection logic
        let _sn = node.as_scope();
    }

    #[allow(unused_variables)]
    fn visit_struct_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
    }

    #[allow(unused_variables)]
    fn visit_enum_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
    }

    #[allow(unused_variables)]
    fn visit_trait_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
    }

    #[allow(unused_variables)]
    fn visit_impl_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
    }

    #[allow(unused_variables)]
    fn visit_type_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
    }

    #[allow(unused_variables)]
    fn visit_const_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
    }

    #[allow(unused_variables)]
    fn visit_static_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
    }

    #[allow(unused_variables)]
    fn visit_field_declaration(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::LangRust;
    use llmcc_core::context::CompileCtxt;
    use llmcc_core::ir_builder::{IrBuildConfig, build_llmcc_ir};

    #[test]
    fn test_decl_visitor() {
        // Test that we can traverse the source file
        let source_code = b"fn main() {}\n";
        let sources = vec![source_code.to_vec()];

        let cc = CompileCtxt::from_sources::<LangRust>(&sources);
        let config = IrBuildConfig::default();
        build_llmcc_ir::<LangRust>(&cc, config).unwrap();

        let unit = cc.compile_unit(0);
        let file_start = unit.file_start_hir_id().unwrap();
        let node = unit.hir_node(file_start);

        let globlas = cc.create_globals();
        let mut scopes = CollectorScopes::new(0, &cc.arena, &cc.interner, globlas);
        let mut v = DeclVisitor::new(unit);
        v.visit_node(node, &mut scopes, globlas, None);

        // Verify node is the source file by checking the HIR kind
        assert_eq!(node.kind(), llmcc_core::ir::HirKind::File);
    }
}
