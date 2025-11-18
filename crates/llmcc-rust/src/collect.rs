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
        let ident_id = node.child_identifier_by_field(*unit, field_id)?;
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
            && let Some(id) = node.child_identifier_by_field(*unit, field_id)
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
            .child_identifier_by_field(*unit, LangRust::field_type)
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
        if let Some(pattern_id) = node.child_identifier_by_field(*unit, LangRust::field_pattern) {
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
    use llmcc_core::symbol::SymKind;
    use llmcc_resolver::{BinderOption, CollectorOption, bind_symbols_with, collect_symbols_with};

    fn with_compiled_unit<F>(sources: &[&str], check: F)
    where
        F: FnOnce(&CompileCtxt<'_>),
    {
        let bytes = sources
            .iter()
            .map(|src| src.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let cc = CompileCtxt::from_sources::<LangRust>(&bytes);
        build_llmcc_ir::<LangRust>(&cc, IrBuildOption).unwrap();
        let globals =
            collect_symbols_with::<LangRust>(&cc, CollectorOption::default().with_sequential(true));
        bind_symbols_with::<LangRust>(&cc, globals, BinderOption);
        check(&cc);
    }

    fn find_symbol_id(
        cc: &CompileCtxt<'_>,
        name: &str,
        kind: SymKind,
    ) -> llmcc_core::symbol::SymId {
        let name_key = cc.interner.intern(name);
        cc.symbol_map
            .read()
            .iter()
            .find(|(_, symbol)| symbol.name == name_key && symbol.kind() == kind)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("symbol {name} with kind {:?} not found", kind))
    }

    fn dependency_names(cc: &CompileCtxt<'_>, sym_id: llmcc_core::symbol::SymId) -> Vec<String> {
        let map = cc.symbol_map.read();
        let symbol = map
            .get(&sym_id)
            .copied()
            .unwrap_or_else(|| panic!("missing symbol for id {:?}", sym_id));
        let deps = symbol.depends.read().clone();
        let mut names = Vec::new();
        for dep in deps {
            if let Some(target) = map.get(&dep) {
                if let Some(name) = cc.interner.resolve_owned(target.name) {
                    names.push(name);
                }
            }
        }
        names.sort();
        names
    }

    #[test]
    fn call_expression_basic_dependency() {
        let source = r#"
fn callee() {}
fn caller() {
    callee();
}
"#;
        with_compiled_unit(&[source], |cc| {
            let caller = find_symbol_id(cc, "caller", SymKind::Function);
            assert_eq!(dependency_names(cc, caller), vec!["callee"]);
        });
    }

    #[test]
    fn method_call_dependency() {
        let source = r#"
struct MyStruct;
impl MyStruct {
    fn foo(&self) {}
}

fn run() {
    let s = MyStruct;
    s.foo();
}
"#;
        with_compiled_unit(&[source], |cc| {
            let run = find_symbol_id(cc, "run", SymKind::Function);
            assert_eq!(dependency_names(cc, run), vec!["foo"]);
        });
    }

    #[test]
    fn call_chain_dependency() {
        let source = r#"
struct Builder;
impl Builder {
    fn new() -> Self { Builder }
    fn step(self) -> Self { self }
    fn finish(self) {}
}

fn chain_invocation() {
    Builder::new().step().finish();
}
"#;
        with_compiled_unit(&[source], |cc| {
            let chain = find_symbol_id(cc, "chain_invocation", SymKind::Function);
            assert_eq!(dependency_names(cc, chain), vec!["finish", "new", "step"]);
        });
    }

    #[test]
    fn wrapped_call_dependency() {
        let source = r#"
async fn async_task() {}
fn maybe() -> Result<(), ()> { Ok(()) }

async fn entry() -> Result<(), ()> {
    (async_task)().await;
    (maybe)()?;
    Ok(())
}
"#;
        with_compiled_unit(&[source], |cc| {
            let entry = find_symbol_id(cc, "entry", SymKind::Function);
            assert_eq!(dependency_names(cc, entry), vec!["async_task", "maybe"]);
        });
    }

    #[test]
    fn macro_invocation_dependency() {
        let source = r#"
macro_rules! ping { () => {} }

fn call_macro() {
    ping!();
}
"#;
        with_compiled_unit(&[source], |cc| {
            let caller = find_symbol_id(cc, "call_macro", SymKind::Function);
            assert_eq!(dependency_names(cc, caller), vec!["ping"]);
        });
    }
}
