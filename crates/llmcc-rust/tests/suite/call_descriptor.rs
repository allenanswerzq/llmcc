use std::collections::HashMap;

use llmcc_core::IrBuildConfig;
use llmcc_rust::{
    build_llmcc_ir, collect_symbols, CallDescriptor, CallTarget, ChainSegment, CompileCtxt,
    LangRust, TypeExpr,
};

fn collect_calls(source: &str) -> Vec<CallDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(&cc, IrBuildConfig::default()).unwrap();

    let globals = cc.create_globals();
    collect_symbols(unit, globals).calls
}

fn call_key(call: &CallDescriptor) -> String {
    match &call.target {
        CallTarget::Path { segments, .. } => segments.join("::"),
        CallTarget::Method { method, .. } => method.clone(),
        CallTarget::Chain { base, segments } => {
            let mut key = base.clone();
            for segment in segments {
                key.push('.');
                key.push_str(&segment.method);
            }
            key
        }
        CallTarget::Unknown(text) => text.clone(),
    }
}

fn path_target(call: &CallDescriptor) -> (&Vec<String>, &Vec<TypeExpr>) {
    let CallTarget::Path { segments, generics } = &call.target else {
        panic!();
    };
    (segments, generics)
}

fn method_target(call: &CallDescriptor) -> (&String, &String, &Vec<TypeExpr>) {
    let CallTarget::Method {
        receiver,
        method,
        generics,
    } = &call.target
    else {
        panic!();
    };
    (receiver, method, generics)
}

fn chain_target(call: &CallDescriptor) -> (&String, &Vec<ChainSegment>) {
    let CallTarget::Chain { base, segments } = &call.target else {
        panic!();
    };
    (base, segments)
}

fn find_call<F>(calls: &[CallDescriptor], predicate: F) -> &CallDescriptor
where
    F: Fn(&CallDescriptor) -> bool,
{
    calls.iter().find(|call| predicate(call)).unwrap()
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
    let (segments, _) = path_target(call);
    assert_eq!(segments, &vec!["foo".to_string(), "bar".to_string()]);
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
    let (receiver, method, generics) = method_target(call);
    assert_eq!(receiver, "value");
    assert_eq!(method, "compute");
    assert_eq!(generics.len(), 1);
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
        let key = call_key(call);
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

    let chain = find_call(&calls, |call| {
        matches!(call.target, CallTarget::Chain { .. })
    });

    let (base, segments) = chain_target(chain);
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

    let handle = find_call(&calls, |call| {
        matches!(
            &call.target,
            CallTarget::Path { segments, .. } if segments.join("::") == "processor::handle"
        )
    });
    assert_eq!(handle.arguments.len(), 1);
    assert_eq!(handle.arguments[0].text, "v");
}

#[test]
fn captures_deep_nested_calls() {
    let source = r#"
        fn wrapper() {
            let _ = outer(inner_a(inner_b(inner_c(inner_d(0)))));
        }
    "#;

    let calls = collect_calls(source);
    assert_eq!(calls.len(), 5);

    let call_map: HashMap<_, _> = calls
        .iter()
        .filter_map(|call| {
            if matches!(call.target, CallTarget::Path { .. }) {
                Some((call_key(call), call))
            } else {
                None
            }
        })
        .collect();

    let outer = call_map.get("outer").unwrap();
    assert_eq!(outer.arguments.len(), 1);
    assert_eq!(
        outer.arguments[0].text,
        "inner_a(inner_b(inner_c(inner_d(0))))"
    );

    let inner_a = call_map.get("inner_a").unwrap();
    assert_eq!(inner_a.arguments.len(), 1);
    assert_eq!(inner_a.arguments[0].text, "inner_b(inner_c(inner_d(0)))");

    let inner_b = call_map.get("inner_b").unwrap();
    assert_eq!(inner_b.arguments.len(), 1);
    assert_eq!(inner_b.arguments[0].text, "inner_c(inner_d(0))");

    let inner_c = call_map.get("inner_c").unwrap();
    assert_eq!(inner_c.arguments.len(), 1);
    assert_eq!(inner_c.arguments[0].text, "inner_d(0)");

    let inner_d = call_map.get("inner_d").unwrap();
    assert_eq!(inner_d.arguments.len(), 1);
    assert_eq!(inner_d.arguments[0].text, "0");
}

#[test]
fn captures_generic_path_call() {
    let source = r#"
        fn wrapper() {
            compute::apply::<Result<i32, i64>, (usize, usize)>(build_value(), 99);
        }
    "#;

    let calls = collect_calls(source);
    let apply = find_call(&calls, |call| {
        matches!(
            &call.target,
            CallTarget::Path { segments, .. } if segments.join("::") == "compute::apply"
        )
    });

    let (_, generics) = path_target(apply);
    assert_eq!(generics.len(), 2);
    if let TypeExpr::Path { segments, generics } = &generics[0] {
        assert_eq!(segments, &vec!["Result".to_string()]);
        assert_eq!(generics.len(), 2);
    } else {
        panic!();
    }
    if let TypeExpr::Tuple(items) = &generics[1] {
        assert_eq!(items.len(), 2);
    } else {
        panic!();
    }

    assert_eq!(apply.arguments.len(), 2);
    assert_eq!(apply.arguments[0].text, "build_value()");
    assert_eq!(apply.arguments[1].text, "99");
}
