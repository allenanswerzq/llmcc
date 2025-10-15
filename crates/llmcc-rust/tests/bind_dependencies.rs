use llmcc_rust::{bind_symbols, build_llmcc_ir, collect_symbols, CompileCtxt, LangRust};

#[test]
fn method_depends_on_enclosing_type() {
    let source = r#"
        struct Foo;

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

    let foo_symbol = unit.defs(foo_desc.hir_id);
    let method_symbol = unit.defs(method_desc.hir_id);

    assert!(
        method_symbol
            .depends_on
            .borrow()
            .iter()
            .any(|id| *id == foo_symbol.id),
        "expected method to depend on Foo"
    );

    assert!(
        foo_symbol
            .depended_by
            .borrow()
            .iter()
            .any(|id| *id == method_symbol.id),
        "expected Foo to record dependency from method"
    );
}

// #[test]
// fn method_records_dependencies_for_calls() {
//     let source = r#"
//         struct Foo;

//         fn helper(_: &Foo) {}

//         impl Foo {
//             fn caller(&self) {
//                 helper(self);
//             }
//         }
//     "#;

//     let sources = vec![source.as_bytes().to_vec()];
//     let cc = CompileCtxt::from_sources::<LangRust>(&sources);
//     let unit = cc.compile_unit(0);
//     build_llmcc_ir::<LangRust>(unit).expect("build HIR");
//     let globals = cc.create_globals();
//     let collection = collect_symbols(unit, globals);
//     bind_symbols(unit, globals);

//     let helper_desc = collection
//         .functions
//         .iter()
//         .find(|desc| desc.name == "helper" && desc.fqn == "helper")
//         .expect("helper descriptor");
//     let caller_desc = collection
//         .functions
//         .iter()
//         .find(|desc| desc.name == "caller")
//         .expect("caller descriptor");

//     let helper_symbol = unit.defs(helper_desc.hir_id);
//     let caller_symbol = unit.defs(caller_desc.hir_id);

//     assert!(
//         caller_symbol
//             .depends_on
//             .borrow()
//             .iter()
//             .any(|id| *id == helper_symbol.id),
//         "expected caller to depend on helper"
//     );

//     assert!(
//         helper_symbol
//             .depended_by
//             .borrow()
//             .iter()
//             .any(|id| *id == caller_symbol.id),
//         "expected helper to record caller as dependent"
//     );
// }
