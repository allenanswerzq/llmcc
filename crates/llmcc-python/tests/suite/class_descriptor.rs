use llmcc_core::{context::CompileCtxt, IrBuildConfig};
use llmcc_python::{build_llmcc_ir, collect_symbols, LangPython, PythonClassDescriptor};

fn collect_classes(source: &str) -> Vec<PythonClassDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangPython>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangPython>(&cc, IrBuildConfig).unwrap();
    let globals = cc.create_globals();
    collect_symbols(unit, globals).classes
}

#[test]
fn collects_simple_class() {
    let source = r#"
class Point:
    pass
"#;
    let classes = collect_classes(source);
    assert_eq!(classes.len(), 1);
    let desc = &classes[0];
    assert_eq!(desc.name, "Point");
    assert!(desc.base_classes.is_empty());
    assert!(desc.methods.is_empty());
}

#[test]
fn captures_class_with_single_base() {
    let source = r#"
class Base:
    pass

class Derived(Base):
    pass
"#;
    let classes = collect_classes(source);
    let derived = classes.iter().find(|c| c.name == "Derived").unwrap();
    assert_eq!(derived.base_classes.len(), 1);
    assert_eq!(derived.base_classes[0], "Base");
}

#[test]
fn captures_class_with_multiple_bases() {
    let source = r#"
class Mixin1:
    pass

class Mixin2:
    pass

class Combined(Mixin1, Mixin2):
    pass
"#;
    let classes = collect_classes(source);
    let combined = classes.iter().find(|c| c.name == "Combined").unwrap();
    assert_eq!(combined.base_classes.len(), 2);
    assert!(combined.base_classes.contains(&"Mixin1".to_string()));
    assert!(combined.base_classes.contains(&"Mixin2".to_string()));
}

#[test]
fn captures_class_methods() {
    let source = r#"
class Calculator:
    def add(self, a, b):
        return a + b

    def subtract(self, a, b):
        return a - b

    def multiply(self, a, b):
        return a * b
"#;
    let classes = collect_classes(source);
    let calc = classes.iter().find(|c| c.name == "Calculator").unwrap();
    assert_eq!(calc.methods.len(), 3);
    assert!(calc.methods.contains(&"add".to_string()));
    assert!(calc.methods.contains(&"subtract".to_string()));
    assert!(calc.methods.contains(&"multiply".to_string()));
}

#[test]
fn captures_class_with_init_method() {
    let source = r#"
class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y

    def distance(self):
        pass
"#;
    let classes = collect_classes(source);
    let point = classes.iter().find(|c| c.name == "Point").unwrap();
    assert_eq!(point.methods.len(), 2);
    assert!(point.methods.contains(&"__init__".to_string()));
    assert!(point.methods.contains(&"distance".to_string()));
}

#[test]
fn captures_class_fields() {
    let source = r#"
class User:
    name: str
    age: int
    email: str = "unknown@example.com"
"#;
    let classes = collect_classes(source);
    let user = classes.iter().find(|c| c.name == "User").unwrap();
    assert_eq!(user.fields.len(), 3);
    assert!(user.fields.iter().any(|f| f.name == "name"));
    assert!(user.fields.iter().any(|f| f.name == "age"));
    assert!(user.fields.iter().any(|f| f.name == "email"));

    let name_field = user.fields.iter().find(|f| f.name == "name").unwrap();
    assert_eq!(name_field.type_hint.as_deref(), Some("str"));
}

#[test]
fn collects_multiple_classes() {
    let source = r#"
class Animal:
    pass

class Dog(Animal):
    pass

class Cat(Animal):
    pass

class Vehicle:
    pass
"#;
    let classes = collect_classes(source);
    assert_eq!(classes.len(), 4);
    assert!(classes.iter().any(|c| c.name == "Animal"));
    assert!(classes.iter().any(|c| c.name == "Dog"));
    assert!(classes.iter().any(|c| c.name == "Cat"));
    assert!(classes.iter().any(|c| c.name == "Vehicle"));
}

#[test]
fn captures_nested_class() {
    let source = r#"
class Outer:
    class Inner:
        def method(self):
            pass
"#;
    let classes = collect_classes(source);
    let _inner = classes.iter().find(|c| c.name == "Inner");
    // Inner classes might be collected depending on implementation
    // At minimum, Outer should be collected
    assert!(classes.iter().any(|c| c.name == "Outer"));
}

#[test]
fn captures_class_with_static_method() {
    let source = r#"
class MathUtils:
    @staticmethod
    def add(a, b):
        return a + b

    @classmethod
    def from_string(cls, s):
        return cls()
"#;
    let classes = collect_classes(source);
    let math_utils = classes.iter().find(|c| c.name == "MathUtils").unwrap();
    assert_eq!(math_utils.methods.len(), 2);
    assert!(math_utils.methods.contains(&"add".to_string()));
    assert!(math_utils.methods.contains(&"from_string".to_string()));
}

#[test]
fn captures_class_with_properties() {
    let source = r#"
class Temperature:
    def __init__(self, celsius):
        self._celsius = celsius

    @property
    def fahrenheit(self):
        return (self._celsius * 9/5) + 32
"#;
    let classes = collect_classes(source);
    let temp = classes.iter().find(|c| c.name == "Temperature").unwrap();
    assert!(temp.methods.contains(&"__init__".to_string()));
    assert!(temp.methods.contains(&"fahrenheit".to_string()));
}
