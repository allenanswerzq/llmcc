use std::collections::HashMap;

use llmcc_rust::{
    build_llmcc_ir, collect_symbols, GlobalCtxt, LangRust, TypeExpr, VariableDescriptor,
    VariableKind, VariableScope,
};

fn collect_variables(source: &str) -> HashMap<String, VariableDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let gcx = GlobalCtxt::from_sources::<LangRust>(&sources);
    let ctx = gcx.file_context(0);
    let tree = ctx.tree();
    build_llmcc_ir::<LangRust>(&tree, ctx).expect("build HIR");
    let root = ctx.file_start_hir_id().expect("registered root id");
    let globals = gcx.alloc_scope(root);
    collect_symbols(root, ctx, globals)
        .variables
        .into_iter()
        .map(|desc| (desc.fqn.clone(), desc))
        .collect()
}

#[test]
fn captures_global_const() {
    let map = collect_variables("const MAX: i32 = 10;\n");
    let desc = map.get("MAX").expect("MAX const");
    assert_eq!(desc.kind, VariableKind::Const);
    assert_eq!(desc.scope, VariableScope::Global);
    assert!(!desc.is_mut);
    let ty = desc.ty.as_ref().expect("const type");
    assert_path(ty, &["i32"]);
}

#[test]
fn captures_static_mut() {
    let map = collect_variables("static mut COUNTER: usize = 0;\n");
    let desc = map.get("COUNTER").expect("COUNTER static");
    assert_eq!(desc.kind, VariableKind::Static);
    assert_eq!(desc.scope, VariableScope::Global);
    assert!(desc.is_mut);
    let ty = desc.ty.as_ref().expect("static type");
    assert_path(ty, &["usize"]);
}

#[test]
fn captures_local_let_with_type() {
    let source = r#"
        fn wrapper() {
            let mut value: Option<Result<i32, &'static str>> = None;
        }
    "#;
    let map = collect_variables(source);
    let desc = map
        .get("wrapper::value")
        .expect("local variable fqn wrapper::value");
    assert_eq!(desc.kind, VariableKind::Let);
    assert_eq!(desc.scope, VariableScope::Local);
    assert!(desc.is_mut);

    let ty = desc.ty.as_ref().expect("let type");
    let generics = assert_path(ty, &["Option"]);
    assert_eq!(generics.len(), 1);
    let inner = &generics[0];
    let result_generics = assert_path(inner, &["Result"]);
    assert_eq!(result_generics.len(), 2);
    assert_path(&result_generics[0], &["i32"]);
    match &result_generics[1] {
        TypeExpr::Reference {
            is_mut,
            lifetime,
            inner,
        } => {
            assert!(!is_mut);
            assert_eq!(lifetime.as_deref(), Some("'static"));
            assert_path(inner, &["str"]);
        }
        other => panic!("unexpected type: {other:?}"),
    }
}

fn assert_path<'a>(expr: &'a TypeExpr, expected: &[&str]) -> &'a [TypeExpr] {
    match expr {
        TypeExpr::Path { segments, generics } => {
            let expected_vec: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
            assert_eq!(segments, &expected_vec);
            generics
        }
        other => panic!("expected path type, found {other:?}"),
    }
}
