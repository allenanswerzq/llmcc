use llmcc_rust::{bind_symbols, build_llmcc_ir, collect_symbols, CompileCtxt, LangRust};

#[test]
fn method_depends_on_enclosing_type() {
    let source = r#"
        struct Foo {
        }

        impl Foo {
            fn method(&self) {}
        }
    "#;

    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(unit).expect("build HIR");

    let globals = cc.create_globals();
    let collection = collect_symbols(unit, globals);
    bind_symbols(unit, globals);

    let foo_desc = collection
        .structs
        .iter()
        .find(|desc| desc.name == "Foo")
        .expect("Foo descriptor");

    let method_desc = collection
        .functions
        .iter()
        .find(|desc| desc.name == "method")
        .expect("method descriptor");

    let foo_symbol = unit.get_scope(foo_desc.hir_id).symbol().unwrap();
    let method_symbol = unit.get_scope(method_desc.hir_id).symbol().unwrap();

    assert!(
        method_symbol
            .depended_by
            .borrow()
            .iter()
            .any(|id| *id == foo_symbol.id),
        "expected method depended by Foo"
    );

    assert!(
        foo_symbol
            .depends_on
            .borrow()
            .iter()
            .any(|id| *id == method_symbol.id),
        "expected Foo depends on method"
    );
}
