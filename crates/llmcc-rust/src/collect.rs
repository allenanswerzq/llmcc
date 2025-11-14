use crate::token::AstVisitorRust;
use crate::util::{parse_crate_name, parse_module_name};
use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::scope::Scope;
use llmcc_core::symbol::Symbol;
use llmcc_resolver::CollectorCore;

#[derive(Debug)]
pub struct DeclVisitor<'tcx> {
    unit: CompileUnit<'tcx>,
}

impl<'tcx> DeclVisitor<'tcx> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self { unit }
    }
}

impl<'tcx> AstVisitorRust<'tcx, CollectorCore<'tcx>> for DeclVisitor<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(file_path) = self.unit().file_path() {
            if let Some(crate_name) = parse_crate_name(&file_path)
                && let Some(symbol) = core.lookup_or_insert_global(&crate_name, node.id())
            {
                core.push_scope_with(node.id(), symbol);
            }
            if let Some(module_name) = parse_module_name(&file_path) {
                if let Some(symbol) = core.lookup_or_insert_global(&module_name, node.id()) {
                    core.push_scope_with(node.id(), symbol);
                }
            }

            // var sn = (AstNodeFile)node;
            // sn.Name = node.FindIdentifier();
            // var moduleName = Path.GetFileNameWithoutExtension(node.Context.File.Path);
            // if (moduleName == "mod") {
            //     moduleName = Path.GetFileName(Path.GetDirectoryName(node.Context.File.Path));
            // }
            // else if (moduleName == "lib") {
            //     break;
            // }
            // sn.Name = new AstNodeId(node.Context, AstFieldRust.None, AstTokenRust.source_file, sn.Begrc, sn.Endrc, moduleName, AstCategory.IdentifierDef);
            // sn.Name.Symbol = scopes.FindOrAdd(sn.Name);
            // sn.Name.Symbol.AddDefined(node);

            // if (sn.Name.Symbol.Scope != null && sn.Name.Symbol.Scope.Root == null) {
            //     sn.Name.Symbol.Scope.SetRoot(sn);
            // }
            // else if (sn.Name.Symbol.Scope == null) {
            //     sn.Name.Symbol.Scope = new AstScope(sn, sn.Name.Symbol);
            // }

            // sn.Scope = sn.Name.Symbol.Scope;
            // scopes.Push(sn.Scope);
        }
    }

    fn visit_mod_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
    }

    fn visit_function_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
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

        let mut core = CollectorCore::new(0, &cc.arena, &cc.interner);
        let globlas = cc.create_globals();
        let mut visitor = DeclVisitor::new(unit);
        visitor.visit_node(node, &mut core, globlas, None);

        // Verify node is the source file by checking the HIR kind
        assert_eq!(node.kind(), llmcc_core::ir::HirKind::File);
    }
}
