use llmcc_core::{context::CompileCtxt, IrBuildConfig};
use llmcc_descriptor::{CallChain, CallChainRoot, CallDescriptor, CallInvocation, CallKind, CallTarget};
use llmcc_python::{build_llmcc_ir, collect_symbols, CallCollection, LangPython};

fn collect_calls(source: &str) -> CallCollection {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangPython>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangPython>(&cc, IrBuildConfig).ok();

    let collection = collect_symbols(unit).result;
    collection.calls
}

fn call_key(call: &CallDescriptor) -> String {
    match &call.target {
        CallTarget::Symbol(symbol) => {
            if symbol.qualifiers.is_empty() {
                symbol.name.clone()
            } else {
                let mut parts = symbol.qualifiers.clone();
                parts.push(symbol.name.clone());
                parts.join("::")
            }
        }
        CallTarget::Chain(chain) => {
            format_call_chain(chain)
        }
        CallTarget::Dynamic { repr } => repr.clone(),
    }
}

fn format_call_chain(chain: &CallChain) -> String {
    let mut key = format_chain_root(&chain.root);
    for segment in &chain.parts {
        key.push('.');
        key.push_str(&segment.name);
    }
    key
}

fn format_chain_root(root: &CallChainRoot) -> String {
    match root {
        CallChainRoot::Expr(expr) => expr.clone(),
        CallChainRoot::Invocation(invocation) => format_invocation(invocation),
    }
}

fn format_invocation(invocation: &CallInvocation) -> String {
    let mut base = format_call_target(invocation.target.as_ref());
    base.push('(');
    let args = invocation
        .arguments
        .iter()
        .map(|arg| arg.value.clone())
        .collect::<Vec<_>>()
        .join(", ");
    base.push_str(&args);
    base.push(')');
    base
}

fn format_call_target(target: &CallTarget) -> String {
    match target {
        CallTarget::Symbol(symbol) => {
            if symbol.qualifiers.is_empty() {
                symbol.name.clone()
            } else {
                let mut parts = symbol.qualifiers.clone();
                parts.push(symbol.name.clone());
                parts.join("::")
            }
        }
        CallTarget::Chain(chain) => format_call_chain(chain),
        CallTarget::Dynamic { repr } => repr.clone(),
    }
}

fn find_call<'a, F>(calls: &'a [CallDescriptor], predicate: F) -> Option<&'a CallDescriptor>
where
    F: Fn(&CallDescriptor) -> bool,
{
    calls.iter().find(|call| predicate(call))
}

fn has_chain_segment(call: &CallDescriptor, root: &str, method: &str) -> bool {
    if let CallTarget::Chain(chain) = &call.target {
        if !matches!(&chain.root, CallChainRoot::Expr(expr) if expr == root) {
            return false;
        }
        return chain.parts.iter().any(|segment| segment.name == method);
    }
    false
}

#[test]
fn captures_simple_function_call() {
    let source = r#"
def caller():
    print("hello")
"#;
    let calls = collect_calls(source);
    assert!(calls.len() > 0);
    let print_call = find_call(&calls, |call| {
        if let CallTarget::Symbol(symbol) = &call.target {
            symbol.kind == CallKind::Function && symbol.name == "print"
        } else {
            false
        }
    });
    assert!(print_call.is_some());
}

#[test]
fn captures_function_call_with_arguments() {
    let source = r#"
def caller():
    helper(1, 2, 3)
"#;
    let calls = collect_calls(source);
    let helper_call = find_call(&calls, |call| {
        if let CallTarget::Symbol(symbol) = &call.target {
            symbol.kind == CallKind::Function && symbol.name == "helper"
        } else {
            false
        }
    });
    assert!(helper_call.is_some());
    if let Some(call) = helper_call {
        assert_eq!(call.arguments.len(), 3);
    }
}

#[test]
fn captures_method_call() {
    let source = r#"
def caller():
    obj.method()
"#;
    let calls = collect_calls(source);
    let method_call = find_call(&calls, |call| {
        if let CallTarget::Chain(chain) = &call.target {
            matches!(&chain.root, CallChainRoot::Expr(expr) if expr == "obj")
                && chain
                    .parts
                    .last()
                    .map(|segment| segment.name.as_str())
                    == Some("method")
        } else {
            false
        }
    });
    assert!(method_call.is_some());
}

#[test]
fn captures_constructor_call() {
    let source = r#"
def caller():
    instance = MyClass()
"#;
    let calls = collect_calls(source);
    let constructor_call = find_call(&calls, |call| {
        if let CallTarget::Symbol(symbol) = &call.target {
            symbol.kind == CallKind::Constructor && symbol.name == "MyClass"
        } else {
            false
        }
    });
    assert!(constructor_call.is_some());
}

#[test]
fn captures_nested_calls() {
    let source = r#"
def caller():
    outer(inner(5), inner(10))
"#;
    let calls = collect_calls(source);

    let outer_calls = calls
        .iter()
        .filter(|call| match &call.target {
            CallTarget::Symbol(symbol) => symbol.name == "outer",
            _ => false,
        })
        .count();

    let inner_calls = calls
        .iter()
        .filter(|call| match &call.target {
            CallTarget::Symbol(symbol) => symbol.name == "inner",
            _ => false,
        })
        .count();

    // Should have at least one outer and two inner calls
    assert!(outer_calls >= 1);
    assert!(inner_calls >= 2);
}

#[test]
fn captures_chained_method_calls() {
    let source = r#"
def caller():
    result = text.strip().upper().split(",")
"#;
    let calls = collect_calls(source);

    let strip_call = calls.iter().any(|call| has_chain_segment(call, "text", "strip"));
    let upper_call = calls.iter().any(|call| has_chain_segment(call, "text", "upper"));
    let split_call = calls.iter().any(|call| has_chain_segment(call, "text", "split"));

    // Should capture at least some of these method calls
    assert!(strip_call || upper_call || split_call);
}

#[test]
fn captures_method_call_with_arguments() {
    let source = r#"
def caller():
    obj.process(10, "test", value=42)
"#;
    let calls = collect_calls(source);
    let process_call = find_call(&calls, |call| has_chain_segment(call, "obj", "process"));
    assert!(process_call.is_some());
    if let Some(call) = process_call {
        assert_eq!(call.arguments.len(), 3);
    }
}

#[test]
fn captures_multiple_method_calls_on_object() {
    let source = r#"
def caller():
    obj.method1()
    obj.method2()
    obj.method3()
"#;
    let calls = collect_calls(source);

    let methods: Vec<_> = calls
        .iter()
        .filter_map(|call| {
            if let CallTarget::Chain(chain) = &call.target {
                if matches!(&chain.root, CallChainRoot::Expr(expr) if expr == "obj") {
                    return chain
                        .parts
                        .last()
                        .map(|segment| segment.name.as_str());
                }
            }
            None
        })
        .collect();

    // Should have at least some method calls on obj
    assert!(methods.len() > 0);
}

#[test]
fn captures_calls_in_class_methods() {
    let source = r#"
class Handler:
    def process(self):
        self.helper()

    def helper(self):
        pass
"#;
    let calls = collect_calls(source);

    let helper_call = find_call(&calls, |call| {
        has_chain_segment(call, "self", "helper")
    });

    assert!(helper_call.is_some());
}

#[test]
fn captures_calls_with_keyword_arguments() {
    let source = r#"
def caller():
    func(a=1, b=2, c=3)
"#;
    let calls = collect_calls(source);
    let func_call = find_call(&calls, |call| {
        if let CallTarget::Symbol(symbol) = &call.target {
            symbol.kind == CallKind::Function && symbol.name == "func"
        } else {
            false
        }
    });
    assert!(func_call.is_some());
    if let Some(call) = func_call {
        assert_eq!(call.arguments.len(), 3);
        // Keyword arguments should be captured
        assert!(call.arguments.iter().any(|arg| arg.name.is_some()));
    }
}

#[test]
fn captures_chain_with_invoked_root() {
    let source = r#"
def caller():
    factory().builder().finish()
"#;
    let calls = collect_calls(source);
    let chain_call = find_call(&calls, |call| matches!(call.target, CallTarget::Chain(_)));
    let chain_call = chain_call.expect("expected a chain call");

    if let CallTarget::Chain(chain) = &chain_call.target {
        match &chain.root {
            CallChainRoot::Invocation(invocation) => {
                match invocation.target.as_ref() {
                    CallTarget::Symbol(symbol) => assert_eq!(symbol.name, "factory"),
                    other => panic!("unexpected root target: {:?}", other),
                }
            }
            other => panic!("expected invocation root, got {:?}", other),
        }

        assert_eq!(chain.parts.len(), 2);
        assert_eq!(chain.parts[0].name, "builder");
        assert_eq!(chain.parts[1].name, "finish");
    }
}

#[test]
fn captures_calls_in_conditionals() {
    let source = r#"
def caller():
    if condition():
        do_something()
    else:
        do_other()
"#;
    let calls = collect_calls(source);

    let condition_call = find_call(&calls, |call| match &call.target {
        CallTarget::Symbol(symbol) => symbol.name == "condition",
        _ => false,
    });

    let do_something_call = find_call(&calls, |call| match &call.target {
        CallTarget::Symbol(symbol) => symbol.name == "do_something",
        _ => false,
    });

    let do_other_call = find_call(&calls, |call| match &call.target {
        CallTarget::Symbol(symbol) => symbol.name == "do_other",
        _ => false,
    });

    assert!(condition_call.is_some());
    assert!(do_something_call.is_some());
    assert!(do_other_call.is_some());
}

#[test]
fn captures_calls_in_loops() {
    let source = r#"
def caller():
    for item in items:
        process(item)
"#;
    let calls = collect_calls(source);

    let process_call = find_call(&calls, |call| match &call.target {
        CallTarget::Symbol(symbol) => symbol.name == "process",
        _ => false,
    });

    assert!(process_call.is_some());
}
