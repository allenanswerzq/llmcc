mod common;

use common::{assert_exists, with_compiled_unit};
use llmcc_core::symbol::SymKind;

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
        assert_exists(cc, "utils", SymKind::Namespace);
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
        // Verify that key symbols are bound correctly with their types
        assert_exists(cc, "Option", SymKind::Struct);
        assert_exists(cc, "get_value", SymKind::Function);
        assert_exists(cc, "User", SymKind::Struct);
        assert_exists(cc, "main", SymKind::Function);
        assert_exists(cc, "new", SymKind::Function);
        assert_exists(cc, "display", SymKind::Function);
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
        assert_exists(cc, "Container", SymKind::Struct);
        assert_exists(cc, "new", SymKind::Function);
        assert_exists(cc, "Outer", SymKind::Struct);
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
        assert_exists(cc, "Display", SymKind::Trait);
        assert_exists(cc, "display", SymKind::Function);
        assert_exists(cc, "Clone", SymKind::Trait);
        assert_exists(cc, "clone", SymKind::Function);
        assert_exists(cc, "FromIterator", SymKind::Trait);
        assert_exists(cc, "Sized", SymKind::Trait);
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
        assert_exists(cc, "main", SymKind::Function);
        assert_exists(cc, "hello", SymKind::Macro);
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
        assert_exists(cc, "Data", SymKind::Struct);
        assert_exists(cc, "Printable", SymKind::Trait);
        assert_exists(cc, "Serializable", SymKind::Trait);
    });
}

#[test]
#[serial_test::serial]
fn test_visit_let_declaration() {
    let source = r#"
        pub struct Foo;
        impl Foo {
            pub fn new() -> Self {
                Foo
            }
        }

        pub struct Bar {
            x: i32,
            y: String,
        }
        pub struct Baz;
        pub struct Boo;

        fn something() -> Boo {
            Boo
        }

        fn func() {
            // Case 1: let without type or value
            let unbound;

            // Case 2: let with inferred type from value (primitive - no dep tracked)
            let x = 42;

            // Case 3: let with explicit type annotation (dep tracked)
            let y: i32 = 100;

            // Case 4: let mutable with inferred type
            let mut z = 5;

            // Case 5: let with custom struct (inferred from value)
            let foo = Foo;

            // Case 6: let with explicit custom struct type
            let bar: Bar = todo!();

            // Case 7: let with struct instantiation
            let baz_instance = Baz::new();

            // Case 8: let with function call (inferred type)
            let boo = something();

            // Case 9: let with struct pattern (destructuring)
            let Bar { x: field_x, y: field_y } = bar;

            // Case 10: let with mutable binding to custom type
            let mut mutable_baz = Baz;
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "func", SymKind::Function);
        assert_exists(cc, "i32", SymKind::Primitive);
        assert_exists(cc, "Foo", SymKind::Struct);
        assert_exists(cc, "Bar", SymKind::Struct);
        assert_exists(cc, "Baz", SymKind::Struct);
        assert_exists(cc, "Boo", SymKind::Struct);
    });
}#[test]
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
        assert_exists(cc, "create_point", SymKind::Function);
        assert_exists(cc, "Point", SymKind::Struct);
        assert_exists(cc, "create_config", SymKind::Function);
        assert_exists(cc, "Config", SymKind::Struct);
        assert_exists(cc, "Person", SymKind::Struct);
        assert_exists(cc, "process", SymKind::Function);
    });
}
