mod common;

use common::{find_symbol_id, with_compiled_unit};
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
        assert!(find_symbol_id(cc, "utils", SymKind::Namespace).0 > 0);
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
        assert!(find_symbol_id(cc, "my_function", SymKind::Function).0 > 0);
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
        assert!(find_symbol_id(cc, "Person", SymKind::Struct).0 > 0);
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
        assert!(find_symbol_id(cc, "Color", SymKind::Enum).0 > 0);
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
        assert!(find_symbol_id(cc, "Drawable", SymKind::Trait).0 > 0);
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
        assert!(find_symbol_id(cc, "MAX_SIZE", SymKind::Const).0 > 0);
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
        assert!(find_symbol_id(cc, "GLOBAL_VAR", SymKind::Static).0 > 0);
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
        assert!(find_symbol_id(cc, "MyResult", SymKind::TypeAlias).0 > 0);
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
        assert!(find_symbol_id(cc, "x", SymKind::Field).0 > 0);
        assert!(find_symbol_id(cc, "y", SymKind::Field).0 > 0);
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
        assert!(find_symbol_id(cc, "Active", SymKind::EnumVariant).0 > 0);
        assert!(find_symbol_id(cc, "Inactive", SymKind::EnumVariant).0 > 0);
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
        assert!(find_symbol_id(cc, "a", SymKind::Variable).0 > 0);
        assert!(find_symbol_id(cc, "b", SymKind::Variable).0 > 0);
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
        assert!(find_symbol_id(cc, "value", SymKind::Variable).0 > 0);
        assert!(find_symbol_id(cc, "another", SymKind::Variable).0 > 0);
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
        assert!(find_symbol_id(cc, "generic_function", SymKind::Function).0 > 0);
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
        assert!(find_symbol_id(cc, "N", SymKind::Const).0 > 0);
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
        assert!(find_symbol_id(cc, "Item", SymKind::TypeAlias).0 > 0);
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
        assert!(find_symbol_id(cc, "host", SymKind::Field).0 > 0);
        assert!(find_symbol_id(cc, "port", SymKind::Field).0 > 0);
        assert!(find_symbol_id(cc, "timeout", SymKind::Field).0 > 0);
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
        assert!(find_symbol_id(cc, "MyType", SymKind::Struct).0 > 0);
        assert!(find_symbol_id(cc, "MyTrait", SymKind::Trait).0 > 0);
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
        assert!(find_symbol_id(cc, "my_macro", SymKind::Macro).0 > 0);
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
        assert!(find_symbol_id(cc, "add", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "subtract", SymKind::Function).0 > 0);
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
        assert!(find_symbol_id(cc, "MyType", SymKind::Struct).0 > 0);
        assert!(find_symbol_id(cc, "by_value", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "by_mut_ref", SymKind::Function).0 > 0);
        assert!(find_symbol_id(cc, "by_ref", SymKind::Function).0 > 0);
    });
}
