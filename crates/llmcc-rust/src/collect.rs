use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::next_hir_id;
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_resolver::{CollectorScopes, ResolverOption};

use crate::LangRust;
use crate::token::AstVisitorRust;
use crate::util::{parse_crate_name, parse_file_name, parse_module_name};

#[derive(Debug)]
pub struct CollectorVisitor<'tcx> {
    phantom: std::marker::PhantomData<&'tcx ()>,
    block_counter: usize,
}

impl<'tcx> CollectorVisitor<'tcx> {
    fn new() -> Self {
        Self {
            phantom: std::marker::PhantomData,
            block_counter: 0,
        }
    }

    /// Declare a symbol from a named field in the AST node
    fn declare_symbol_from_field(
        &self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
        field_id: u16,
    ) -> Option<&'tcx Symbol> {
        let ident = node.child_identifier_by_field(*unit, field_id)?;
        scopes.lookup_or_insert(&ident.name, node, kind)
    }

    /// Find all identifiers in a pattern node (recursive)
    fn collect_pattern_identifiers(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
    ) -> Vec<&'tcx Symbol> {
        let mut symbols = Vec::new();
        Self::collect_pattern_identifiers_impl(unit, node, scopes, kind, &mut symbols);
        symbols
    }

    fn collect_pattern_identifiers_impl(
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &CollectorScopes<'tcx>,
        kind: SymKind,
        symbols: &mut Vec<&'tcx Symbol>,
    ) {
        // Skip non-binding identifiers
        if matches!(
            node.kind_id(),
            LangRust::type_identifier
                | LangRust::primitive_type
                | LangRust::field_identifier
        ) {
            return;
        }

        // Special handling for scoped identifiers: don't collect them as variables, but recurse
        if matches!(
            node.kind_id(),
            LangRust::scoped_identifier | LangRust::scoped_type_identifier
        ) {
            for child_id in node.children() {
                let child = unit.hir_node(*child_id);
                Self::collect_pattern_identifiers_impl(unit, &child, scopes, kind, symbols);
            }
            return;
        }

        if let Some(ident) = node.as_ident() {
            let name = ident.name.to_string();
            let sym = if kind == SymKind::Variable {
                scopes.lookup_or_insert_chained(&name, node, kind)
            } else {
                scopes.lookup_or_insert(&name, node, kind)
            };

            if let Some(sym) = sym {
                ident.set_symbol(sym);
                symbols.push(sym);
            }
        }
        for child_id in node.children() {
            let child = unit.hir_node(*child_id);
            Self::collect_pattern_identifiers_impl(unit, &child, scopes, kind, symbols);
        }
    }

    fn impl_symbol_name(type_name: Option<String>) -> String {
        match type_name {
            Some(name) => Self::normalize_impl_type_name(name),
            None => "impl".to_string(),
        }
    }

    fn normalize_impl_type_name(name: String) -> String {
        if name.starts_with("::") {
            if let Some((head, _)) = name.rsplit_once("::") {
                return if head.is_empty() { name } else { head.to_string() };
            }
            return name;
        }
        if name.starts_with("Self::") {
            return "Self".to_string();
        }
        if let Some((_, tail)) = name.rsplit_once("::") {
            return tail.to_string();
        }
        name
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
        let _ = (namespace, parent);

        if let Some(sn) = node.as_scope()
            && let Some(ident) = node.child_identifier_by_field(*unit, field_id)
            && let Some(sym) = scopes.lookup_or_insert(&ident.name, node, kind)
        {
            // Link the identifier to the symbol and the scoped node to the identifier.
            ident.set_symbol(sym);
            sn.set_ident(ident);

            // Allocate the new scope associated with the symbol.
            let scope = unit.cc.alloc_hir_scope(next_hir_id(), sym);

            // Update the symbol with the new scope and defining node ID.
            sym.set_scope(scope.id());

            // Link the ScopedNamed node to the newly created scope.
            sn.set_scope(scope);

            scopes.push_scope(scope);
            self.visit_children(unit, node, scopes, scope, Some(sym));
            scopes.pop_scope();
        }
    }
}

impl<'tcx> AstVisitorRust<'tcx, CollectorScopes<'tcx>> for CollectorVisitor<'tcx> {
    fn visit_block(
        &mut self,
        unit: &CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        scopes: &mut CollectorScopes<'tcx>,
        namespace: &'tcx Scope<'tcx>,
        parent: Option<&Symbol>,
    ) {
        if let Some(sn) = node.as_scope() {
            // Create a unique name for the block to avoid collisions
            let name = format!("block_{}", self.block_counter);
            self.block_counter += 1;

            if let Some(symbol) = scopes.lookup_or_insert(&name, node, SymKind::Namespace) {
                let ident = unit
                    .cc
                    .alloc_hir_ident(next_hir_id(), &name, symbol);
                sn.set_ident(ident);

                let scope = unit.cc.alloc_hir_scope(next_hir_id(), symbol);
                symbol.set_scope(scope.id());
                sn.set_scope(scope);

                scopes.push_scope(scope);
                self.visit_children(unit, node, scopes, namespace, Some(symbol));
                scopes.pop_scope();
                return;
            }
        }
        self.visit_children(unit, node, scopes, namespace, parent);
    }

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

        // Initialize primitives in the global scope if they don't exist
        let primitives = [
            "i32", "i64", "i16", "i8", "i128", "isize",
            "u32", "u64", "u16", "u8", "u128", "usize",
            "f32", "f64",
            "bool", "char", "str",
        ];
        for prim in primitives {
            scopes.lookup_or_insert_global(prim, node, SymKind::Type);
        }

        if let Some(crate_name) = parse_crate_name(file_path)
            && let Some(symbol) = scopes.lookup_or_insert_global(&crate_name, node, SymKind::Crate)
        {
            scopes.push_scope_with(node, Some(symbol));
        }

        if let Some(module_name) = parse_module_name(file_path) {
            if let Some(symbol) = scopes.lookup_or_insert_global(&module_name, node, SymKind::Module)
            {
                scopes.push_scope_with(node, Some(symbol));
            }
        }

        if let Some(file_name) = parse_file_name(file_path)
            && let Some(sn) = node.as_scope()
        {
            if let Some(file_sym) = scopes .lookup_or_insert(&file_name, node, SymKind::File)
            {
                let ident = unit
                    .cc
                    .alloc_hir_ident(next_hir_id(), &file_name, file_sym);
                sn.set_ident(ident);
                ident.set_symbol(file_sym);
                file_sym.add_defining(node.id());
                // file_sym.set_fqn(scopes.build_fqn_with(&file_name));

                let scope = unit.cc.alloc_hir_scope(next_hir_id(), file_sym);
                file_sym.set_scope(scope.id());
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
        let type_name = node
            .child_identifier_by_field(*unit, LangRust::field_type)
            .map(|ident| ident.name.to_string());
        let impl_name = Self::impl_symbol_name(type_name);

        if let Some(symbol) = scopes.lookup_or_insert(&impl_name, node, SymKind::Impl) {
            let ident = unit
                .cc
                .alloc_hir_ident(next_hir_id(), &impl_name, symbol);
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
        if let Some(pattern) = node.child_by_field(*unit, LangRust::field_pattern) {
            Self::collect_pattern_identifiers(unit, &pattern, scopes, SymKind::Variable);
        }

        self.visit_children(unit, node, scopes, namespace, parent);
    }
}

pub fn collect_symbols<'tcx>(
    unit: &CompileUnit<'tcx>,
    node: &HirNode<'tcx>,
    scopes: &mut CollectorScopes<'tcx>,
    namespace: &'tcx Scope<'tcx>,
    _config: &ResolverOption,
) {
    CollectorVisitor::new().visit_node(unit, node, scopes, namespace, None);
}

#[cfg(test)]
mod tests {
    use crate::token::LangRust;

    use llmcc_core::context::CompileCtxt;
    use llmcc_core::ir_builder::{IrBuildOption, build_llmcc_ir};
    use llmcc_core::symbol::{SymId, SymKind};
    use llmcc_resolver::{ResolverOption, bind_symbols_with, collect_symbols_with};

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
        let resolver_option = ResolverOption::default().with_sequential(true);
        let globals = collect_symbols_with::<LangRust>(&cc, &resolver_option);
        bind_symbols_with::<LangRust>(&cc, globals, &resolver_option);
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

    fn dependency_names(cc: &CompileCtxt<'_>, sym_id: SymId) -> Vec<String> {
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

    fn type_name_of(cc: &CompileCtxt<'_>, sym_id: SymId) -> Option<String> {
        let map = cc.symbol_map.read();
        let symbol = map.get(&sym_id).copied()?;
        let ty_id = symbol.type_of();
        drop(map);
        let ty_id = ty_id?;
        let map = cc.symbol_map.read();
        let ty_symbol = map.get(&ty_id).copied()?;
        cc.interner.resolve_owned(ty_symbol.name)
    }

    fn assert_dependencies(source: &[&str], expectations: &[(&str, SymKind, &[&str])]) {
        with_compiled_unit(source, |cc| {
            for (name, kind, deps) in expectations {
                let sym_id = find_symbol_id(cc, name, *kind);
                let expected: Vec<String> = deps.iter().map(|s| s.to_string()).collect();
                assert_eq!(
                    dependency_names(cc, sym_id),
                    expected,
                    "dependency mismatch for symbol {name}"
                );
            }
        });
    }

    fn assert_symbol_type(source: &[&str], name: &str, kind: SymKind, expected: Option<&str>) {
        with_compiled_unit(source, |cc| {
            let sym_id = find_symbol_id(cc, name, kind);
            let actual = type_name_of(cc, sym_id);
            assert_eq!(
                actual.as_deref(),
                expected,
                "type mismatch for symbol {name}"
            );
        });
    }

    #[test]
    fn call_expression_basic_dependency() {
        let source = r#"
fn callee() {}
fn caller() {
    callee();
}
"#;
        assert_dependencies(&[source], &[("caller", SymKind::Function, &["callee"])]);
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
        // run depends on MyStruct (via s) and foo (via call)
        assert_dependencies(&[source], &[("run", SymKind::Function, &["MyStruct", "foo"])]);
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
        assert_dependencies(
            &[source],
            &[(
                "chain_invocation",
                SymKind::Function,
                &["finish", "new", "step"],
            )],
        );
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
        assert_dependencies(
            &[source],
            &[("entry", SymKind::Function, &["async_task", "maybe"])],
        );
    }

    #[test]
    fn macro_invocation_dependency() {
        let source = r#"
macro_rules! ping { () => {} }

fn call_macro() {
    ping!();
}
"#;
        assert_dependencies(&[source], &[("call_macro", SymKind::Function, &["ping"])]);
    }

    #[test]
    fn scoped_function_dependency() {
        let source = r#"
mod helpers {
    pub fn compute() {}
}

fn run() {
    helpers::compute();
    crate::helpers::compute();
}
"#;
        assert_dependencies(&[source], &[("run", SymKind::Function, &["compute"])]);
    }

    #[test]
    fn associated_function_dependency() {
        let source = r#"
struct Foo;
impl Foo {
    fn build() -> Self {
        Foo
    }
}

fn run() {
    Foo::build();
}
"#;
        assert_dependencies(&[source], &[("run", SymKind::Function, &["build"])]);
    }

    #[test]
    fn trait_fully_qualified_call_dependency() {
        let source = r#"
trait Greeter {
    fn greet();
}

struct Foo;

impl Greeter for Foo {
    fn greet() {}
}

fn run() {
    <Foo as Greeter>::greet();
}
"#;
        assert_dependencies(&[source], &[("run", SymKind::Function, &["greet"])]);
    }

    #[test]
    fn namespaced_macro_dependency() {
        let source = r#"
mod outer {
    pub mod inner {
        macro_rules! shout {
            () => {};
        }
        pub(crate) use shout;
    }
}

fn run() {
    outer::inner::shout!();
}
"#;
        assert_dependencies(&[source], &[("run", SymKind::Function, &["shout"])]);
    }

    #[test]
    fn super_module_function_dependency() {
        let source = r#"
mod outer {
    pub fn top() {}
    pub mod inner {
        pub fn run() {
            super::top();
        }
    }
}
"#;
        assert_dependencies(&[source], &[("run", SymKind::Function, &["top"])]);
    }

    #[test]
    fn variable_type_annotation() {
        let source = r#"
struct Foo;

fn run() {
    let value: Foo = Foo;
    let other = Foo;
}
"#;
        assert_symbol_type(&[source], "value", SymKind::Variable, Some("Foo"));
        assert_symbol_type(&[source], "other", SymKind::Variable, None);
    }

    #[test]
    fn static_type_annotation() {
        let source = r#"
struct Foo;
static GLOBAL: Foo = Foo;
"#;
        assert_symbol_type(&[source], "GLOBAL", SymKind::Static, Some("Foo"));
    }

    #[test]
    fn parameter_type_annotation() {
        let source = r#"
struct Foo;

fn consume(param: Foo) {
    let _ = param;
}
"#;
        assert_symbol_type(&[source], "param", SymKind::Variable, Some("Foo"));
    }

    #[test]
    fn field_type_annotation() {
        let source = r#"
struct Bar;
struct Bucket {
    item: Bar,
}
"#;
        assert_symbol_type(&[source], "item", SymKind::Field, Some("Bar"));
    }

    #[test]
    fn const_and_type_alias_types() {
        let source = r#"
struct Foo;
type Alias = Foo;
const ANSWER: i32 = 42;
"#;
        assert_symbol_type(&[source], "Alias", SymKind::Type, Some("Foo"));
        assert_symbol_type(&[source], "ANSWER", SymKind::Const, None);
    }

    #[test]
    fn struct_field_generic_dependency() {
        let source = r#"
struct Foo;
struct List<T>(T);

struct Container {
    data: List<Foo>,
}
"#;
        assert_dependencies(
            &[source],
            &[
                ("Container", SymKind::Struct, &["Foo", "List"]),
                ("data", SymKind::Field, &["Foo", "List"]),
            ],
        );
    }

    #[test]
    fn enum_variant_dependency() {
        let source = r#"
struct Foo;
enum Wrapper {
    Item(Foo),
}
"#;
        assert_dependencies(&[source], &[("Wrapper", SymKind::Enum, &["Foo"])]);
    }

    #[test]
    fn let_statement_generic_dependency() {
        let source = r#"
struct Foo;
struct Bar;
enum Result<T, E> {
    Ok(T),
    Err(E),
}

fn run() {
    let value: Result<Foo, Bar> = Result::Ok(Foo);
}
"#;
        assert_dependencies(
            &[source],
            &[
                ("value", SymKind::Variable, &["Bar", "Foo", "Result"]),
                ("run", SymKind::Function, &["Bar", "Foo", "Result"]),
            ],
        );
    }
}
