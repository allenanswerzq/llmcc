use std::collections::HashMap;

use llmcc_rust::{
    build_llmcc_ir, collect_symbols, CallDescriptor, CallTarget, GlobalCtxt, HirId, LangRust,
    SymbolRegistry,
};

fn collect_calls(source: &str) -> Vec<CallDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let gcx = GlobalCtxt::from_sources::<LangRust>(&sources);
    let ctx = gcx.file_context(0);
    let tree = ctx.tree();
    build_llmcc_ir::<LangRust>(&tree, ctx).expect("build HIR");
    let mut registry = SymbolRegistry::default();
    collect_symbols(HirId(0), ctx, &mut registry).calls
}

#[test]
fn captures_simple_call() {
    let source = r#"
        fn wrapper() {
            foo::bar(1, 2);
        }
    "#;
    let calls = collect_calls(source);
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    match &call.target {
        CallTarget::Path { segments, .. } => {
            assert_eq!(segments, &vec!["foo".to_string(), "bar".to_string()]);
        }
        other => panic!("unexpected target: {other:?}"),
    }
    assert_eq!(call.arguments.len(), 2);
    assert_eq!(call.arguments[0].text, "1");
    assert_eq!(call.arguments[1].text, "2");
    assert_eq!(call.enclosing_function.as_deref(), Some("wrapper"));
}

#[test]
fn captures_method_call() {
    let source = r#"
        fn wrapper() {
            value.compute::<usize>(10);
        }
    "#;
    let calls = collect_calls(source);
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    match &call.target {
        CallTarget::Method {
            receiver,
            method,
            generics,
        } => {
            assert_eq!(receiver, "value");
            assert!(method.starts_with("compute"));
            assert!(method.contains("::<usize>"));
            assert!(generics.is_empty() || generics.len() == 1);
        }
        other => panic!("unexpected target: {other:?}"),
    }
    // Ensure argument captured
    assert_eq!(call.arguments[0].text, "10");
}

#[test]
fn captures_nested_calls() {
    let source = r#"
        fn wrapper() {
            outer(inner(1), inner(2));
        }
    "#;
    let calls = collect_calls(source);
    // Expect three calls: two inners and the outer
    assert_eq!(calls.len(), 3);

    let counts = calls.iter().fold(HashMap::new(), |mut acc, call| {
        let key = match &call.target {
            CallTarget::Path { segments, .. } => segments.join("::"),
            CallTarget::Method { method, .. } => method.clone(),
            CallTarget::Unknown(text) => text.clone(),
        };
        *acc.entry(key).or_insert(0) += 1;
        acc
    });

    assert_eq!(counts.get("outer"), Some(&1));
    assert_eq!(counts.get("inner"), Some(&2));
}
