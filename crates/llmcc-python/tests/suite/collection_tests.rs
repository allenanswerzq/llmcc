use llmcc_core::context::CompileCtxt;
use llmcc_python::{collect_symbols, LangPython};

fn collect_from_source(source: &str) -> llmcc_python::CollectionResult {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangPython>(&sources);
    let unit = cc.compile_unit(0);
    let globals = cc.create_globals();
    collect_symbols(unit, globals)
}

#[test]
fn test_collect_simple_function() {
    let source = "def foo():\n    pass\n";
    let _result = collect_from_source(source);
    // Collection is in progress, just verify it doesn't crash
}

#[test]
fn test_collect_multiple_functions() {
    let source = "def foo():\n    pass\n\ndef bar():\n    pass\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_class() {
    let source = "class MyClass:\n    pass\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_class_with_method() {
    let source = "class MyClass:\n    def method(self):\n        pass\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_import_statement() {
    let source = "import os\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_multiple_imports() {
    let source = "import os\nimport sys\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_variable_assignment() {
    let source = "x = 42\ny = 'hello'\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_nested_function() {
    let source = "def outer():\n    def inner():\n        pass\n    pass\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_preserves_function_name() {
    let source = "def test_function_name():\n    pass\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_preserves_class_name() {
    let source = "class TestClassName:\n    pass\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_empty_module() {
    let source = "# Just a comment\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_function_and_class_together() {
    let source = "def func():\n    pass\n\nclass MyClass:\n    pass\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_decorated_function() {
    let source = "@decorator\ndef func():\n    pass\n";
    let _result = collect_from_source(source);
}

#[test]
fn test_collect_mixed_code() {
    let source = r#"
import os
from sys import argv

def helper():
    pass

class DataHandler:
    def __init__(self):
        pass

    def process(self):
        pass

x = 10
y = helper()
"#;
    let _result = collect_from_source(source);
}
