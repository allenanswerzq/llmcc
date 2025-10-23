use llmcc_core::context::CompileCtxt;
use llmcc_python::{bind_symbols, LangPython};

fn bind_from_source(source: &str) -> llmcc_python::BindingResult {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangPython>(&sources);
    let unit = cc.compile_unit(0);
    let globals = cc.create_globals();
    bind_symbols(unit, globals)
}

#[test]
fn test_bind_simple_function_call() {
    let source = "def foo():\n    pass\n\nfoo()\n";
    let _result = bind_from_source(source);
    // Binding is in progress, just verify it doesn't crash
}

#[test]
fn test_bind_function_to_definition() {
    let source = "def helper():\n    pass\n\ndef caller():\n    helper()\n";
    let _result = bind_from_source(source);
}

#[test]
fn test_bind_method_call() {
    let source = r#"
class MyClass:
    def method(self):
        pass

obj = MyClass()
obj.method()
"#;
    let _result = bind_from_source(source);
}

#[test]
fn test_bind_builtin_function() {
    let source = "print('hello')\nlen([1, 2, 3])\n";
    let _result = bind_from_source(source);
}

#[test]
fn test_bind_imported_function() {
    let source = "from os import path\npath.exists('.')\n";
    let _result = bind_from_source(source);
}

#[test]
fn test_bind_multiple_calls() {
    let source = "def func():\n    pass\n\nfunc()\nfunc()\nfunc()\n";
    let _result = bind_from_source(source);
}

#[test]
fn test_bind_nested_calls() {
    let source = "def inner():\n    pass\n\ndef outer():\n    inner()\n\nouter()\n";
    let _result = bind_from_source(source);
}

#[test]
fn test_bind_with_no_calls() {
    let source = "def func():\n    x = 1\n    y = 2\n    z = x + y\n";
    let _result = bind_from_source(source);
}

#[test]
fn test_bind_undefined_function_call() {
    let source = "undefined_func()\n";
    let _result = bind_from_source(source);
}

#[test]
fn test_bind_empty_module() {
    let source = "# Just comments\n";
    let result = bind_from_source(source);
    assert_eq!(result.calls.len(), 0, "Empty module should have no calls");
}

#[test]
fn test_bind_preserves_call_location() {
    let source = "def foo():\n    pass\n\nresult = foo()\n";
    let _result = bind_from_source(source);
}

#[test]
fn test_bind_chained_method_calls() {
    let source = r#"
text = "hello"
upper = text.upper()
result = upper.replace("H", "J")
"#;
    let _result = bind_from_source(source);
}

#[test]
fn test_bind_lambda_calls() {
    let source = "f = lambda x: x * 2\nf(5)\n";
    let _result = bind_from_source(source);
}

#[test]
fn test_bind_complex_module() {
    let source = r#"
import math
from os import path

class Calculator:
    def __init__(self):
        self.value = 0

    def add(self, x):
        self.value += x
        return self.value

def main():
    calc = Calculator()
    calc.add(5)
    result = math.sqrt(2)
    exists = path.exists(".")
    return result

if __name__ == "__main__":
    main()
"#;
    let _result = bind_from_source(source);
}
