use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{Symbol, SymbolKind};
use llmcc_resolver::CollectorCore;

use crate::token::AstVisitorRust;
use crate::token::LangRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

#[derive(Debug)]
pub struct DeclVisitor<'tcx> {
    unit: CompileUnit<'tcx>,
}

impl<'tcx> DeclVisitor<'tcx> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self { unit }
    }

    fn visit_named_scope<F>(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        kind: SymbolKind,
        mut visit_fn: F,
    ) where
        F: FnMut(&mut Self, &mut CollectorCore<'tcx>, &Symbol),
    {
        if let Some(name_ident) = node.opt_child_by_field(self.unit(), LangRust::field_name) {
            if let Some(name_text) = name_ident.as_ident() {
                if let Some(symbol) = core.lookup_or_insert(&name_text.name, node.id(), kind) {
                    symbol.add_defining(node.id());
                    core.push_scope_with(node.id(), Some(symbol));
                    visit_fn(self, core, &symbol);
                    core.pop_scope();
                }
            }
        }
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
                && let Some(symbol) =
                    core.lookup_or_insert_global(&crate_name, node.id(), SymbolKind::Module)
            {
                core.push_scope_with(node.id(), Some(symbol));
            }

            if let Some(module_name) = parse_module_name(&file_path)
                && let Some(symbol) =
                    core.lookup_or_insert_global(&module_name, node.id(), SymbolKind::Module)
            {
                core.push_scope_with(node.id(), Some(symbol));
            }

            if let Some(file_name) = parse_file_name(&file_path)
                && let Some(symbol) =
                    core.lookup_or_insert(&file_name, node.id(), SymbolKind::Module)
            {
                core.push_scope_with(node.id(), Some(symbol));
            }
        }
    }

    fn visit_mod_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // For module items (mod my_module { ... } or mod my_module;)
        self.visit_named_scope(node, core, SymbolKind::Module, |visitor, core, symbol| {
            visitor.visit_children(&node, core, core.top_scope(), Some(symbol));
        });
    }

    fn visit_function_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // For function definitions (fn my_func(x: i32) -> i32 { ... })
        self.visit_named_scope(node, core, SymbolKind::Function, |visitor, core, symbol| {
            visitor.visit_children(&node, core, core.top_scope(), Some(symbol));
        });
    }

    fn visit_struct_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // For struct definitions (struct MyStruct { field: Type } or struct MyStruct;)
        self.visit_named_scope(node, core, SymbolKind::Struct, |visitor, core, symbol| {
            visitor.visit_children(&node, core, core.top_scope(), Some(symbol));
        });
    }

    fn visit_enum_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // For enum definitions (enum MyEnum { Variant1, Variant2 })
        self.visit_named_scope(node, core, SymbolKind::Enum, |visitor, core, symbol| {
            visitor.visit_children(&node, core, core.top_scope(), Some(symbol));
        });
    }

    fn visit_trait_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // For trait definitions (trait MyTrait { fn method(&self) {} })
        self.visit_named_scope(node, core, SymbolKind::Trait, |visitor, core, symbol| {
            visitor.visit_children(&node, core, core.top_scope(), Some(symbol));
        });
    }

    fn visit_impl_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // For impl blocks (impl MyStruct { fn method(&self) {} })
        // The impl itself is named after the type being implemented
        if let Some(type_node) = node.opt_child_by_field(self.unit(), LangRust::field_type) {
            if let Some(type_ident) = type_node.as_ident() {
                if let Some(symbol) =
                    core.lookup_or_insert(&type_ident.name, node.id(), SymbolKind::Impl)
                {
                    symbol.add_defining(node.id());
                    core.push_scope_with(node.id(), Some(symbol));
                    self.visit_children(&node, core, core.top_scope(), Some(symbol));
                    core.pop_scope();
                }
            }
        }
    }

    fn visit_type_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // For type aliases (type MyType = i32;)
        if let Some(name_ident) = node.opt_child_by_field(self.unit(), LangRust::field_name) {
            if let Some(name_text) = name_ident.as_ident() {
                if let Some(symbol) =
                    core.lookup_or_insert(&name_text.name, node.id(), SymbolKind::Struct)
                {
                    symbol.add_defining(node.id());
                }
            }
        }
    }

    fn visit_const_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // For const declarations (const MY_CONST: i32 = 42;)
        if let Some(name_ident) = node.opt_child_by_field(self.unit(), LangRust::field_name) {
            if let Some(name_text) = name_ident.as_ident() {
                if let Some(symbol) =
                    core.lookup_or_insert(&name_text.name, node.id(), SymbolKind::Const)
                {
                    symbol.add_defining(node.id());
                }
            }
        }
    }

    fn visit_static_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // For static declarations (static MY_STATIC: i32 = 42;)
        if let Some(name_ident) = node.opt_child_by_field(self.unit(), LangRust::field_name) {
            if let Some(name_text) = name_ident.as_ident() {
                if let Some(symbol) =
                    core.lookup_or_insert(&name_text.name, node.id(), SymbolKind::Const)
                {
                    symbol.add_defining(node.id());
                }
            }
        }
    }

    fn visit_field_declaration(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut CollectorCore<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // For struct field declarations (field_name: Type)
        if let Some(name_ident) = node.opt_child_by_field(self.unit(), LangRust::field_name) {
            if let Some(name_text) = name_ident.as_ident() {
                if let Some(symbol) =
                    core.lookup_or_insert(&name_text.name, node.id(), SymbolKind::Field)
                {
                    symbol.add_defining(node.id());
                }
            }
        }
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
        let mut core = CollectorCore::new(0, &cc.arena, &cc.interner, globlas);
        let mut visitor = DeclVisitor::new(unit);
        visitor.visit_node(node, &mut core, globlas, None);

        // Verify node is the source file by checking the HIR kind
        assert_eq!(node.kind(), llmcc_core::ir::HirKind::File);
    }
}
