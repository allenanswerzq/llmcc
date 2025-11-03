use std::collections::HashMap;

use llmcc_core::IrBuildConfig;
use llmcc_rust::{
    build_llmcc_ir, collect_symbols, CompileCtxt, LangRust, TypeExpr, VariableDescriptor,
    VariableKind, VariableScope,
};

fn collect_variables(source: &str) -> HashMap<String, VariableDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(&cc, IrBuildConfig).unwrap();

    let globals = cc.create_globals();
    let prefix = format!("unit{}::", unit.index);

    let mut map = HashMap::new();
    for desc in collect_symbols(unit, globals).variables {
        if let Some(ref fqn) = desc.fqn {
            map.insert(fqn.clone(), desc.clone());
            if let Some(stripped) = fqn.strip_prefix(&prefix) {
                map.insert(stripped.to_string(), desc.clone());
            }
        }

        map.insert(desc.name.clone(), desc);
    }

    map
}

#[test]
fn captures_global_const() {
    let map = collect_variables("const MAX: i32 = 10;\n");
    let desc = map.get("MAX").unwrap();
    assert_eq!(desc.kind, VariableKind::Constant);
    assert_eq!(desc.scope, VariableScope::Global);
    assert_eq!(desc.is_mutable, Some(false));
    let ty = desc.type_annotation.as_ref().unwrap();
    assert_path(ty, &["i32"]);
}

#[test]
fn captures_static_mut() {
    let map = collect_variables("static mut COUNTER: usize = 0;\n");
    let desc = map.get("COUNTER").unwrap();
    assert_eq!(desc.kind, VariableKind::Static);
    assert_eq!(desc.scope, VariableScope::Global);
    assert_eq!(desc.is_mutable, Some(true));
    let ty = desc.type_annotation.as_ref().unwrap();
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
    let desc = map.get("wrapper::value").unwrap();
    assert_eq!(desc.kind, VariableKind::Binding);
    assert_eq!(desc.scope, VariableScope::Function);
    assert_eq!(desc.is_mutable, Some(true));

    let ty = desc.type_annotation.as_ref().unwrap();
    let generics = assert_path(ty, &["Option"]);
    assert_eq!(generics.len(), 1);
    let inner = &generics[0];
    let result_generics = assert_path(inner, &["Result"]);
    assert_eq!(result_generics.len(), 2);
    assert_path(&result_generics[0], &["i32"]);
    if let TypeExpr::Reference {
        is_mut,
        lifetime,
        inner,
    } = &result_generics[1]
    {
        assert!(!is_mut);
        assert_eq!(lifetime.as_deref(), Some("'static"));
        assert_path(inner, &["str"]);
    } else {
        panic!();
    }
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
