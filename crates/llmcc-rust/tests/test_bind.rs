mod common;

use common::{assert_depends, assert_depends_batch, assert_exists, with_compiled_unit};
use llmcc_core::symbol::{DepKind, SymKind};

#[test]
#[serial_test::serial]
fn test_visit_source_file() {
    let source = r#"
        fn main() {}
    "#;

    with_compiled_unit(&[source], |cc| {
        let all_symbols = cc.get_all_symbols();
        assert!(!all_symbols.is_empty());
    });
}

#[test]
#[serial_test::serial]
fn test_visit_mod_item() {
    let source = r#"
        mod utils {}
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "source_0",
            SymKind::File,
            "utils",
            SymKind::Namespace,
            None,
        );
    });
}

#[test]
#[serial_test::serial]
fn test_visit_function_item() {
    let source = r#"
        struct Option<T> {}

        fn get_value() -> Option<i32> {
            Some(42)
        }

        struct User {
            name: String,
        }

        impl User {
            fn new(name: String) -> User {
                User { name }
            }

            fn foo() {
                println!("foo");
            }

            fn display(&self) {
                println!("User: {}", self.name);
                Self::foo();
            }
        }

        fn main() {
            let user = User::new(String::from("Alice"));
            user.display();
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends_batch(
            cc,
            vec![
                (
                    "get_value",
                    SymKind::Function,
                    "Option",
                    SymKind::Struct,
                    Some(DepKind::ReturnType),
                ),
                (
                    "new",
                    SymKind::Function,
                    "User",
                    SymKind::Struct,
                    Some(DepKind::ReturnType),
                ),
                (
                    "display",
                    SymKind::Function,
                    "foo",
                    SymKind::Function,
                    Some(DepKind::Uses),
                ),
            ],
        );
    });
}

#[test]
#[serial_test::serial]
fn test_visit_impl_item() {
    let source = r#"
        trait Printable {
            fn print(&self);
        }

        struct Container<T, U, V>(T);
        struct Inner;
        struct Foo;
        struct Outer<T>;

        impl Container<Inner, Foo, Outer<Foo>> {
            fn new(value: Inner) -> Container<Inner, Foo, Outer<Foo>> {
                Container(value)
            }
        }

        impl Printable for Container<Inner, Foo, Outer<Foo>> {
            fn print(&self) {
                println!("Printing Inner container.");
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends_batch(
            cc,
            vec![
                (
                    "Container",
                    SymKind::Struct,
                    "new",
                    SymKind::Function,
                    Some(DepKind::Uses),
                ),
                (
                    "Container",
                    SymKind::Struct,
                    "Outer",
                    SymKind::Struct,
                    Some(DepKind::Uses),
                ),
            ],
        );
    });
}

#[test]
#[serial_test::serial]
fn test_visit_struct_item() {
    let source = r#"
        struct User {
            name: String,
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "User", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_visit_enum_item() {
    let source = r#"
        enum Color {
            Red,
            Green,
            Blue,
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "Color", SymKind::Enum);
    });
}

#[test]
#[serial_test::serial]
fn test_visit_trait_item() {
    let source = r#"
        trait Display {
            fn display(&self);
        }

        trait Clone {
            fn clone(&self) -> Self;
        }

        trait Iterator {
            type Item;
            fn next(&mut self) -> Option<Self::Item>;
        }

        trait Sized {}

        trait FromIterator<T>: Sized + Clone {
            fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self;
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends_batch(
            cc,
            vec![
                (
                    "Display",
                    SymKind::Trait,
                    "display",
                    SymKind::Function,
                    Some(DepKind::Uses),
                ),
                (
                    "Clone",
                    SymKind::Trait,
                    "clone",
                    SymKind::Function,
                    Some(DepKind::Uses),
                ),
                (
                    "FromIterator",
                    SymKind::Trait,
                    "Sized",
                    SymKind::Trait,
                    Some(DepKind::TypeBound),
                ),
                (
                    "FromIterator",
                    SymKind::Trait,
                    "Clone",
                    SymKind::Trait,
                    Some(DepKind::TypeBound),
                ),
            ],
        );
    });
}

#[test]
#[serial_test::serial]
fn test_visit_macro_definition() {
    let source = r#"
        macro_rules! hello {
            () => {
                println!("Hello!");
            };
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "hello", SymKind::Macro);
    });
}

#[test]
#[serial_test::serial]
fn test_visit_macro_invocation() {
    let source = r#"
        macro_rules! hello {
            () => {
                println!("Hello!");
            };
        }

        fn main() {
            hello!();
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends_batch(
            cc,
            vec![(
                "main",
                SymKind::Function,
                "hello",
                SymKind::Macro,
                Some(DepKind::Calls),
            )],
        );
    });
}

#[test]
#[serial_test::serial]
fn test_visit_function_signature_item() {
    let source = r#"
        fn add(a: i32, b: i32) -> i32;
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "add", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_visit_type_item() {
    let source = r#"
        trait Printable {
            fn print(&self);
        }

        trait Serializable {
            fn serialize(&self) -> String;
        }

        struct Data<T> {
            value: T,
        }

        type PrintableData<T> = Data<T> where T: Printable;
        type SerializableCollection<T> = Data<T> where T: Serializable + Printable;
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "PrintableData", SymKind::TypeAlias);
        assert_exists(cc, "SerializableCollection", SymKind::TypeAlias);

        assert_depends_batch(
            cc,
            vec![
                (
                    "PrintableData",
                    SymKind::TypeAlias,
                    "Data",
                    SymKind::Struct,
                    Some(DepKind::Alias),
                ),
                (
                    "PrintableData",
                    SymKind::TypeAlias,
                    "Printable",
                    SymKind::Trait,
                    Some(DepKind::Uses),
                ),
                (
                    "SerializableCollection",
                    SymKind::TypeAlias,
                    "Data",
                    SymKind::Struct,
                    Some(DepKind::Alias),
                ),
                (
                    "SerializableCollection",
                    SymKind::TypeAlias,
                    "Serializable",
                    SymKind::Trait,
                    Some(DepKind::Uses),
                ),
                (
                    "SerializableCollection",
                    SymKind::TypeAlias,
                    "Printable",
                    SymKind::Trait,
                    Some(DepKind::Uses),
                ),
            ],
        );
    });
}

#[test]
#[serial_test::serial]
fn test_visit_let_declaration() {
    let source = r#"
        pub struct Config;
        pub struct Message;
        pub struct Handler;
        pub struct Request;
        pub struct Point;
        pub struct Person;
        pub enum Status {
            Active,
            Inactive,
        }

        // Test function with explicit let type annotation
        pub fn setup() {
            let config: Config = todo!();
            let msg: Message = todo!();
        }

        // Test function with multiple let declarations
        pub fn handle_request() {
            let handler: Handler = todo!();
            let req: Request = todo!();
            let count: i32 = 5;
        }

        pub fn complex_flow() {
            let cfg: Config = todo!();
            let _ = {
                let m: Message = todo!();
                m
            };
        }

        // Test inferred type from value
        pub fn inferred_types() {
            let point = Point;
            let handler = Handler;
        }

        // Test reference patterns
        pub fn process_reference_pattern() {
            let config = Config;
            let ref cfg = config;
        }

        // Test mutable patterns
        pub fn process_mutable_pattern() {
            let mut handler = Handler;
            let req = Request;
        }

        // Test let with explicit scoped type
        pub fn process_scoped_types() {
            let p: Point = todo!();
            let m: Message = todo!();
        }

        // Test pattern with type and value
        pub fn pattern_with_type() {
            let value: Config = todo!();
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends_batch(
            cc,
            vec![
                ("setup", SymKind::Function, "Config", SymKind::Struct, None),
                ("setup", SymKind::Function, "Message", SymKind::Struct, None),
                (
                    "handle_request",
                    SymKind::Function,
                    "Handler",
                    SymKind::Struct,
                    None,
                ),
                (
                    "handle_request",
                    SymKind::Function,
                    "Request",
                    SymKind::Struct,
                    None,
                ),
                (
                    "complex_flow",
                    SymKind::Function,
                    "Config",
                    SymKind::Struct,
                    None,
                ),
                (
                    "complex_flow",
                    SymKind::Function,
                    "Message",
                    SymKind::Struct,
                    None,
                ),
                (
                    "inferred_types",
                    SymKind::Function,
                    "Point",
                    SymKind::Struct,
                    None,
                ),
                (
                    "inferred_types",
                    SymKind::Function,
                    "Handler",
                    SymKind::Struct,
                    None,
                ),
                (
                    "process_reference_pattern",
                    SymKind::Function,
                    "Config",
                    SymKind::Struct,
                    None,
                ),
                (
                    "process_mutable_pattern",
                    SymKind::Function,
                    "Handler",
                    SymKind::Struct,
                    None,
                ),
                (
                    "process_mutable_pattern",
                    SymKind::Function,
                    "Request",
                    SymKind::Struct,
                    None,
                ),
                (
                    "process_scoped_types",
                    SymKind::Function,
                    "Point",
                    SymKind::Struct,
                    None,
                ),
                (
                    "process_scoped_types",
                    SymKind::Function,
                    "Message",
                    SymKind::Struct,
                    None,
                ),
                (
                    "pattern_with_type",
                    SymKind::Function,
                    "Config",
                    SymKind::Struct,
                    None,
                ),
            ],
        );
    });
}

#[test]
#[serial_test::serial]
fn test_visit_struct_expression() {
    let source = r#"
        pub struct Point {
            x: i32,
            y: i32,
        }

        pub struct Config {
            name: String,
            value: i32,
        }

        pub struct Person {
            name: String,
            age: u32,
            config: Config,
        }

        pub enum Status {
            Active,
            Inactive,
        }

        // Function that creates struct instances
        pub fn create_point() -> Point {
            Point { x: 0, y: 0 }
        }

        pub fn create_config() {
            let cfg = Config { name: "test".to_string(), value: 42 };
            let person = Person {
                name: "Alice".to_string(),
                age: 30,
                config: cfg,
            };
        }

        pub fn process() {
            let p1 = Point { x: 10, y: 20 };
            let p2 = Point { x: 30, y: 40 };
            let cfg = Config { name: "cfg".to_string(), value: 100 };
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends_batch(
            cc,
            vec![
                (
                    "create_point",
                    SymKind::Function,
                    "Point",
                    SymKind::Struct,
                    None,
                ),
                (
                    "create_config",
                    SymKind::Function,
                    "Config",
                    SymKind::Struct,
                    None,
                ),
                (
                    "create_config",
                    SymKind::Function,
                    "Person",
                    SymKind::Struct,
                    None,
                ),
                ("process", SymKind::Function, "Point", SymKind::Struct, None),
                (
                    "process",
                    SymKind::Function,
                    "Config",
                    SymKind::Struct,
                    None,
                ),
            ],
        );
    });
}
