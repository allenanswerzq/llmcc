use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::next_hir_id;
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::CollectorScopes;

use crate::LangRust;
use crate::token::AstVisitorRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

#[derive(Debug)]
pub struct CollectorVisitor<'tcx> {
    phantom: std::marker::PhantomData<&'tcx ()>,
}

impl<'tcx> CollectorVisitor<'tcx> {
    fn new() -> Self {
        Self {
            phantom: std::marker::PhantomData,
        }
    }

    fn declare_symbol_from_ident(
        &self,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        ident: &'tcx llmcc_core::ir::HirIdent<'tcx>,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let symbol = scopes.lookup_or_insert(&ident.name, node, kind)?;
        ident.set_symbol(symbol);
        symbol.add_defining(node.id());
        Some(symbol)
    }

    fn declare_symbol_from_field(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        let ident_id = node.find_identifier_for_field(*unit, field_id)?;
        let ident = unit.hir_node(ident_id).as_ident()?;
        self.declare_symbol_from_ident(node, scopes, ident, kind)
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_scoped_named(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
        kind: SymKind,
        field_id: u16,
    ) {
        let _ = namespace;
        let _ = parent;

        if let Some(sn) = node.as_scope()
            && let Some(id) = node.find_identifier_for_field(*unit, field_id)
        {
            let ident = unit.hir_node(id).as_ident().unwrap();
            if let Some(sym) = scopes.lookup_or_insert(&ident.name, node, kind) {
                ident.set_symbol(sym);
                sn.set_ident(ident);

                let scope = unit.cc.alloc_hir_scope(next_hir_id(), sym);
                sym.set_scope(scope.id());
                sym.add_defining(node.id());
                sn.set_scope(scope);

                scopes.push_scope(scope);
                self.visit_children(unit, node, scopes, scope, Some(sym));
                scopes.pop_scope();
            }
        }
    }
}

impl<'tcx> AstVisitorRust<'tcx, CollectorScopes<'tcx>> for CollectorVisitor<'tcx> {
    #[rustfmt::skip]
    fn visit_source_file(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let file_path = unit.file_path().expect("no file path found to compile");
        let start_depth = scopes.scope_depth();

        if let Some(crate_name) = parse_crate_name(file_path)
            && let Some(symbol) = scopes.lookup_or_insert_global(&crate_name, node, SymKind::Crate)
        {
            scopes.push_scope_with(node, Some(symbol));
        }

        if let Some(module_name) = parse_module_name(file_path) {
            if let Some(symbol) =
                scopes.lookup_or_insert_global(&module_name, node, SymKind::Module)
            {
                scopes.push_scope_with(node, Some(symbol));
            }
        }

        if let Some(file_name) = parse_file_name(file_path)
            && let Some(sn) = node.as_scope()
        {
            if let Some(module_symbol) = scopes
                .lookup_or_insert(&file_name, node, SymKind::File)
            {
                let ident = unit.cc.alloc_hir_ident(next_hir_id(), file_name.clone(), module_symbol);
                sn.set_ident(ident);
                ident.set_symbol(module_symbol);
                module_symbol.add_defining(node.id());

                let scope = unit.cc.alloc_hir_scope(next_hir_id(), module_symbol);
                module_symbol.set_scope(scope.id());
                sn.set_scope(scope);
                scopes.push_scope(scope);
            }
        }

        self.visit_children(unit, node, scopes, namespace, parent);
        scopes.pop_until(start_depth);
    }

    fn visit_mod_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if node.child_by_field(*unit, LangRust::field_body).is_none() {
            return;
        }
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Namespace,
            LangRust::field_name,
        );
    }

    fn visit_function_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Function,
            LangRust::field_name,
        );
    }

    fn visit_function_signature_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = self.declare_symbol_from_field(
            unit,
            node,
            scopes,
            SymKind::Function,
            LangRust::field_name,
        ) {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_struct_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Struct,
            LangRust::field_name,
        );
    }

    fn visit_enum_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Enum,
            LangRust::field_name,
        );
    }

    fn visit_trait_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Trait,
            LangRust::field_name,
        );
    }

    fn visit_impl_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        let _ = parent;
        let Some(sn) = node.as_scope() else {
            return;
        };

        let impl_name = node
            .find_identifier_for_field(*unit, LangRust::field_type)
            .and_then(|id| unit.hir_node(id).as_ident().map(|ident| ident.name.clone()))
            .map(|name| format!("impl {}", name))
            .unwrap_or_else(|| format!("impl#{}", node.id().0));

        if let Some(symbol) = scopes.lookup_or_insert(&impl_name, node, SymKind::Impl) {
            symbol.add_defining(node.id());
            let ident = unit
                .cc
                .alloc_hir_ident(next_hir_id(), impl_name.clone(), symbol);
            sn.set_ident(ident);

            let scope = unit.cc.alloc_hir_scope(next_hir_id(), symbol);
            symbol.set_scope(scope.id());
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
            scopes.pop_scope();
        }
    }

    fn visit_macro_definition(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            unit,
            node,
            scopes,
            namespace,
            parent,
            SymKind::Macro,
            LangRust::field_name,
        );
    }

    fn visit_const_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) =
            self.declare_symbol_from_field(unit, node, scopes, SymKind::Const, LangRust::field_name)
        {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_static_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = self.declare_symbol_from_field(
            unit,
            node,
            scopes,
            SymKind::Static,
            LangRust::field_name,
        ) {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_type_item(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.declare_symbol_from_field(unit, node, scopes, SymKind::Type, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_type_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.declare_symbol_from_field(unit, node, scopes, SymKind::Type, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_associated_type(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.declare_symbol_from_field(unit, node, scopes, SymKind::Type, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_field_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.declare_symbol_from_field(unit, node, scopes, SymKind::Field, LangRust::field_name);
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_enum_variant(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(ident) = node.as_ident() {
            if let Some(symbol) = scopes.lookup_or_insert(&ident.name, node, SymKind::EnumVariant) {
                ident.set_symbol(symbol);
                symbol.add_defining(node.id());
                self.visit_children(unit, node, scopes, namespace, Some(symbol));
                return;
            }
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

    fn visit_parameter(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(symbol) = self.declare_symbol_from_field(
            unit,
            node,
            scopes,
            SymKind::Variable,
            LangRust::field_pattern,
        ) {
            self.visit_children(unit, node, scopes, namespace, Some(symbol));
        } else {
            self.visit_children(unit, node, scopes, namespace, parent);
        }
    }

    fn visit_let_declaration(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(pattern_id) = node.find_identifier_for_field(*unit, LangRust::field_pattern) {
            if let Some(ident) = unit.hir_node(pattern_id).as_ident() {
                if let Some(symbol) =
                    scopes.lookup_or_insert_chained(&ident.name, node, SymKind::Variable)
                {
                    ident.set_symbol(symbol);
                    symbol.add_defining(node.id());
                }
            }
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }
}

pub fn collect_symbols<'tcx>(
    unit: &CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scopes: &mut CollectorScopes<'tcx>,
    namespace: &'tcx Scope<'tcx>,
) {
    CollectorVisitor::new().visit_node(unit, node, scopes, namespace, None);
}

#[cfg(test)]
mod tests {
    use crate::token::LangRust;

    use llmcc_core::context::CompileCtxt;
    use llmcc_core::ir_builder::{IrBuildOption, build_llmcc_ir};
    use llmcc_resolver::{BinderOption, CollectorOption, bind_symbols_with, collect_symbols_with};

    fn compile_from_soruces(sources: Vec<Vec<u8>>) {
        let cc = CompileCtxt::from_sources::<LangRust>(&sources);
        build_llmcc_ir::<LangRust>(&cc, IrBuildOption).unwrap();

        let globals =
            collect_symbols_with::<LangRust>(&cc, CollectorOption::default().with_print_ir(true));
        bind_symbols_with::<LangRust>(&cc, globals, BinderOption);
    }

    #[test]
    fn test_decl_visitor() {
        let foo = br#"
mod outer {
    fn nested_function() {}

    const INNER_CONST: i32 = 1;
}

fn top_level() {}
struct Foo {
    field: i32,
}

enum Color {
    Red,
}

trait Greeter {
    fn greet(&self);
}

impl Foo {
    fn method(&self) {}
}

type Alias = Foo;
const TOP_CONST: i32 = 42;
static TOP_STATIC: i32 = 7;
"#;
        let bar = br#"
mod outer {
    fn nested_function() {}
}
    "#;

        compile_from_soruces(vec![foo.to_vec(), bar.to_vec()]);
    }
}
