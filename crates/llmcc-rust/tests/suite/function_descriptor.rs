use std::collections::HashMap;

use llmcc_core::IrBuildConfig;
use llmcc_rust::{build_llmcc_ir, collect_symbols, CompileCtxt, FnVisibility, LangRust, TypeExpr};

fn collect_functions(source: &str) -> HashMap<String, llmcc_rust::FunctionDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(&cc, IrBuildConfig::default()).unwrap();

    let globals = cc.create_globals();
    collect_symbols(unit, globals)
        .functions
        .into_iter()
        .map(|desc| (desc.fqn.clone(), desc))
        .collect()
}

#[test]
fn detects_private_function() {
    let map = collect_functions("fn foo() {}\n");
    let foo = map.get("foo").unwrap();
    assert_eq!(foo.visibility, FnVisibility::Private);
    assert!(foo.parameters.is_empty());
    assert!(foo.return_type.is_none());
}

#[test]
fn detects_public_visibility() {
    let map = collect_functions("pub fn foo() {}\n");
    assert_eq!(map.get("foo").unwrap().visibility, FnVisibility::Public);
}

#[test]
fn detects_pub_crate_visibility() {
    let map = collect_functions("pub(crate) fn foo() {}\n");
    assert_eq!(map.get("foo").unwrap().visibility, FnVisibility::Crate);
}

#[test]
fn captures_parameters_and_return_type() {
    let source = r#"
        fn transform(value: i32, label: Option<&str>) -> Result<i32, &'static str> {
            Ok(value)
        }
    "#;
    let map = collect_functions(source);
    let desc = map.get("transform").unwrap();
    assert_eq!(desc.parameters.len(), 2);
    assert_eq!(desc.parameters[0].pattern, "value");
    assert_eq!(desc.parameters[1].pattern, "label");

    let param0 = desc.parameters[0].ty.as_ref().unwrap();
    assert_path(param0, &["i32"]);

    let param1 = desc.parameters[1].ty.as_ref().unwrap();
    let generics = assert_path(param1, &["Option"]);
    assert_eq!(generics.len(), 1);
    let inner = &generics[0];
    if let TypeExpr::Reference {
        is_mut,
        lifetime,
        inner,
    } = inner
    {
        assert!(!is_mut);
        assert!(lifetime.is_none());
        assert_path(inner, &["str"]);
    } else {
        panic!();
    }

    let return_type = desc.return_type.as_ref().unwrap();
    let generics = assert_path(return_type, &["Result"]);
    assert_eq!(generics.len(), 2);
    assert_path(&generics[0], &["i32"]);
    if let TypeExpr::Reference {
        is_mut,
        lifetime,
        inner,
    } = &generics[1]
    {
        assert!(!is_mut);
        assert_eq!(lifetime.as_deref(), Some("'static"));
        assert_path(inner, &["str"]);
    } else {
        panic!();
    }
}

#[test]
fn captures_async_const_and_unsafe_flags() {
    let source = r#"
        async unsafe fn perform() {}
        const fn build() -> i32 { 0 }
    "#;
    let map = collect_functions(source);

    let perform = map.get("perform").unwrap();
    assert!(perform.is_async);
    assert!(perform.is_unsafe);
    assert!(!perform.is_const);

    let build = map.get("build").unwrap();
    assert!(build.is_const);
    assert!(!build.is_async);
    assert!(!build.is_unsafe);
}

fn assert_path<'a>(expr: &'a TypeExpr, expected: &[&str]) -> &'a [TypeExpr] {
    if let TypeExpr::Path { segments, generics } = expr {
        let expected_vec: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(segments, &expected_vec);
        generics
    } else {
        panic!();
    }
}
