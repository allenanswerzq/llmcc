use std::collections::HashMap;

use llmcc_rust::function::{FnVisibility, FunctionOwner};
use llmcc_rust::{build_llmcc_ir, collect_symbols, GlobalCtxt, HirId, LangRust, SymbolRegistry};

fn collect(source: &str) -> HashMap<String, llmcc_rust::function::FunctionDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let gcx = GlobalCtxt::from_sources::<LangRust>(&sources);
    let ctx = gcx.create_context(0);
    let tree = ctx.tree();
    build_llmcc_ir::<LangRust>(&tree, ctx).expect("build HIR");
    let mut registry = SymbolRegistry::default();
    collect_symbols(HirId(0), ctx, &mut registry)
        .into_iter()
        .map(|desc| (desc.fqn.clone(), desc))
        .collect()
}

#[test]
fn detects_private_function() {
    let map = collect("fn foo() {}\n");
    let foo = map.get("foo").expect("foo");
    assert_eq!(foo.visibility, FnVisibility::Private);
    assert!(foo.parameters.is_empty());
    assert!(foo.return_type.is_none());
}

#[test]
fn detects_public_visibility() {
    let map = collect("pub fn foo() {}\n");
    assert_eq!(map.get("foo").unwrap().visibility, FnVisibility::Public);
}

#[test]
fn detects_pub_crate_visibility() {
    let map = collect("pub(crate) fn foo() {}\n");
    assert_eq!(map.get("foo").unwrap().visibility, FnVisibility::Crate);
}

#[test]
fn captures_parameters_and_return_type() {
    let source = r#"
        fn transform(value: i32, label: Option<&str>) -> Result<i32, &'static str> {
            Ok(value)
        }
    "#;
    let map = collect(source);
    let desc = map.get("transform").unwrap();
    assert_eq!(desc.parameters.len(), 2);
    assert_eq!(desc.parameters[0].pattern, "value");
    assert_eq!(desc.parameters[0].ty.as_deref(), Some("i32"));
    assert_eq!(desc.parameters[1].ty.as_deref(), Some("Option<&str>"));
    assert_eq!(
        desc.return_type.as_deref(),
        Some("Result<i32, &'static str>")
    );
}

#[test]
fn captures_async_const_and_unsafe_flags() {
    let source = r#"
        async unsafe fn perform() {}
        const fn build() -> i32 { 0 }
    "#;
    let map = collect(source);

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
    let map = collect(source);
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
    let map = collect(source);
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
    let map = collect(source);
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
    let map = collect(source);
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
    let map = collect(source);
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
    let map = collect(source);
    let make_adder = map.get("make_adder").unwrap();
    assert_eq!(
        make_adder.return_type.as_deref(),
        Some("impl Fn(i32) -> i32")
    );
}
