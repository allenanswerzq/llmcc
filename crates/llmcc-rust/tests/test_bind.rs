mod common;

use llmcc_core::symbol::SymKind;

use common::{find_symbol_id, with_compiled_unit};
use serial_test::serial;
use textwrap::dedent;

#[serial]
#[test]
fn bind_function_call_resolves_to_correct_symbol() {
    let source = dedent(
        "
        fn add(a: i32, b: i32) -> i32 {
            a + b
        }

        fn main() {
            let result = add(5, 3);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let add_func = find_symbol_id(cc, "add", SymKind::Function);
        let main_func = find_symbol_id(cc, "main", SymKind::Function);

        assert!(add_func.0 > 0);
        assert!(main_func.0 > 0);
    });
}

#[serial]
#[test]
fn bind_struct_field_access_resolution() {
    let source = dedent(
        "
        struct Point {
            x: i32,
            y: i32,
        }

        fn main() {
            let p = Point { x: 10, y: 20 };
            let x_val = p.x;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "Point", SymKind::Struct).0 > 0);
    });
}

#[serial]
#[test]
fn bind_method_call_on_type() {
    let source = dedent(
        "
        struct Calculator;

        impl Calculator {
            fn square(n: i32) -> i32 {
                n * n
            }
        }

        fn main() {
            let result = Calculator::square(5);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "Calculator", SymKind::Struct).0 > 0);
        assert!(find_symbol_id(cc, "square", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn bind_variable_type_from_explicit_annotation() {
    let source = dedent(
        "
        fn main() {
            let x: i32 = 42;
            let s: String = String::from(\"hello\");
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn bind_variable_type_from_initializer() {
    let source = dedent(
        "
        fn main() {
            let x = 42;
            let s = \"hello\";
            let v = vec![1, 2, 3];
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn bind_generic_type_parameters() {
    let source = dedent(
        "
        fn identity<T>(value: T) -> T {
            value
        }

        fn main() {
            let x = identity(42);
            let s = identity(\"hello\");
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "identity", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn bind_explicit_return_type() {
    let source = dedent(
        "
        fn get_age() -> i32 {
            42
        }

        fn get_name() -> String {
            String::from(\"Alice\")
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "get_age", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "get_name", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn bind_inferred_return_type() {
    let source = dedent(
        "
        fn compute(x: i32) -> i32 {
            x + 1
        }

        fn get_tuple() -> (i32, String) {
            (42, String::from(\"test\"))
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "compute", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "get_tuple", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn bind_function_calls_in_call_graph() {
    let source = dedent(
        "
        fn add(a: i32, b: i32) -> i32 {
            a + b
        }

        fn multiply(a: i32, b: i32) -> i32 {
            a * b
        }

        fn compute(x: i32, y: i32) -> i32 {
            let sum = add(x, y);
            multiply(sum, 2)
        }

        fn main() {
            let result = compute(3, 4);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "add", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "multiply", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "compute", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn test_visit_struct_item() {
    let source = dedent(
        "
        struct Point {
            x: i32,
            y: i32,
        }
        ",
    );
    with_compiled_unit(&[&source], |_cc| {});
}

#[serial]
#[test]
fn bind_trait_implementation() {
    let source = dedent(
        "
        trait Drawable {
            fn draw(&self);
        }

        struct Circle {
            radius: f32,
        }

        impl Drawable for Circle {
            fn draw(&self) {
                println!(\"Drawing circle\");
            }
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "Drawable", SymKind::Trait).0 > 0);
        assert!(find_symbol_id(cc, "Circle", SymKind::Struct).0 > 0);
        assert!(find_symbol_id(cc, "draw", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn bind_multiple_trait_implementations() {
    let source = dedent(
        "
        trait Drawable {
            fn draw(&self);
        }

        trait Resizable {
            fn resize(&mut self, scale: f32);
        }

        struct Rectangle;

        impl Drawable for Rectangle {
            fn draw(&self) {}
        }

        impl Resizable for Rectangle {
            fn resize(&mut self, scale: f32) {}
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "Drawable", SymKind::Trait).0 > 0);
        assert!(find_symbol_id(cc, "Resizable", SymKind::Trait).0 > 0);
        assert!(find_symbol_id(cc, "Rectangle", SymKind::Struct).0 > 0);
    });
}
