use std::collections::HashMap;

use llmcc_rust::{
    build_llmcc_ir, collect_symbols, FnVisibility, FunctionOwner, CompileCtxt, LangRust, TypeExpr,
};

fn collect_functions(source: &str) -> HashMap<String, llmcc_rust::FunctionDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let unit = cc.compile_unit(0);
    let tree = unit.tree();
    build_llmcc_ir::<LangRust>(&tree, unit).expect("build HIR");
    let root = unit.file_start_hir_id().expect("registered root id");
    let globals = cc.alloc_scope(root);
    collect_symbols(root, unit, globals)
        .functions
        .into_iter()
        .map(|desc| (desc.fqn.clone(), desc))
        .collect()
}

#[test]
fn detects_private_function() {
    let map = collect_functions("fn foo() {}\n");
    let foo = map.get("foo").expect("foo");
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

    let param0 = desc.parameters[0].ty.as_ref().expect("param0 type");
    assert_path(param0, &["i32"]);

    let param1 = desc.parameters[1].ty.as_ref().expect("param1 type");
    let generics = assert_path(param1, &["Option"]);
    assert_eq!(generics.len(), 1);
    let inner = &generics[0];
    match inner {
        TypeExpr::Reference {
            is_mut,
            lifetime,
            inner,
        } => {
            assert!(!is_mut);
            assert!(lifetime.is_none());
            assert_path(inner, &["str"]);
        }
        other => panic!("unexpected type: {other:?}"),
    }

    let return_type = desc.return_type.as_ref().expect("return type");
    let generics = assert_path(return_type, &["Result"]);
    assert_eq!(generics.len(), 2);
    assert_path(&generics[0], &["i32"]);
    match &generics[1] {
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

#[test]
fn resolves_module_owner() {
    let source = r#"
        mod outer {
            pub fn inner() {}
        }
    "#;
    let map = collect_functions(source);
    let inner = map.get("outer::inner").unwrap();
    match &inner.owner {
        FunctionOwner::Free { modules } => assert_eq!(modules, &vec!["outer".to_string()]),
        other => panic!("unexpected owner: {other:?}"),
    }
}

#[test]
fn resolves_impl_method_owner() {
    let source = r#"
        struct Foo;
        impl Foo {
            fn method(&self, v: i32) -> i32 { v }
        }
    "#;
    let map = collect_functions(source);
    let method = map.get("Foo::method").unwrap();
    match &method.owner {
        FunctionOwner::Impl {
            modules,
            self_ty,
            trait_name,
        } => {
            assert!(modules.is_empty());
            assert_eq!(self_ty, "Foo");
            assert!(trait_name.is_none());
        }
        other => panic!("unexpected owner: {other:?}"),
    }
    assert_eq!(method.parameters[0].pattern, "&self");
}

#[test]
fn resolves_trait_default_method() {
    let source = r#"
        trait MyTrait {
            fn provided(&self) {}
        }
    "#;
    let map = collect_functions(source);
    let provided = map.get("MyTrait::provided").unwrap();
    match &provided.owner {
        FunctionOwner::Trait {
            trait_name,
            modules,
        } => {
            assert_eq!(trait_name, "MyTrait");
            assert!(modules.is_empty());
        }
        other => panic!("unexpected owner: {other:?}"),
    }
}

#[test]
fn resolves_trait_impl_method() {
    let source = r#"
        struct Foo;
        trait MyTrait { fn required(&self); }
        impl MyTrait for Foo {
            fn required(&self) {}
        }
    "#;
    let map = collect_functions(source);
    let required = map.get("Foo::required").unwrap();
    match &required.owner {
        FunctionOwner::Impl {
            modules,
            self_ty,
            trait_name,
        } => {
            assert!(modules.is_empty());
            assert_eq!(self_ty, "Foo");
            assert_eq!(trait_name.as_deref(), Some("MyTrait"));
        }
        other => panic!("unexpected owner: {other:?}"),
    }
}

#[test]
fn captures_generic_information() {
    let source = r#"
        fn max<T: Ord>(a: T, b: T) -> T {
            if a >= b { a } else { b }
        }
    "#;
    let map = collect_functions(source);
    let max = map.get("max").unwrap();
    assert_eq!(max.generics.as_deref(), Some("<T: Ord>"));
    assert_eq!(max.parameters.len(), 2);
}

#[test]
fn captures_closure_return_types() {
    let source = r#"
        fn make_adder(y: i32) -> impl Fn(i32) -> i32 {
            move |x| x + y
        }
    "#;
    let map = collect_functions(source);
    let make_adder = map.get("make_adder").unwrap();
    let return_type = make_adder.return_type.as_ref().expect("return type");
    match return_type {
        TypeExpr::ImplTrait { bounds } => assert_eq!(bounds, "impl Fn(i32) -> i32"),
        other => panic!("unexpected return type: {other:?}"),
    }
}

#[test]
fn captures_where_clause_and_signature() {
    let source = r#"
        pub async fn compose<F>(f: F) -> impl Fn(i32) -> i32
        where
            F: Fn(i32) -> i32 + Clone,
        {
            move |x| f(f(f(f(x))))
        }
    "#;

    let map = collect_functions(source);
    let compose = map.get("compose").expect("compose function");
    assert!(compose.is_async);
    assert_eq!(
        compose.where_clause.as_deref(),
        Some("where F: Fn(i32) -> i32 + Clone")
    );
    assert!(
        compose
            .signature
            .starts_with("pub async fn compose<F>(f: F) -> impl Fn(i32) -> i32"),
        "unexpected signature: {}",
        compose.signature
    );
    assert_eq!(compose.parameters.len(), 1);
    assert_eq!(compose.parameters[0].pattern, "f");
}

#[test]
fn resolves_deep_module_owner_chain() {
    let source = r#"
        mod a {
            pub mod b {
                pub mod c {
                    pub fn leaf() {}
                }
            }
        }
    "#;

    let map = collect_functions(source);
    let leaf = map.get("a::b::c::leaf").expect("leaf function");
    match &leaf.owner {
        FunctionOwner::Free { modules } => {
            assert_eq!(
                modules,
                &vec!["a".to_string(), "b".to_string(), "c".to_string()]
            );
        }
        other => panic!("unexpected owner: {other:?}"),
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
