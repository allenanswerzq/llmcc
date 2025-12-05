mod common;

use common::{find_symbol_id, with_compiled_unit};
use llmcc_core::symbol::SymKind;
use serial_test::serial;
use textwrap::dedent;

#[serial]
#[test]
fn integration_multi_file_function_resolution() {
    let file1 = dedent(
        "
        pub fn add(a: i32, b: i32) -> i32 {
            a + b
        }
        ",
    );

    let file2 = dedent(
        "
        fn main() {
            let result = add(5, 3);
        }
        ",
    );

    with_compiled_unit(&[&file1, &file2], |cc| {
        assert!(find_symbol_id(cc, "add", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_multi_file_struct_usage() {
    let file1 = dedent(
        "
        pub struct Config {
            pub name: String,
            pub port: u16,
        }
        ",
    );

    let file2 = dedent(
        "
        fn main() {
            let config = Config {
                name: String::from(\"server\"),
                port: 8080,
            };
        }
        ",
    );

    with_compiled_unit(&[&file1, &file2], |cc| {
        assert!(find_symbol_id(cc, "Config", SymKind::Struct).0 > 0);
    });
}

#[serial]
#[test]
fn integration_multi_file_trait_implementation() {
    let file1 = dedent(
        "
        pub trait Handler {
            fn handle(&self);
        }
        ",
    );

    let file2 = dedent(
        "
        pub struct DefaultHandler;

        impl Handler for DefaultHandler {
            fn handle(&self) {
                println!(\"Handling...\");
            }
        }
        ",
    );

    with_compiled_unit(&[&file1, &file2], |cc| {
        assert!(find_symbol_id(cc, "Handler", SymKind::Trait).0 > 0);
        assert!(find_symbol_id(cc, "DefaultHandler", SymKind::Struct).0 > 0);
    });
}

#[serial]
#[test]
fn integration_multi_file_module_hierarchy() {
    let file1 = dedent(
        "
        pub mod api {
            pub fn get_user() -> String {
                String::from(\"user\")
            }
        }
        ",
    );

    let file2 = dedent(
        "
        fn main() {
            let user = api::get_user();
        }
        ",
    );

    with_compiled_unit(&[&file1, &file2], |cc| {
        assert!(find_symbol_id(cc, "api", SymKind::Namespace).0 > 0);
        assert!(find_symbol_id(cc, "get_user", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_cross_file_generic_usage() {
    let file1 = dedent(
        "
        pub struct Container<T> {
            pub value: T,
        }

        impl<T> Container<T> {
            pub fn new(value: T) -> Self {
                Container { value }
            }

            pub fn get(&self) -> &T {
                &self.value
            }
        }
        ",
    );

    let file2 = dedent(
        "
        fn main() {
            let int_container = Container::new(42);
            let val = int_container.get();
        }
        ",
    );

    with_compiled_unit(&[&file1, &file2], |cc| {
        assert!(find_symbol_id(cc, "Container", SymKind::Struct).0 > 0);
    });
}

#[serial]
#[test]
fn integration_deep_function_call_chain() {
    let source = dedent(
        "
        fn level_1() -> i32 {
            1
        }

        fn level_2() -> i32 {
            level_1() + 2
        }

        fn level_3() -> i32 {
            level_2() + 3
        }

        fn level_4() -> i32 {
            level_3() + 4
        }

        fn main() {
            let result = level_4();
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "level_1", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "level_2", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "level_3", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "level_4", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_mutually_recursive_functions() {
    let source = dedent(
        "
        fn is_even(n: i32) -> bool {
            if n == 0 {
                true
            } else {
                is_odd(n - 1)
            }
        }

        fn is_odd(n: i32) -> bool {
            if n == 0 {
                false
            } else {
                is_even(n - 1)
            }
        }

        fn main() {
            let result = is_even(4);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "is_even", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "is_odd", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_nested_generic_types() {
    let source = dedent(
        "
        fn process() -> Option<Result<Vec<String>, &'static str>> {
            Some(Ok(vec![String::from(\"test\")]))
        }

        fn main() {
            let result = process();
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "process", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_lifetime_annotations() {
    let source = dedent(
        "
        fn borrow<'a>(data: &'a String) -> &'a str {
            data.as_str()
        }

        fn main() {
            let s = String::from(\"test\");
            let borrowed = borrow(&s);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "borrow", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_trait_bounds() {
    let source = dedent(
        "
        use std::fmt::Display;

        fn print_it<T: Display>(val: T) {
            println!(\"{}\", val);
        }

        fn main() {
            print_it(42);
            print_it(\"hello\");
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "print_it", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_stdlib_vec_methods() {
    let source = dedent(
        "
        fn process_numbers(numbers: Vec<i32>) -> i32 {
            numbers.iter().sum()
        }

        fn main() {
            let nums = vec![1, 2, 3, 4, 5];
            let total = process_numbers(nums);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "process_numbers", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_stdlib_option_result_handling() {
    let source = dedent(
        "
        fn parse_number(s: &str) -> Option<i32> {
            s.parse().ok()
        }

        fn safe_divide(a: i32, b: i32) -> Result<i32, &'static str> {
            if b == 0 {
                Err(\"Division by zero\")
            } else {
                Ok(a / b)
            }
        }

        fn main() {
            let opt = parse_number(\"42\");
            let res = safe_divide(10, 2);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "parse_number", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "safe_divide", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_pattern_matching_in_functions() {
    let source = dedent(
        "
        enum Color {
            Red,
            Green,
            Blue,
        }

        fn describe_color(color: Color) -> &'static str {
            match color {
                Color::Red => \"It's red\",
                Color::Green => \"It's green\",
                Color::Blue => \"It's blue\",
            }
        }

        fn main() {
            let color = Color::Red;
            let description = describe_color(color);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "Color", SymKind::Enum).0 > 0);
        assert!(find_symbol_id(cc, "describe_color", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_closures_with_captures() {
    let source = dedent(
        "
        fn apply_operation(x: i32, y: i32) -> i32 {
            let add = |a: i32| a + x;
            let multiply = |a: i32| a * y;
            let result = add(10);
            multiply(result)
        }

        fn main() {
            let output = apply_operation(5, 3);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "apply_operation", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "main", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_higher_order_functions() {
    let source = dedent(
        "
        fn apply<T: Copy>(x: T, f: fn(T) -> T) -> T {
            f(x)
        }

        fn double(x: i32) -> i32 {
            x * 2
        }

        fn triple(x: i32) -> i32 {
            x * 3
        }

        fn main() {
            let result1 = apply(5, double);
            let result2 = apply(5, triple);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "apply", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "double", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "triple", SymKind::Function).0 > 0);
    });
}

#[serial]
#[test]
fn integration_associated_types_in_traits() {
    let source = dedent(
        "
        trait Iterator {
            type Item;
            fn next(&mut self) -> Option<Self::Item>;
        }

        struct Counter {
            count: i32,
        }

        impl Iterator for Counter {
            type Item = i32;
            fn next(&mut self) -> Option<i32> {
                self.count += 1;
                Some(self.count)
            }
        }

        fn main() {
            let mut counter = Counter { count: 0 };
            let val = counter.next();
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "Iterator", SymKind::Trait).0 > 0);
        assert!(find_symbol_id(cc, "Counter", SymKind::Struct).0 > 0);
    });
}

#[serial]
#[test]
fn integration_trait_object_usage() {
    let source = dedent(
        "
        trait Drawable {
            fn draw(&self);
        }

        struct Circle;
        struct Square;

        impl Drawable for Circle {
            fn draw(&self) {}
        }

        impl Drawable for Square {
            fn draw(&self) {}
        }

        fn render(shape: &dyn Drawable) {
            shape.draw();
        }

        fn main() {
            let circle = Circle;
            let square = Square;
            render(&circle);
            render(&square);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "Drawable", SymKind::Trait).0 > 0);
        assert!(find_symbol_id(cc, "Circle", SymKind::Struct).0 > 0);
        assert!(find_symbol_id(cc, "Square", SymKind::Struct).0 > 0);
        assert!(find_symbol_id(cc, "render", SymKind::Function).0 > 0);
    });
}
