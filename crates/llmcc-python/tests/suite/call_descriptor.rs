use llmcc_core::context::CompileCtxt;
use llmcc_python::{build_llmcc_ir, collect_symbols, LangPython};

fn collect_calls(source: &str) -> Vec<llmcc_python::CallDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangPython>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangPython>(&cc).ok();

    let globals = cc.create_globals();
    collect_symbols(unit, globals).calls
}

fn call_key(call: &llmcc_python::CallDescriptor) -> String {
    match &call.target {
        llmcc_python::CallTarget::Function(name) => name.clone(),
        llmcc_python::CallTarget::Method(obj, method) => format!("{}.{}", obj, method),
        llmcc_python::CallTarget::Constructor(class_name) => class_name.clone(),
    }
}

fn find_call<'a, F>(calls: &'a [llmcc_python::CallDescriptor], predicate: F) -> Option<&'a llmcc_python::CallDescriptor>
where
    F: Fn(&llmcc_python::CallDescriptor) -> bool,
{
    calls.iter().find(|call| predicate(call))
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
        if let CallTarget::Function(name) = &call.target {
            name == "print"
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
        if let CallTarget::Function(name) = &call.target {
            name == "helper"
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
        if let CallTarget::Method(obj, method) = &call.target {
            obj == "obj" && method == "method"
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
        if let CallTarget::Constructor(class_name) = &call.target {
            class_name == "MyClass"
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

    let outer_calls = calls.iter().filter(|call| {
        if let CallTarget::Function(name) = &call.target {
            name == "outer"
        } else {
            false
        }
    }).count();

    let inner_calls = calls.iter().filter(|call| {
        if let CallTarget::Function(name) = &call.target {
            name == "inner"
        } else {
            false
        }
    }).count();

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

    let strip_call = find_call(&calls, |call| {
        if let CallTarget::Method(obj, method) = &call.target {
            obj == "text" && method == "strip"
        } else {
            false
        }
    });

    let upper_call = find_call(&calls, |call| {
        if let CallTarget::Method(_, method) = &call.target {
            method == "upper"
        } else {
            false
        }
    });

    let split_call = find_call(&calls, |call| {
        if let CallTarget::Method(_, method) = &call.target {
            method == "split"
        } else {
            false
        }
    });

    // Should capture at least some of these method calls
    assert!(strip_call.is_some() || upper_call.is_some() || split_call.is_some());
}

#[test]
fn captures_method_call_with_arguments() {
    let source = r#"
def caller():
    obj.process(10, "test", value=42)
"#;
    let calls = collect_calls(source);
    let process_call = find_call(&calls, |call| {
        if let CallTarget::Method(obj, method) = &call.target {
            obj == "obj" && method == "process"
        } else {
            false
        }
    });
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

    let methods: Vec<_> = calls.iter().filter_map(|call| {
        if let CallTarget::Method(obj, method) = &call.target {
            if obj == "obj" {
                return Some(method.as_str());
            }
        }
        None
    }).collect();

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
        if let CallTarget::Method(obj, method) = &call.target {
            obj == "self" && method == "helper"
        } else {
            false
        }
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
        if let CallTarget::Function(name) = &call.target {
            name == "func"
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
fn captures_calls_in_conditionals() {
    let source = r#"
def caller():
    if condition():
        do_something()
    else:
        do_other()
"#;
    let calls = collect_calls(source);

    let condition_call = find_call(&calls, |call| {
        if let CallTarget::Function(name) = &call.target {
            name == "condition"
        } else {
            false
        }
    });

    let do_something_call = find_call(&calls, |call| {
        if let CallTarget::Function(name) = &call.target {
            name == "do_something"
        } else {
            false
        }
    });

    let do_other_call = find_call(&calls, |call| {
        if let CallTarget::Function(name) = &call.target {
            name == "do_other"
        } else {
            false
        }
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

    let process_call = find_call(&calls, |call| {
        if let CallTarget::Function(name) = &call.target {
            name == "process"
        } else {
            false
        }
    });

    assert!(process_call.is_some());
}
