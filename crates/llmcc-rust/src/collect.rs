use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::CollectorScopes;

use crate::LangRust;
use crate::token::AstVisitorRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

#[derive(Debug)]
pub struct DeclVisitor<'tcx> {
    unit: CompileUnit<'tcx>,
}

impl<'tcx> DeclVisitor<'tcx> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self { unit }
    }

    /// Helper to create a scoped named item (function, struct, enum, trait, module, etc.)
    /// This consolidates the common pattern for items that need to register an identifier
    /// and create a scope for their children.
    fn visit_scoped_named(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
        kind: SymKind,
        field_id: u16,
    ) {
        if let Some(sn) = node.as_scope() {
            if let Some(id) = node.find_identifier_for_field(self.unit, field_id) {
                let ident = self.unit.hir_node(id).as_ident().unwrap();
                if let Some(sym) = scopes.lookup_or_insert(&ident.name, node, kind) {
                    ident.set_symbol(sym);
                    sn.set_ident(ident);

                    let scope = self.unit.alloc_hir_scope(sym);
                    sym.set_scope(scope.id());
                    sym.add_defining(node.id());
                    sn.set_scope(scope);

                    scopes.push_scope(scope);
                    self.visit_children(node, scopes, scope, Some(sym));
                    scopes.pop_scope();
                }
            }
        }
    }

    /// Helper to create an unscoped named item (const, static, type_alias, field, etc.)
    /// This registers an identifier without creating a scope.
    fn visit_unscoped_named_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        kind: SymKind,
        field_id: u16,
    ) {
        if let Some(id) = node.find_identifier_for_field(self.unit, field_id) {
            let ident = self.unit.hir_node(id).as_ident().unwrap();
            if let Some(symbol) = scopes.lookup_or_insert(&ident.name, node, kind) {
                ident.set_symbol(symbol);
                symbol.add_defining(node.id());
            }
        }
    }

    /// Helper to create an unscoped item using a direct identifier search.
    /// Used when the item doesn't use a field ID.
    fn visit_unscoped_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        kind: SymKind,
    ) {
        if let Some(id) = node.find_identifier(self.unit) {
            let ident = self.unit.hir_node(id).as_ident().unwrap();
            if let Some(symbol) = scopes.lookup_or_insert(&ident.name, node, kind) {
                ident.set_symbol(symbol);
                symbol.add_defining(node.id());
            }
        }
    }

    /// Helper for scoped items using existing symbols from identifiers (e.g., impl_item).
    /// Like `visit_scoped_named`, but uses the symbol from the identifier
    /// directly instead of creating a new one. Falls back to unnamed scope if no identifier.
    fn visit_scoped_item_using_existing_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
        field_id: u16,
    ) {
        if let Some(sn) = node.as_scope() {
            if let Some(id) = node.find_identifier_for_field(self.unit, field_id) {
                let ident = self.unit.hir_node(id).as_ident().unwrap();
                sn.set_ident(ident);

                let scope = self.unit.alloc_hir_scope(ident.symbol());
                sn.set_scope(scope);

                scopes.push_scope(scope);
                self.visit_children(node, scopes, scope, Some(ident.symbol()));
                scopes.pop_scope();
            } else {
                let scope = self.unit.alloc_scope(node.id());
                sn.set_scope(scope);

                scopes.push_scope(scope);
                self.visit_children(node, scopes, namespace, parent);
                scopes.pop_scope();
            }
        }
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
            && let Some(symbol) = scopes.lookup_or_insert_global(&crate_name, node, SymKind::Crate)
        {
            scopes.push_scope_with(node, Some(symbol));
        }

        if let Some(module_name) = parse_module_name(&file_path)
            && let Some(symbol) = scopes.lookup_or_insert_global(&module_name, node, SymKind::Module)
        {
            scopes.push_scope_with(node, Some(symbol));

            if let Some(file_name) = parse_file_name(&file_path)
                && let Some(sn) = node.as_scope()
            {
                let ident = self.unit.alloc_hir_ident(file_name.clone(), symbol);
                sn.set_ident(ident);

                if let Some(file_sym) = scopes.lookup_or_insert(&file_name, node, SymKind::File) {
                    ident.set_symbol(file_sym);
                    file_sym.add_defining(node.id());

                    let scope = self.unit.alloc_hir_scope(file_sym);
                    file_sym.set_scope(scope.id());

                    sn.set_scope(scope);
                    scopes.push_scope(scope);

                    self.visit_children(node, scopes, namespace, parent);
                }
            }
        }

    }

    fn visit_mod_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if node
            .child_by_field(self.unit, LangRust::field_body)
            .is_none()
        {
            return;
        }
        self.visit_scoped_named(
            node,
            scopes,
            namespace,
            parent,
            SymKind::Module,
            LangRust::field_name,
        );
    }

    fn visit_function_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named(
            node,
            scopes,
            namespace,
            parent,
            SymKind::Function,
            LangRust::field_name,
        );
    }

    /// Override to prevent visiting unhandled node types.
    /// We only care about declaration items (mod, function, struct, etc.) and don't need to
    /// recursively visit tokens, parameters, blocks, etc.
    fn visit_unknown(
        &mut self,
        _node: &HirNode<'tcx>,
        _scopes: &mut CollectorScopes<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        // Do nothing - don't traverse children of unknown/unhandled node types
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::LangRust;
    use llmcc_core::context::CompileCtxt;
    use llmcc_core::interner::InternPool;
    use llmcc_core::ir_builder::{IrBuildConfig, build_llmcc_ir};
    use llmcc_core::printer::print_llmcc_ir;
    use llmcc_core::symbol::ScopeId;

    #[test]
    fn test_decl_visitor() {
        let source_code = br#"
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
        let sources = vec![source_code.to_vec()];

        let cc = CompileCtxt::from_sources::<LangRust>(&sources);
        let config = IrBuildConfig::default();
        build_llmcc_ir::<LangRust>(&cc, config).unwrap();

        let unit = cc.compile_unit(0);
        print_llmcc_ir(unit);

        let file_start = unit.file_start_hir_id().unwrap();
        let node = unit.hir_node(file_start);

        let globlas = cc.create_globals();
        let mut scopes = CollectorScopes::new(0, &cc.arena, &cc.interner, globlas);
        let mut v = DeclVisitor::new(unit);
        v.visit_node(node, &mut scopes, globlas, None);

        print!("{:#?}", globlas);
    }
}
