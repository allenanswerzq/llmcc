mod common;

use common::{assert_depends, assert_exists, with_compiled_unit};
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
        // Test return type dependencies for standalone function
        assert_depends(
            cc,
            "get_value",
            SymKind::Function,
            "Option",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );

        // Test return type in impl block (explicit type instead of Self)
        assert_depends(
            cc,
            "new",
            SymKind::Function,
            "User",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );

        assert_depends(
            cc,
            "display",
            SymKind::Function,
            "foo",
            SymKind::Function,
            Some(DepKind::Uses),
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
        assert_depends(
            cc,
            "Container",
            SymKind::Struct,
            "new",
            SymKind::Function,
            Some(DepKind::Uses),
        );

        assert_depends(
            cc,
            "Container",
            SymKind::Struct,
            "Outer",
            SymKind::Struct,
            Some(DepKind::Uses),
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
        // Test Display trait with method
        assert_depends(
            cc,
            "Display",
            SymKind::Trait,
            "display",
            SymKind::Function,
            Some(DepKind::Uses),
        );

        // Test Clone trait
        assert_depends(
            cc,
            "Clone",
            SymKind::Trait,
            "clone",
            SymKind::Function,
            Some(DepKind::Uses),
        );

        // Test FromIterator trait with bound
        assert_depends(
            cc,
            "FromIterator",
            SymKind::Trait,
            "Sized",
            SymKind::Trait,
            Some(DepKind::TypeBound),
        );

        assert_depends(
            cc,
            "FromIterator",
            SymKind::Trait,
            "Clone",
            SymKind::Trait,
            Some(DepKind::TypeBound),
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
        assert_depends(
            cc,
            "main",
            SymKind::Function,
            "hello",
            SymKind::Macro,
            Some(DepKind::Calls),
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
        // Test type alias with where clause
        assert_exists(cc, "PrintableData", SymKind::TypeAlias);
        assert_depends(
            cc,
            "PrintableData",
            SymKind::TypeAlias,
            "Data",
            SymKind::Struct,
            Some(DepKind::Alias),
        );

        assert_depends(
            cc,
            "PrintableData",
            SymKind::TypeAlias,
            "Printable",
            SymKind::Trait,
            Some(DepKind::Uses),
        );

        // Test type alias with multiple where clause bounds
        assert_exists(cc, "SerializableCollection", SymKind::TypeAlias);
        assert_depends(
            cc,
            "SerializableCollection",
            SymKind::TypeAlias,
            "Data",
            SymKind::Struct,
            Some(DepKind::Alias),
        );

        assert_depends(
            cc,
            "SerializableCollection",
            SymKind::TypeAlias,
            "Serializable",
            SymKind::Trait,
            Some(DepKind::Uses),
        );
        assert_depends(
            cc,
            "SerializableCollection",
            SymKind::TypeAlias,
            "Printable",
            SymKind::Trait,
            Some(DepKind::Uses),
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
        // Test explicit type annotations in setup
        assert_depends(
            cc,
            "setup",
            SymKind::Function,
            "Config",
            SymKind::Struct,
            None,
        );

        assert_depends(
            cc,
            "setup",
            SymKind::Function,
            "Message",
            SymKind::Struct,
            None,
        );

        // Test multiple let dependencies in another function
        assert_depends(
            cc,
            "handle_request",
            SymKind::Function,
            "Handler",
            SymKind::Struct,
            None,
        );

        assert_depends(
            cc,
            "handle_request",
            SymKind::Function,
            "Request",
            SymKind::Struct,
            None,
        );

        // Test nested let declarations
        assert_depends(
            cc,
            "complex_flow",
            SymKind::Function,
            "Config",
            SymKind::Struct,
            None,
        );

        assert_depends(
            cc,
            "complex_flow",
            SymKind::Function,
            "Message",
            SymKind::Struct,
            None,
        );

        // Test inferred types from let statements
        assert_depends(
            cc,
            "inferred_types",
            SymKind::Function,
            "Point",
            SymKind::Struct,
            None,
        );

        assert_depends(
            cc,
            "inferred_types",
            SymKind::Function,
            "Handler",
            SymKind::Struct,
            None,
        );

        // Test reference pattern function tracks Config type
        assert_depends(
            cc,
            "process_reference_pattern",
            SymKind::Function,
            "Config",
            SymKind::Struct,
            None,
        );

        // Test mutable pattern function tracks Handler and Request
        assert_depends(
            cc,
            "process_mutable_pattern",
            SymKind::Function,
            "Handler",
            SymKind::Struct,
            None,
        );

        assert_depends(
            cc,
            "process_mutable_pattern",
            SymKind::Function,
            "Request",
            SymKind::Struct,
            None,
        );

        // Test scoped type annotations
        assert_depends(
            cc,
            "process_scoped_types",
            SymKind::Function,
            "Point",
            SymKind::Struct,
            None,
        );

        assert_depends(
            cc,
            "process_scoped_types",
            SymKind::Function,
            "Message",
            SymKind::Struct,
            None,
        );

        // Test pattern with explicit type annotation
        assert_depends(
            cc,
            "pattern_with_type",
            SymKind::Function,
            "Config",
            SymKind::Struct,
            None,
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
        // Test function depends on Point from struct expression
        assert_depends(
            cc,
            "create_point",
            SymKind::Function,
            "Point",
            SymKind::Struct,
            None,
        );

        // Test function depends on Config from struct expression
        assert_depends(
            cc,
            "create_config",
            SymKind::Function,
            "Config",
            SymKind::Struct,
            None,
        );

        // Test function depends on Person from struct expression
        assert_depends(
            cc,
            "create_config",
            SymKind::Function,
            "Person",
            SymKind::Struct,
            None,
        );

        // Test multiple struct expressions in single function
        assert_depends(
            cc,
            "process",
            SymKind::Function,
            "Point",
            SymKind::Struct,
            None,
        );

        assert_depends(
            cc,
            "process",
            SymKind::Function,
            "Config",
            SymKind::Struct,
            None,
        );
    });
}

#[test]
#[serial_test::serial]
fn test_ty_resolve_scoped_identifiers() {
    let source = r#"
        pub mod utils {
            pub struct Helper;
        }

        pub use utils::Helper;

        pub fn use_helper() -> Helper {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        // Should resolve scoped identifier to the struct
        assert_depends(
            cc,
            "use_helper",
            SymKind::Function,
            "Helper",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_ty_resolve_callable() {
    let source = r#"
        pub fn helper() -> i32 {
            42
        }

        pub fn caller() -> i32 {
            helper()
        }

        pub fn indirect_call() -> i32 {
            caller()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        // Functions calling other functions should have Uses dependency
        assert_depends(
            cc,
            "caller",
            SymKind::Function,
            "helper",
            SymKind::Function,
            Some(DepKind::Calls),
        );

        assert_depends(
            cc,
            "indirect_call",
            SymKind::Function,
            "caller",
            SymKind::Function,
            Some(DepKind::Calls),
        );
    });
}
