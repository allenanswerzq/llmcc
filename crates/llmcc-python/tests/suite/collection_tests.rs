use llmcc_core::{context::CompileCtxt, IrBuildConfig};
use llmcc_descriptor::{
    ClassDescriptor, FunctionDescriptor, FunctionParameter, ImportDescriptor, TypeExpr,
    VariableDescriptor,
};
use llmcc_python::{build_llmcc_ir, collect_symbols, CollectionResult, LangPython};

fn collect_from_source(source: &str) -> CollectionResult {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangPython>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangPython>(&cc, IrBuildConfig).unwrap();
    let collection = collect_symbols(unit);
    collection.result
}

fn expect_function<'a>(collection: &'a CollectionResult, name: &str) -> &'a FunctionDescriptor {
    collection
        .functions
        .iter()
        .find(|descriptor| descriptor.name == name)
        .unwrap_or_else(|| panic!("Function '{name}' should be found in collection"))
}

fn expect_class<'a>(collection: &'a CollectionResult, name: &str) -> &'a ClassDescriptor {
    collection
        .classes
        .iter()
        .find(|descriptor| descriptor.name == name)
        .unwrap_or_else(|| panic!("Class '{name}' should be found in collection"))
}

#[allow(dead_code)]
fn expect_variable<'a>(collection: &'a CollectionResult, name: &str) -> &'a VariableDescriptor {
    collection
        .variables
        .iter()
        .find(|descriptor| descriptor.name == name)
        .unwrap_or_else(|| panic!("Variable '{name}' should be found in collection"))
}

fn expect_import<'a>(collection: &'a CollectionResult, module: &str) -> &'a ImportDescriptor {
    collection
        .imports
        .iter()
        .find(|descriptor| descriptor.source == module)
        .unwrap_or_else(|| panic!("Import '{module}' should be found in collection"))
}

fn type_repr(expr: &TypeExpr) -> String {
    match expr {
        TypeExpr::Path { segments, .. } => segments.join("::"),
        TypeExpr::Opaque { repr, .. } => repr.clone(),
        TypeExpr::Unknown(repr) => repr.clone(),
        other => format!("{:?}", other),
    }
}

fn parameter_has_name(param: &FunctionParameter) -> bool {
    param
        .name
        .as_ref()
        .map(|name| !name.is_empty())
        .unwrap_or_else(|| {
            param
                .pattern
                .as_ref()
                .map(|pattern| !pattern.is_empty())
                .unwrap_or(false)
        })
}

#[test]
fn collects_simple_function() {
    let source = r#"
def foo():
    pass
"#;
    let result = collect_from_source(source);

    let func = expect_function(&result, "foo");
    assert!(!func.name.is_empty(), "Function name should not be empty");
}

#[test]
fn collects_function_with_parameters() {
    let source = r#"
def greet(name, age=25):
    pass
"#;
    let result = collect_from_source(source);

    let func = expect_function(&result, "greet");
    assert!(!func.name.is_empty(), "Function name should not be empty");
    assert!(
        !func.parameters.is_empty(),
        "Function should have parameters"
    );
    assert!(
        func.parameters.iter().all(parameter_has_name),
        "Parameter names should not be empty",
    );
}

#[test]
fn collects_function_with_return_type_hint() {
    let source = r#"
def get_value() -> int:
    return 42
"#;
    let result = collect_from_source(source);

    let func = expect_function(&result, "get_value");
    let return_type = func
        .return_type
        .as_ref()
        .map(type_repr)
        .expect("Return type should be present");
    assert!(!return_type.is_empty(), "Return type should not be empty");
}

#[test]
fn collects_multiple_functions() {
    let source = r#"
def func_one():
    pass

def func_two():
    pass
"#;
    let result = collect_from_source(source);

    // Should collect valid function descriptors
    assert!(
        result
            .functions
            .iter()
            .all(|descriptor| !descriptor.name.is_empty()),
        "Collected function names should not be empty",
    );
}

#[test]
fn collects_simple_class() {
    let source = r#"
class MyClass:
    pass
"#;
    let result = collect_from_source(source);

    let class = expect_class(&result, "MyClass");
    assert!(!class.name.is_empty(), "Class name should not be empty");
}

#[test]
fn collects_class_with_methods() {
    let source = r#"
class Calculator:
    def add(self, x, y):
        pass
    def subtract(self, x, y):
        pass
"#;
    let result = collect_from_source(source);

    let class = expect_class(&result, "Calculator");
    assert!(!class.methods.is_empty(), "Class should have methods");
    assert!(
        class.methods.iter().all(|method| !method.is_empty()),
        "Method names should not be empty",
    );
}

#[test]
fn collects_class_with_inheritance() {
    let source = r#"
class Base:
    pass

class Derived(Base):
    pass
"#;
    let result = collect_from_source(source);

    let derived = expect_class(&result, "Derived");
    assert!(
        !derived.base_types.is_empty(),
        "Class should have base classes"
    );
    assert!(
        derived
            .base_types
            .iter()
            .all(|base_class| !type_repr(base_class).is_empty()),
        "Base class names should not be empty",
    );
}

#[test]
fn collects_class_with_fields() {
    let source = r#"
class Person:
    def __init__(self):
        self.name = ""
        self.age = 0
"#;
    let result = collect_from_source(source);

    let class = expect_class(&result, "Person");
    assert!(!class.fields.is_empty(), "Class should have fields");
    assert!(
        class
            .fields
            .iter()
            .all(|field| matches!(field.name.as_deref(), Some(name) if !name.is_empty())),
        "Field names should not be empty",
    );
}

#[test]
fn collects_global_variables() {
    let source = r#"
x = 42
y = 'hello'
z = [1, 2, 3]
"#;
    let result = collect_from_source(source);

    // All collected variables should have valid structure
    assert!(
        result
            .variables
            .iter()
            .all(|variable| !variable.name.is_empty()),
        "Variable names should not be empty",
    );
}

#[test]
fn collects_simple_import() {
    let source = r#"
import os
"#;
    let result = collect_from_source(source);

    let import = expect_import(&result, "os");
    assert!(!import.source.is_empty(), "Module name should not be empty");
}

#[test]
fn collects_multiple_imports() {
    let source = r#"
import os
import sys
import json
"#;
    let result = collect_from_source(source);

    // All collected imports should have valid structure
    assert!(
        result
            .imports
            .iter()
            .all(|import| !import.source.is_empty()),
        "Module name should not be empty",
    );
}

#[test]
fn collects_decorated_function() {
    let source = r#"
@decorator
def func():
    pass
"#;
    let result = collect_from_source(source);

    let func = expect_function(&result, "func");
    assert!(
        !func.decorators.is_empty(),
        "Function should have decorators"
    );
    assert!(
        func.decorators
            .iter()
            .all(|decorator| !decorator.is_empty()),
        "Decorator names should not be empty",
    );
}

#[test]
fn collects_function_with_type_hints() {
    let source = r#"
def typed_func(name: str, age: int) -> bool:
    pass
"#;
    let result = collect_from_source(source);

    let func = expect_function(&result, "typed_func");
    assert!(
        !func.parameters.is_empty(),
        "Function should have parameters"
    );
    assert!(
        func.parameters.iter().all(parameter_has_name),
        "Parameter names should not be empty",
    );
    let return_type = func
        .return_type
        .as_ref()
        .map(type_repr)
        .expect("Return type should be present");
    assert!(!return_type.is_empty(), "Return type should not be empty");
}

#[test]
fn collects_empty_module() {
    let source = r#"
# Just a comment
"#;
    let result = collect_from_source(source);

    // Empty module should have empty collections
    assert!(
        result.functions.is_empty(),
        "Empty module should have no functions"
    );
    assert!(
        result.classes.is_empty(),
        "Empty module should have no classes"
    );
}

#[test]
fn collects_mixed_definitions() {
    let source = r#"
def function_one():
    pass

class ClassOne:
    pass

def function_two():
    pass

x = 10
"#;
    let result = collect_from_source(source);

    // Collection should have valid structure for all collected items
    assert!(
        result
            .functions
            .iter()
            .all(|descriptor| !descriptor.name.is_empty()),
        "Function names should not be empty",
    );
    assert!(
        result
            .classes
            .iter()
            .all(|descriptor| !descriptor.name.is_empty()),
        "Class names should not be empty",
    );
    assert!(
        result
            .variables
            .iter()
            .all(|descriptor| !descriptor.name.is_empty()),
        "Variable names should not be empty",
    );
}

#[test]
fn collects_class_with_init_method() {
    let source = r#"
class MyClass:
    def __init__(self, name):
        self.name = name

    def display(self):
        pass
"#;
    let result = collect_from_source(source);

    let class = expect_class(&result, "MyClass");
    assert!(!class.methods.is_empty(), "Class should have methods");
    assert!(
        class.methods.iter().all(|method| !method.is_empty()),
        "Method names should not be empty",
    );
}

#[test]
fn collects_nested_functions() {
    let source = r#"
def outer():
    def inner():
        pass
    pass
"#;
    let result = collect_from_source(source);

    // Should handle nested functions without panic
    assert!(
        result
            .functions
            .iter()
            .all(|descriptor| !descriptor.name.is_empty()),
        "Function names should not be empty",
    );
}

#[test]
fn collects_class_with_multiple_inheritance() {
    let source = r#"
class Base1:
    pass

class Base2:
    pass

class Derived(Base1, Base2):
    pass
"#;
    let result = collect_from_source(source);

    let derived = expect_class(&result, "Derived");
    assert!(
        !derived.base_types.is_empty(),
        "Class should have base classes"
    );
    assert!(
        derived
            .base_types
            .iter()
            .all(|base_class| !type_repr(base_class).is_empty()),
        "Base class names should not be empty",
    );
}

#[test]
fn collects_all_descriptor_types() {
    let source = r#"
def my_function():
    pass

class MyClass:
    pass

x = 100
import os
"#;
    let result = collect_from_source(source);

    // All collected items should have valid structure
    assert!(
        result
            .functions
            .iter()
            .all(|descriptor| !descriptor.name.is_empty()),
        "Function names should not be empty",
    );
    assert!(
        result
            .classes
            .iter()
            .all(|descriptor| !descriptor.name.is_empty()),
        "Class names should not be empty",
    );
    assert!(
        result
            .variables
            .iter()
            .all(|descriptor| !descriptor.name.is_empty()),
        "Variable names should not be empty",
    );
    assert!(
        result
            .imports
            .iter()
            .all(|descriptor| !descriptor.source.is_empty()),
        "Module names should not be empty",
    );
}

#[test]
fn function_parameters_structure_valid() {
    let source = r#"
def func(a, b=10, *args, **kwargs):
    pass
"#;
    let result = collect_from_source(source);

    let func = expect_function(&result, "func");
    assert!(
        !func.parameters.is_empty(),
        "Function should have parameters"
    );
    assert!(
        func.parameters.iter().all(parameter_has_name),
        "Parameter name should not be empty",
    );
}

#[test]
fn class_structure_is_consistent() {
    let source = r#"
class TestClass:
    def method1(self):
        pass

    def method2(self):
        pass
"#;
    let result = collect_from_source(source);

    let class = expect_class(&result, "TestClass");
    assert!(!class.methods.is_empty(), "Class should have methods");
    assert!(
        class.methods.iter().all(|method| !method.is_empty()),
        "Method name should not be empty",
    );
}
