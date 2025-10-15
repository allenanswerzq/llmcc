use llmcc_core::ir::HirId;
use llmcc_core::symbol::Symbol;
use llmcc_rust::{bind_symbols, build_llmcc_ir, collect_symbols, CompileCtxt, LangRust};

fn compile(
    source: &str,
) -> (
    &'static CompileCtxt<'static>,
    llmcc_core::context::CompileUnit<'static>,
) {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(&sources)));
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(unit).expect("build HIR");
    (cc, unit)
}

fn find_struct<'a>(
    collection: &'a llmcc_rust::CollectionResult,
    name: &str,
) -> &'a llmcc_rust::StructDescriptor {
    collection
        .structs
        .iter()
        .find(|desc| desc.name == name)
        .unwrap()
}

fn find_function<'a>(
    collection: &'a llmcc_rust::CollectionResult,
    name: &str,
) -> &'a llmcc_rust::FunctionDescriptor {
    collection
        .functions
        .iter()
        .find(|desc| desc.name == name)
        .unwrap()
}

fn symbol(unit: llmcc_core::context::CompileUnit<'static>, hir_id: HirId) -> &'static Symbol {
    unit.get_scope(hir_id).symbol().unwrap()
}

fn assert_depends_on(symbol: &Symbol, target: &Symbol) {
    assert!(symbol.depends_on.borrow().iter().any(|id| *id == target.id));
}

fn assert_depended_by(symbol: &Symbol, source: &Symbol) {
    assert!(symbol
        .depended_by
        .borrow()
        .iter()
        .any(|id| *id == source.id));
}

#[test]
fn type_records_dependencies_on_methods() {
    let source = r#"
        struct Foo;

        impl Foo {
            fn method(&self) {}
        }
    "#;

    let (cc, unit) = compile(source);
    let globals = cc.create_globals();
    let collection = collect_symbols(unit, globals);
    bind_symbols(unit, globals);

    let foo_desc = find_struct(&collection, "Foo");
    let method_desc = find_function(&collection, "method");

    let foo_symbol = symbol(unit, foo_desc.hir_id);
    let method_symbol = symbol(unit, method_desc.hir_id);

    assert_depends_on(foo_symbol, method_symbol);
    assert_depended_by(method_symbol, foo_symbol);
}

#[test]
fn method_depends_on_inherent_method() {
    let source = r#"
        struct Foo;

        impl Foo {
            fn helper(&self) {}

            fn caller(&self) {
                self.helper();
            }
        }
    "#;

    let (cc, unit) = compile(source);
    let globals = cc.create_globals();
    let collection = collect_symbols(unit, globals);
    bind_symbols(unit, globals);

    let helper_desc = find_function(&collection, "helper");
    let caller_desc = find_function(&collection, "caller");

    let helper_symbol = symbol(unit, helper_desc.hir_id);
    let caller_symbol = symbol(unit, caller_desc.hir_id);

    assert_depends_on(caller_symbol, helper_symbol);
    assert_depended_by(helper_symbol, caller_symbol);
}

#[test]
fn function_depends_on_called_function() {
    let source = r#"
        fn helper() {}

        fn caller() {
            helper();
        }
    "#;

    let (cc, unit) = compile(source);
    let globals = cc.create_globals();
    let collection = collect_symbols(unit, globals);
    bind_symbols(unit, globals);

    let helper_desc = find_function(&collection, "helper");
    let caller_desc = find_function(&collection, "caller");

    let helper_symbol = symbol(unit, helper_desc.hir_id);
    let caller_symbol = symbol(unit, caller_desc.hir_id);

    assert_depends_on(caller_symbol, helper_symbol);
    assert_depended_by(helper_symbol, caller_symbol);
}

#[test]
fn function_depends_on_argument_type() {
    let source = r#"
        struct Foo;

        fn takes(_: Foo) {}
    "#;

    let (cc, unit) = compile(source);
    let globals = cc.create_globals();
    let collection = collect_symbols(unit, globals);
    bind_symbols(unit, globals);

    let foo_desc = find_struct(&collection, "Foo");
    let takes_desc = find_function(&collection, "takes");

    let foo_symbol = symbol(unit, foo_desc.hir_id);
    let takes_symbol = symbol(unit, takes_desc.hir_id);

    assert_depends_on(takes_symbol, foo_symbol);
}

#[test]
fn const_initializer_records_dependencies() {
    let source = r#"
        fn helper() -> i32 { 5 }

        const VALUE: i32 = helper();
    "#;

    let (cc, unit) = compile(source);
    let globals = cc.create_globals();
    let collection = collect_symbols(unit, globals);
    bind_symbols(unit, globals);

    let helper_desc = collection
        .functions
        .iter()
        .find(|desc| desc.name == "helper")
        .expect("helper descriptor");
    let const_desc = collection
        .variables
        .iter()
        .find(|desc| desc.name == "VALUE")
        .expect("const descriptor");

    let helper_symbol = symbol(unit, helper_desc.hir_id);
    let const_symbol = symbol(unit, const_desc.hir_id);

    assert_depends_on(const_symbol, helper_symbol);
}
