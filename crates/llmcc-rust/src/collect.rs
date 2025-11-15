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
    fn visit_scoped_named_item(
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
                let symbol = scopes.lookup_or_insert(&ident.name, node, kind);
                ident.set_symbol(symbol.unwrap());
                sn.set_ident(ident);

                let scope = self.unit.alloc_hir_scope(symbol.unwrap());
                sn.ident().symbol().set_scope(scope.id());
                sn.ident().symbol().add_defining(node.id());
                sn.set_scope(scope);

                scopes.push_scope(sn.scope());
                self.visit_children(node, scopes, scope, symbol);
                scopes.pop_scope();
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
            let symbol = scopes.lookup_or_insert(&ident.name, node, kind);
            ident.set_symbol(symbol.unwrap());
            symbol.unwrap().add_defining(node.id());
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
            let symbol = scopes.lookup_or_insert(&ident.name, node, kind);
            ident.set_symbol(symbol.unwrap());
            symbol.unwrap().add_defining(node.id());
        }
    }

    /// Helper for scoped items using existing symbols from identifiers (e.g., impl_item).
    /// Like `visit_scoped_named_item`, but uses the symbol from the identifier
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
            && let Some(symbol) = scopes.lookup_or_insert_global(&crate_name, node, SymKind::Module)
        {
            scopes.push_scope_with(node, Some(symbol));
        }

        if let Some(module_name) = parse_module_name(&file_path)
            && let Some(symbol) = scopes.lookup_or_insert_global(&module_name, node, SymKind::Module)
        {
            scopes.push_scope_with(node, Some(symbol));
        }

        if let Some(file_name) = parse_file_name(&file_path)
            && let Some(symbol) = scopes.lookup_or_insert(&file_name, node, SymKind::Module)
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
            }

            self.visit_children(node, scopes, namespace, parent);
        }
    }

    fn visit_mod_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        // Mod items without a body (e.g., `mod foo;`) don't create scopes
        if node.child_by_field(self.unit, LangRust::field_body).is_none() {
            return;
        }

        self.visit_scoped_named_item(node, scopes, namespace, parent, SymKind::Module, LangRust::field_name);
    }

    fn visit_function_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named_item(node, scopes, namespace, parent, SymKind::Function, LangRust::field_name);
    }

    fn visit_struct_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named_item(node, scopes, namespace, parent, SymKind::Struct, LangRust::field_name);
    }

    fn visit_enum_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named_item(node, scopes, namespace, parent, SymKind::Enum, LangRust::field_name);
    }

    fn visit_trait_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_named_item(node, scopes, namespace, parent, SymKind::Trait, LangRust::field_name);
    }

    fn visit_impl_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_scoped_item_using_existing_symbol(node, scopes, namespace, parent, LangRust::field_type);
    }

    fn visit_type_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_unscoped_item(node, scopes, SymKind::Const);
    }

    fn visit_const_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_unscoped_named_item(node, scopes, SymKind::Const, LangRust::field_name);
    }

    fn visit_static_item(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        self.visit_unscoped_named_item(node, scopes, SymKind::Static, LangRust::field_name);
    }

    fn visit_field_declaration(
        &mut self,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(id) = node.find_identifier_for_field(self.unit, LangRust::field_name) {
            let ident = self.unit.hir_node(id).as_ident().unwrap();
            let symbol = scopes.lookup_or_insert(&ident.name, node, SymKind::Field);
            ident.set_symbol(symbol.unwrap());
            symbol.unwrap().add_defining(node.id());

            if let Some(parent_sym) = parent {
                if let Some(scope_id) = parent_sym.scope() {
                    symbol.unwrap().set_parent_scope(scope_id);
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
    use llmcc_core::interner::InternPool;
    use llmcc_core::ir_builder::{IrBuildConfig, build_llmcc_ir};
    use llmcc_core::symbol::ScopeId;

    fn lookup_symbol<'tcx>(
        scope: &'tcx Scope<'tcx>,
        interner: &InternPool,
        name: &str,
        kind: SymKind,
    ) -> &'tcx Symbol {
        let key = interner.intern(name);
        scope
            .lookup_symbols(key)
            .into_iter()
            .find(|symbol| symbol.kind() == kind)
            .unwrap_or_else(|| panic!("symbol `{name}` with kind {kind:?} not found"))
    }

    fn scope_by_id<'tcx>(cc: &'tcx CompileCtxt<'tcx>, scope_id: ScopeId) -> &'tcx Scope<'tcx> {
        cc.arena
            .iter_scope()
            .find(|scope| scope.id() == scope_id)
            .expect("scope id missing from compile context")
    }

    fn symbol_name<'tcx>(cc: &'tcx CompileCtxt<'tcx>, symbol: &Symbol) -> String {
        cc.interner
            .resolve_owned(symbol.name)
            .unwrap_or_else(|| "<unresolved>".to_string())
    }

    fn find_symbol_anywhere<'tcx>(
        cc: &'tcx CompileCtxt<'tcx>,
        name: &str,
        kind: SymKind,
    ) -> Option<&'tcx Symbol> {
        let key = cc.interner.intern(name);
        cc.arena
            .iter_scope()
            .flat_map(|scope| scope.lookup_symbols(key))
            .find(|symbol| symbol.kind() == kind)
    }

    fn names_of_kind<'tcx>(cc: &'tcx CompileCtxt<'tcx>, kind: SymKind) -> Vec<String> {
        let mut names = Vec::new();
        cc.arena.iter_scope().for_each(|scope| {
            scope.for_each_symbol(|symbol| {
                if symbol.kind() == kind {
                    names.push(symbol_name(cc, symbol));
                }
            });
        });
        names.sort();
        names.dedup();
        names
    }

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
        let file_start = unit.file_start_hir_id().unwrap();
        let node = unit.hir_node(file_start);

        let globlas = cc.create_globals();
        let mut scopes = CollectorScopes::new(0, &cc.arena, &cc.interner, globlas);
        let mut v = DeclVisitor::new(unit);
        v.visit_node(node, &mut scopes, globlas, None);

        // file name comes from the virtual path: virtual://unit_0.rs
        assert_eq!(node.kind(), llmcc_core::ir::HirKind::File);

        let module_symbol = lookup_symbol(globlas, &cc.interner, "unit_0", SymKind::Module);
        assert_eq!(module_symbol.unit_index(), Some(0));
        assert!(module_symbol.parent_scope().is_some());
        assert!(module_symbol.scope().is_some());
        assert_ne!(module_symbol.scope(), module_symbol.parent_scope());

        let module_scope = scope_by_id(&cc, module_symbol.scope().unwrap());
        assert_eq!(module_scope.owner(), node.id());
        assert_eq!(
            module_scope
                .symbol()
                .expect("module scope should have symbol")
                .id(),
            module_symbol.id()
        );

        let file_path = unit.file_path().unwrap();
        let maybe_crate_name = parse_crate_name(file_path);
        if let Some(crate_name) = &maybe_crate_name {
            let crate_symbol =
                lookup_symbol(globlas, &cc.interner, crate_name, SymKind::Module);

            assert_eq!(crate_symbol.unit_index(), Some(0));
            assert_eq!(crate_symbol.scope(), module_symbol.parent_scope());
            assert_eq!(crate_symbol.parent_scope(), Some(globlas.id()));

            let crate_scope = scope_by_id(&cc, crate_symbol.scope().unwrap());
            assert_eq!(
                crate_scope
                    .symbol()
                    .expect("crate scope should own its symbol")
                    .id(),
                crate_symbol.id()
            );
        } else {
            assert_eq!(module_symbol.parent_scope(), Some(globlas.id()));
        }

        let outer_symbol = find_symbol_anywhere(&cc, "outer", SymKind::Module).unwrap_or_else(
            || {
                panic!(
                    "outer module missing. modules found: {:?}",
                    names_of_kind(&cc, SymKind::Module)
                )
            },
        );
        assert!(outer_symbol.scope().is_some());
        let outer_scope = scope_by_id(&cc, outer_symbol.scope().unwrap());
        assert_eq!(
            outer_scope
                .symbol()
                .expect("outer scope missing parent symbol")
                .id(),
            outer_symbol.id()
        );

        let nested_fn =
            lookup_symbol(outer_scope, &cc.interner, "nested_function", SymKind::Function);
        assert!(nested_fn.scope().is_some());

        let inner_const =
            lookup_symbol(outer_scope, &cc.interner, "INNER_CONST", SymKind::Const);
        assert_eq!(inner_const.defining.read().len(), 1);

        let top_function =
            find_symbol_anywhere(&cc, "top_level", SymKind::Function).expect("top_level missing");
        assert!(top_function.scope().is_some());

        let foo_symbol =
            find_symbol_anywhere(&cc, "Foo", SymKind::Struct).expect("Foo struct missing");
        let foo_scope = scope_by_id(&cc, foo_symbol.scope().unwrap());
        assert_eq!(
            foo_scope
                .symbol()
                .expect("foo scope missing symbol reference")
                .id(),
            foo_symbol.id()
        );

        let field_symbol = lookup_symbol(foo_scope, &cc.interner, "field", SymKind::Field);
        assert_eq!(field_symbol.parent_scope(), foo_symbol.scope());

        let foo_related_scopes: Vec<_> = cc
            .arena
            .iter_scope()
            .filter(|scope| scope.symbol().map(|s| s.id()) == Some(foo_symbol.id()))
            .collect();
        assert!(
            foo_related_scopes.len() >= 2,
            "expected struct and impl scopes for Foo but found {}",
            foo_related_scopes.len()
        );

        let method_symbol =
            find_symbol_anywhere(&cc, "method", SymKind::Function).expect("method missing");
        assert!(method_symbol.scope().is_some());

        let color_symbol =
            find_symbol_anywhere(&cc, "Color", SymKind::Enum).expect("Color enum missing");
        assert!(color_symbol.scope().is_some());
        let color_scope = scope_by_id(&cc, color_symbol.scope().unwrap());
        assert_eq!(
            color_scope
                .symbol()
                .expect("color scope missing symbol reference")
                .id(),
            color_symbol.id()
        );

        let greeter_symbol =
            find_symbol_anywhere(&cc, "Greeter", SymKind::Trait).expect("Greeter trait missing");
        assert!(greeter_symbol.scope().is_some());
        let greeter_scope = scope_by_id(&cc, greeter_symbol.scope().unwrap());
        assert_eq!(
            greeter_scope
                .symbol()
                .expect("greeter scope missing symbol reference")
                .id(),
            greeter_symbol.id()
        );

        let alias_symbol =
            find_symbol_anywhere(&cc, "Alias", SymKind::Const).expect("Alias type missing");
        assert!(alias_symbol.scope().is_none());

        let top_const =
            find_symbol_anywhere(&cc, "TOP_CONST", SymKind::Const).expect("TOP_CONST missing");
        assert_eq!(top_const.defining.read().len(), 1);

        let top_static =
            find_symbol_anywhere(&cc, "TOP_STATIC", SymKind::Static).expect("TOP_STATIC missing");
        assert_eq!(top_static.defining.read().len(), 1);

        let mut global_symbols = Vec::new();
        globlas.for_each_symbol(|symbol| {
            global_symbols.push((symbol_name(&cc, symbol), symbol.kind()));
        });
        assert!(
            global_symbols
                .iter()
                .any(|(name, kind)| name == "unit_0" && *kind == SymKind::Module),
            "unit_0 module symbol missing from globals: {:?}",
            global_symbols
        );
        if let Some(crate_name) = maybe_crate_name {
            assert!(
                global_symbols
                    .iter()
                    .any(|(name, kind)| name == &crate_name && *kind == SymKind::Module),
                "crate symbol `{}` missing from globals: {:?}",
                crate_name,
                global_symbols
            );
        }
    }
}
