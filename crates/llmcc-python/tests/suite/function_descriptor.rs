use llmcc_core::context::CompileCtxt;
use llmcc_python::{collect_symbols, LangPython, PythonFunctionDescriptor};

fn collect_functions(source: &str) -> Vec<PythonFunctionDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangPython>(&sources);
    let unit = cc.compile_unit(0);

    let globals = cc.create_globals();
    collect_symbols(unit, globals).functions
}

#[test]
fn collects_simple_function() {
    let source = "def foo():\n    pass\n";
    let functions = collect_functions(source);
    assert_eq!(functions.len(), 1);
    let desc = &functions[0];
    assert_eq!(desc.name, "foo");
    assert!(desc.parameters.is_empty());
    assert!(desc.return_type.is_none());
    assert!(desc.decorators.is_empty());
}

#[test]
fn captures_function_with_parameters() {
    let source = r#"
def greet(name, age):
    pass
"#;
    let functions = collect_functions(source);
    let desc = functions.iter().find(|f| f.name == "greet").unwrap();
    assert_eq!(desc.parameters.len(), 2);
    assert_eq!(desc.parameters[0].name, "name");
    assert_eq!(desc.parameters[1].name, "age");
}

#[test]
fn captures_function_with_default_parameters() {
    let source = r#"
def process(value, count=5):
    pass
"#;
    let functions = collect_functions(source);
    let desc = functions.iter().find(|f| f.name == "process").unwrap();
    assert_eq!(desc.parameters.len(), 2);
    assert_eq!(desc.parameters[0].default_value, None);
    assert_eq!(desc.parameters[1].default_value.as_deref(), Some("5"));
}

#[test]
fn captures_function_with_type_hints() {
    let source = r#"
def add(x: int, y: int) -> int:
    return x + y
"#;
    let functions = collect_functions(source);
    let desc = functions.iter().find(|f| f.name == "add").unwrap();
    assert_eq!(desc.parameters.len(), 2);
    assert_eq!(desc.parameters[0].type_hint.as_deref(), Some("int"));
    assert_eq!(desc.parameters[1].type_hint.as_deref(), Some("int"));
    assert_eq!(desc.return_type.as_deref(), Some("int"));
}

#[test]
fn captures_decorated_function() {
    let source = r#"
@property
def value(self):
    pass
"#;
    let functions = collect_functions(source);
    let desc = functions.iter().find(|f| f.name == "value").unwrap();
    assert_eq!(desc.decorators.len(), 1);
    assert_eq!(desc.decorators[0], "property");
}

#[test]
fn captures_multiple_decorators() {
    let source = r#"
@decorator1
@decorator2
@decorator3
def wrapped():
    pass
"#;
    let functions = collect_functions(source);
    let desc = functions.iter().find(|f| f.name == "wrapped").unwrap();
    assert_eq!(desc.decorators.len(), 3);
    assert_eq!(desc.decorators[0], "decorator1");
    assert_eq!(desc.decorators[1], "decorator2");
    assert_eq!(desc.decorators[2], "decorator3");
}

#[test]
fn captures_function_with_args_kwargs() {
    let source = r#"
def flexible(*args, **kwargs):
    pass
"#;
    let functions = collect_functions(source);
    let desc = functions.iter().find(|f| f.name == "flexible").unwrap();
    assert_eq!(desc.parameters.len(), 2);
    assert!(desc.parameters.iter().any(|p| p.name == "args"));
    assert!(desc.parameters.iter().any(|p| p.name == "kwargs"));
}

#[test]
fn collects_multiple_functions() {
    let source = r#"
def first():
    pass

def second(x):
    pass

def third(a, b, c):
    pass
"#;
    let functions = collect_functions(source);
    assert_eq!(functions.len(), 3);
    assert!(functions.iter().any(|f| f.name == "first"));
    assert!(functions.iter().any(|f| f.name == "second"));
    assert!(functions.iter().any(|f| f.name == "third"));
}
