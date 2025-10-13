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
            CallTarget::Chain { base, segments } => {
                let mut s = base.clone();
                for seg in segments {
                    s.push_str(&format!(".{}", seg.method));
                }
                s
            }
            CallTarget::Unknown(text) => text.clone(),
        };
        *acc.entry(key).or_insert(0) += 1;
        acc
    });

    assert_eq!(counts.get("outer"), Some(&1));
    assert_eq!(counts.get("inner"), Some(&2));
}

#[test]
fn captures_lambda_argument() {
    let source = r#"
        fn wrapper() {
            let _ = apply(|x| x + 1, 5);
        }
    "#;
    let calls = collect_calls(source);
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.enclosing_function.as_deref(), Some("wrapper"));
    assert_eq!(call.arguments.len(), 2);
    assert_eq!(call.arguments[0].text, "|x| x + 1");
    assert_eq!(call.arguments[1].text, "5");
}

#[test]
fn captures_method_chain() {
    let source = r#"
        fn wrapper() {
            data.iter().map(|v| processor::handle(v)).collect::<Vec<_>>();
        }
    "#;
    let calls = collect_calls(source);
    // expect one chain call plus inner processor::handle
    assert!(calls.len() >= 2);

    let chain = calls
        .iter()
        .find(|call| matches!(call.target, CallTarget::Chain { .. }))
        .expect("chain call");

    match &chain.target {
        CallTarget::Chain { base, segments } => {
            assert_eq!(base, "data");
            assert_eq!(segments.len(), 3);
            assert_eq!(segments[0].method, "iter");
            assert!(segments[0].arguments.is_empty());
            assert_eq!(segments[1].method, "map");
            assert_eq!(segments[1].arguments.len(), 1);
            assert_eq!(segments[1].arguments[0].text, "|v| processor::handle(v)");
            assert_eq!(segments[2].method, "collect");
            assert!(segments[2].arguments.is_empty());
            assert_eq!(segments[2].generics.len(), 1);
        }
        other => panic!("expected chain target but found {other:?}"),
    }

    let handle = calls
        .iter()
        .find(|call| match &call.target {
            CallTarget::Path { segments, .. } => segments.join("::") == "processor::handle",
            _ => false,
        })
        .expect("processor::handle call");
    assert_eq!(handle.arguments.len(), 1);
    assert_eq!(handle.arguments[0].text, "v");
}
