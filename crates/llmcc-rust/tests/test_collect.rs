mod common;

use common::{assert_collect_symbol, with_compiled_unit};
use llmcc_core::symbol::SymKind;
use serial_test::serial;
use textwrap::dedent;

#[serial]
#[test]
fn visit_mod_item_declares_namespace() {
    let source = dedent(
        "
        mod utils {
            pub fn helper() {}
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "utils", SymKind::Namespace, true);
    });
}

#[serial]
#[test]
fn visit_function_item_declares_function() {
    let source = dedent(
        "
        fn my_function() {
            let x = 42;
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "my_function", SymKind::Function, true);
    });
}

#[serial]
#[test]
fn visit_struct_item_declares_struct() {
    let source = dedent(
        "
        struct Person {
            name: String,
            age: u32,
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "Person", SymKind::Struct, true);
    });
}

#[serial]
#[test]
fn visit_enum_item_declares_enum() {
    let source = dedent(
        "
        enum Color {
            Red,
            Green,
            Blue,
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "Color", SymKind::Enum, true);
    });
}

#[serial]
#[test]
fn visit_trait_item_declares_trait() {
    let source = dedent(
        "
        trait Drawable {
            fn draw(&self);
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "Drawable", SymKind::Trait, true);
    });
}

#[serial]
#[test]
fn visit_const_item_declares_const() {
    let source = dedent(
        "
        const MAX_SIZE: usize = 100;
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "MAX_SIZE", SymKind::Const, false);
    });
}

#[serial]
#[test]
fn visit_static_item_declares_static() {
    let source = dedent(
        "
        static GLOBAL_VAR: i32 = 42;
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "GLOBAL_VAR", SymKind::Static, false);
    });
}

#[serial]
#[test]
fn visit_type_item_declares_type_alias() {
    let source = dedent(
        "
        type MyResult<T> = Result<T, String>;
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "MyResult", SymKind::TypeAlias, false);
    });
}

#[serial]
#[test]
fn visit_field_declaration_declares_field() {
    let source = dedent(
        "
        struct Point {
            x: i32,
            y: i32,
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "x", SymKind::Field, false);
        assert_collect_symbol(cc, "y", SymKind::Field, false);
    });
}

#[serial]
#[test]
fn visit_enum_variant_declares_variant() {
    let source = dedent(
        "
        enum Status {
            Active,
            Inactive,
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "Active", SymKind::EnumVariant, false);
        assert_collect_symbol(cc, "Inactive", SymKind::EnumVariant, false);
    });
}

#[serial]
#[test]
fn visit_parameter_declares_parameter() {
    let source = dedent(
        "
        fn add(a: i32, b: i32) -> i32 {
            a + b
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "a", SymKind::Variable, false);
        assert_collect_symbol(cc, "b", SymKind::Variable, false);
    });
}

#[serial]
#[test]
fn visit_let_declaration_declares_variable() {
    let source = dedent(
        "
        fn create_value() {
            let value = 42;
            let another = \"hello\";
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "value", SymKind::Variable, false);
        assert_collect_symbol(cc, "another", SymKind::Variable, false);
    });
}

#[serial]
#[test]
fn visit_type_parameter_declares_type_param() {
    let source = dedent(
        "
        fn generic_function<T>(value: T) -> T {
            value
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "generic_function", SymKind::Function, true);
        assert_collect_symbol(cc, "T", SymKind::TypeParameter, false);
    });
}

#[serial]
#[test]
fn visit_const_parameter_declares_const_param() {
    let source = dedent(
        "
        fn with_const<const N: usize>() -> usize {
            N
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "N", SymKind::Const, false);
    });
}

#[serial]
#[test]
fn visit_associated_type_in_trait() {
    let source = dedent(
        "
        trait MyIterator {
            type Item;
            fn next(&mut self) -> Option<Self::Item>;
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "Item", SymKind::TypeAlias, false);
    });
}

#[serial]
#[test]
fn visit_multiple_struct_fields() {
    let source = dedent(
        "
        struct Config {
            host: String,
            port: u16,
            timeout: u64,
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        for name in ["host", "port", "timeout"] {
            assert_collect_symbol(cc, name, SymKind::Field, false);
        }
    });
}

#[serial]
#[test]
fn visit_impl_trait_for_type() {
    let source = dedent(
        "
        struct MyType;
        trait MyTrait {
            fn do_something(&self);
        }
        impl MyTrait for MyType {
            fn do_something(&self) {}
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "MyType", SymKind::Struct, true);
        assert_collect_symbol(cc, "MyTrait", SymKind::Trait, true);
        assert_collect_symbol(cc, "do_something", SymKind::Function, true);
    });
}

#[serial]
#[test]
fn visit_closure_expression_declares_parameters() {
    let source = dedent(
        "
        struct Wrapper(i32, i32);

        fn main() {
            let closure = |Wrapper(left, right)| {
                left + right
            };

            let _ = closure(Wrapper(1, 2));
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "left", SymKind::Variable, false);
        assert_collect_symbol(cc, "right", SymKind::Variable, false);
    });
}

#[serial]
#[test]
fn visit_let_declaration_marks_closure_symbols() {
    let source = dedent(
        "
        fn main() {
            let closure = || {};
            let value = 5;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "closure", SymKind::Closure, false);
        assert_collect_symbol(cc, "value", SymKind::Variable, false);
    });
}

#[serial]
#[test]
fn visit_let_declaration_collects_complex_patterns() {
    let source = dedent(
        "
        struct Foo {
            tuple: (i32, i32),
            array: [i32; 3],
            value: i32,
        }

        enum Pair {
            Item(i32),
            Single(i32),
        }

        fn main() {
            let [start, .., end] = [1, 2, 3, 4];
            let Foo { tuple: (left, right), array: [head, ..], .. } = Foo {
                tuple: (1, 2),
                array: [3, 4, 5],
                value: 9,
            };
            let (ref inner_ref, mut inner_mut) = (&10, 20);
            let &value = &30;
            let ref mut opt = Some(5);
            let Pair::Item(num) | Pair::Single(num) = Pair::Item(40);
            let (first, (second, third)) = (1, (2, 3));
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        for name in [
            "start",
            "end",
            "left",
            "right",
            "head",
            "inner_ref",
            "inner_mut",
            "value",
            "opt",
            "num",
            "first",
            "second",
            "third",
        ] {
            assert_collect_symbol(cc, name, SymKind::Variable, false);
        }
    });
}

#[serial]
#[test]
fn visit_macro_rules_declares_macro() {
    let source = dedent(
        "
        macro_rules! my_macro {
            () => {
                42
            };
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "my_macro", SymKind::Macro, true);
    });
}

#[serial]
#[test]
fn visit_function_signature_in_trait() {
    let source = dedent(
        "
        trait Calculator {
            fn add(a: i32, b: i32) -> i32;
            fn subtract(x: i32, y: i32) -> i32;
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "add", SymKind::Function, true);
        assert_collect_symbol(cc, "subtract", SymKind::Function, true);
    });
}

#[serial]
#[test]
fn visit_self_in_different_parameter_forms() {
    let source = dedent(
        "
        struct MyType;

        impl MyType {
            fn by_value(self) {}
            fn by_mut_ref(&mut self) {}
            fn by_ref(&self) {}
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {
        assert_collect_symbol(cc, "MyType", SymKind::Struct, true);
        assert_collect_symbol(cc, "by_value", SymKind::Function, true);
        assert_collect_symbol(cc, "by_mut_ref", SymKind::Function, true);
        assert_collect_symbol(cc, "by_ref", SymKind::Function, true);
    });
}
