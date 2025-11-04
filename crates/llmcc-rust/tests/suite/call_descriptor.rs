use std::collections::HashMap;

use llmcc_core::IrBuildConfig;
use llmcc_descriptor::{CallChain, CallDescriptor, CallKind, CallSymbol, CallTarget, TypeExpr};
use llmcc_rust::{build_llmcc_ir, collect_symbols, CompileCtxt, LangRust};

fn collect_calls(source: &str) -> Vec<CallDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(&cc, IrBuildConfig).unwrap();

    let globals = cc.create_globals();
    collect_symbols(unit, globals).calls
}

fn call_key(call: &CallDescriptor) -> String {
    match &call.target {
        CallTarget::Symbol(symbol) => {
            let mut parts = symbol.qualifiers.clone();
            parts.push(symbol.name.clone());
            parts.join("::")
        }
        CallTarget::Chain(chain) => {
            let mut key = chain.root.clone();
            for segment in &chain.segments {
                key.push('.');
                key.push_str(&segment.name);
            }
            key
        }
        CallTarget::Dynamic { repr } => repr.clone(),
    }
}

fn symbol_target(call: &CallDescriptor) -> &CallSymbol {
    match &call.target {
        CallTarget::Symbol(symbol) => symbol,
        _ => panic!("expected symbol target"),
    }
}

fn chain_target(call: &CallDescriptor) -> &CallChain {
    match &call.target {
        CallTarget::Chain(chain) => chain,
        _ => panic!("expected chain target"),
    }
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
    let symbol = symbol_target(call);
    assert_eq!(symbol.qualifiers, vec!["foo".to_string()]);
    assert_eq!(symbol.name, "bar");
    assert_eq!(call.arguments.len(), 2);
    assert_eq!(call.arguments[0].value, "1");
    assert_eq!(call.arguments[1].value, "2");
    assert_eq!(call.enclosing.as_deref(), Some("wrapper"));
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
    let chain = chain_target(call);
    assert_eq!(chain.root, "value");
    assert_eq!(chain.segments.len(), 1);
    let segment = &chain.segments[0];
    assert_eq!(segment.name, "compute");
    assert_eq!(segment.kind, CallKind::Method);
    assert_eq!(segment.type_arguments.len(), 1);
    assert_eq!(call.arguments[0].value, "10");
}

#[test]
fn captures_nested_calls() {
    let source = r#"
        fn wrapper() {
            outer(inner(1), inner(2));
        }
    "#;
    let calls = collect_calls(source);
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
    assert_eq!(call.enclosing.as_deref(), Some("wrapper"));
    assert_eq!(call.arguments.len(), 2);
    assert_eq!(call.arguments[0].value, "|x| x + 1");
    assert_eq!(call.arguments[1].value, "5");
}

#[test]
fn captures_method_chain() {
    let source = r#"
        fn wrapper() {
            data.iter().map(|v| processor::handle(v)).collect::<Vec<_>>();
        }
    "#;
    let calls = collect_calls(source);
    assert!(calls.len() >= 2);

    let chain = find_call(&calls, |call| matches!(call.target, CallTarget::Chain(_)));
    let chain = chain_target(chain);
    assert_eq!(chain.root, "data");
    assert_eq!(chain.segments.len(), 3);
    assert_eq!(chain.segments[0].name, "iter");
    assert_eq!(chain.segments[0].kind, CallKind::Method);
    assert!(chain.segments[0].arguments.is_empty());
    assert_eq!(chain.segments[1].name, "map");
    assert_eq!(chain.segments[1].arguments.len(), 1);
    assert_eq!(
        chain.segments[1].arguments[0].value,
        "|v| processor::handle(v)"
    );
    assert_eq!(chain.segments[2].name, "collect");
    assert!(chain.segments[2].arguments.is_empty());
    assert_eq!(chain.segments[2].type_arguments.len(), 1);

    let handle = find_call(&calls, |call| {
        matches!(
            &call.target,
            CallTarget::Symbol(symbol)
                if symbol
                    .qualifiers
                    .iter()
                    .chain(std::iter::once(&symbol.name))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("::")
                    == "processor::handle"
        )
    });
    assert_eq!(handle.arguments.len(), 1);
    assert_eq!(handle.arguments[0].value, "v");
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
            if matches!(call.target, CallTarget::Symbol(_)) {
                Some((call_key(call), call))
            } else {
                None
            }
        })
        .collect();

    let outer = call_map.get("outer").unwrap();
    assert_eq!(outer.arguments.len(), 1);
    assert_eq!(
        outer.arguments[0].value,
        "inner_a(inner_b(inner_c(inner_d(0))))"
    );

    let inner_a = call_map.get("inner_a").unwrap();
    assert_eq!(inner_a.arguments.len(), 1);
    assert_eq!(inner_a.arguments[0].value, "inner_b(inner_c(inner_d(0)))");

    let inner_b = call_map.get("inner_b").unwrap();
    assert_eq!(inner_b.arguments.len(), 1);
    assert_eq!(inner_b.arguments[0].value, "inner_c(inner_d(0))");

    let inner_c = call_map.get("inner_c").unwrap();
    assert_eq!(inner_c.arguments.len(), 1);
    assert_eq!(inner_c.arguments[0].value, "inner_d(0)");

    let inner_d = call_map.get("inner_d").unwrap();
    assert_eq!(inner_d.arguments.len(), 1);
    assert_eq!(inner_d.arguments[0].value, "0");
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
        matches!(&call.target, CallTarget::Symbol(symbol) if {
            let mut parts = symbol.qualifiers.clone();
            parts.push(symbol.name.clone());
            parts.join("::") == "compute::apply"
        })
    });

    let symbol = symbol_target(apply);
    assert_eq!(symbol.type_arguments.len(), 2);
    if let TypeExpr::Path { segments, generics } = &symbol.type_arguments[0] {
        assert_eq!(segments, &vec!["Result".to_string()]);
        assert_eq!(generics.len(), 2);
    } else {
        panic!();
    }
    if let TypeExpr::Tuple(items) = &symbol.type_arguments[1] {
        assert_eq!(items.len(), 2);
    } else {
        panic!();
    }

    assert_eq!(apply.arguments.len(), 2);
    assert_eq!(apply.arguments[0].value, "build_value()");
    assert_eq!(apply.arguments[1].value, "99");
}
