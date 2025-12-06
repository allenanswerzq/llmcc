mod common;

use llmcc_core::symbol::SymKind;

use common::{BindExpect, assert_bind_symbol, with_compiled_unit};
use serial_test::serial;
use textwrap::dedent;

#[serial]
#[test]
fn test_visit_source_file() {
    // Tests: visit_source_file - handles crate/module/file scope setup
    let source = dedent(
        "
        fn top_level() {}
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "top_level",
            BindExpect::new(SymKind::Function).expect_scope(),
        );
    });
}

#[serial]
#[test]
fn test_visit_identifier() {
    // Tests: visit_identifier - resolves identifiers to symbols
    let source = dedent(
        "
        fn use_var() {
            let x = 10;
            let y = x;
            drop(y);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "x", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "y", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn test_visit_type_identifier() {
    // Tests: visit_type_identifier - resolves type names to struct/enum/trait symbols
    let source = dedent(
        "
        struct MyType;
        fn consume(val: MyType) {}
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "MyType",
            BindExpect::new(SymKind::Struct).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "val",
            BindExpect::new(SymKind::Variable).with_type_of("MyType"),
        );
    });
}

#[serial]
#[test]
fn test_visit_primitive_type() {
    // Tests: visit_primitive_type - resolves primitive types (i32, bool, str, etc.)
    let source = dedent(
        "
        fn primitives(a: i32, b: bool, c: f64, d: char) {}
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "a",
            BindExpect::new(SymKind::Variable).with_type_of("i32"),
        );
        assert_bind_symbol(
            cc,
            "b",
            BindExpect::new(SymKind::Variable).with_type_of("bool"),
        );
        assert_bind_symbol(
            cc,
            "c",
            BindExpect::new(SymKind::Variable).with_type_of("f64"),
        );
        assert_bind_symbol(
            cc,
            "d",
            BindExpect::new(SymKind::Variable).with_type_of("char"),
        );
    });
}

#[serial]
#[test]
fn test_visit_block() {
    // Tests: visit_block - establishes block scope for { ... }
    let source = dedent(
        "
        fn with_blocks() {
            {
                let inner = 1;
                drop(inner);
            }
            let outer = 2;
            drop(outer);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "inner", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "outer", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn test_visit_mod_item() {
    // Tests: visit_mod_item - creates module scope and handles pub visibility
    let source = dedent(
        "
        mod private_mod {
            fn hidden() {}
        }

        pub mod public_mod {
            pub fn visible() {}
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "private_mod",
            BindExpect::new(SymKind::Namespace).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "public_mod",
            BindExpect::new(SymKind::Namespace)
                .expect_scope()
                .with_is_global(true),
        );
        assert_bind_symbol(
            cc,
            "visible",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_is_global(true),
        );
    });
}

#[serial]
#[test]
fn test_visit_function_signature_item() {
    // Tests: visit_function_signature_item - trait function signatures
    let source = dedent(
        "
        trait Compute {
            fn calculate(&self) -> i32;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Compute",
            BindExpect::new(SymKind::Trait).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "calculate",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_type_of("i32"),
        );
    });
}

#[serial]
#[test]
fn test_visit_function_item() {
    // Tests: visit_function_item - function binding with return type and main detection
    let source = dedent(
        "
        fn main() {}

        pub fn exported() -> bool { true }

        fn internal(x: i32) -> i32 { x }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "main",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_is_global(true),
        );
        assert_bind_symbol(
            cc,
            "exported",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_type_of("bool")
                .with_is_global(true),
        );
        assert_bind_symbol(
            cc,
            "internal",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_type_of("i32"),
        );
    });
}

#[serial]
#[test]
fn test_visit_field_identifier() {
    // Tests: visit_field_identifier - struct field name identifiers
    let source = dedent(
        "
        struct Data {
            name: i32,
        }
        fn use_field(d: Data) {
            let _ = d.name;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "name",
            BindExpect::new(SymKind::Field)
                .with_type_of("i32")
                .with_field_of("Data"),
        );
    });
}

#[serial]
#[test]
fn test_visit_struct_item() {
    // Tests: visit_struct_item - struct definition with Self binding
    let source = dedent(
        "
        struct Container {
            value: i64,
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Container",
            BindExpect::new(SymKind::Struct)
                .expect_scope()
                .with_nested_types(vec!["i64"]),
        );
        assert_bind_symbol(
            cc,
            "Self",
            BindExpect::new(SymKind::TypeAlias)
                .with_type_of("Container")
                .expect_scope(),
        );
    });
}

#[serial]
#[test]
fn test_visit_field_declaration() {
    // Tests: visit_field_declaration - field with type assignment
    let source = dedent(
        "
        struct Record {
            id: u32,
            active: bool,
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "id",
            BindExpect::new(SymKind::Field)
                .with_type_of("u32")
                .with_field_of("Record"),
        );
        assert_bind_symbol(
            cc,
            "active",
            BindExpect::new(SymKind::Field)
                .with_type_of("bool")
                .with_field_of("Record"),
        );
    });
}

#[serial]
#[test]
fn test_visit_impl_item() {
    // Tests: visit_impl_item - impl block connecting methods to type
    let source = dedent(
        "
        struct Widget {
            size: i32,
        }

        impl Widget {
            fn new() -> Self {
                Widget { size: 0 }
            }

            fn get_size(&self) -> i32 {
                self.size
            }
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Widget",
            BindExpect::new(SymKind::Struct).expect_scope(),
        );
        assert_bind_symbol(cc, "new", BindExpect::new(SymKind::Function).expect_scope());
        assert_bind_symbol(
            cc,
            "get_size",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_type_of("i32"),
        );
    });
}

#[serial]
#[test]
fn test_visit_impl_item_with_trait() {
    // Tests: visit_impl_item - trait implementation links trait scope
    let source = dedent(
        "
        trait Printable {
            fn print(&self);
        }

        struct Message;

        impl Printable for Message {
            fn print(&self) {}
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Printable",
            BindExpect::new(SymKind::Trait).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "Message",
            BindExpect::new(SymKind::Struct).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "print",
            BindExpect::new(SymKind::Function).expect_scope(),
        );
    });
}

#[serial]
#[test]
fn test_visit_call_expression() {
    // Tests: visit_call_expression - function call binding
    let source = dedent(
        "
        fn helper() -> i32 { 42 }

        fn caller() {
            let result = helper();
            drop(result);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "helper",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_type_of("i32"),
        );
        assert_bind_symbol(cc, "result", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn test_visit_enum_item() {
    // Tests: visit_enum_item - enum definition with variants
    let source = dedent(
        "
        enum Status {
            Active,
            Inactive,
            Pending,
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "Status", BindExpect::new(SymKind::Enum).expect_scope());
        assert_bind_symbol(
            cc,
            "Active",
            BindExpect::new(SymKind::EnumVariant).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "Inactive",
            BindExpect::new(SymKind::EnumVariant).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "Pending",
            BindExpect::new(SymKind::EnumVariant).expect_scope(),
        );
    });
}

#[serial]
#[test]
fn test_visit_macro_definition() {
    // Tests: visit_macro_definition - macro_rules! definition
    let source = dedent(
        r#"
        macro_rules! my_macro {
            () => {};
        }
        "#,
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "my_macro",
            BindExpect::new(SymKind::Macro).expect_scope(),
        );
    });
}

#[serial]
#[test]
fn test_visit_macro_invocation() {
    // Tests: visit_macro_invocation - macro call like println!
    let source = dedent(
        r#"
        macro_rules! create_val {
            ($val:expr) => { $val };
        }

        fn use_macro() {
            let x = create_val!(42);
            drop(x);
        }
        "#,
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "create_val",
            BindExpect::new(SymKind::Macro).expect_scope(),
        );
        assert_bind_symbol(cc, "x", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn test_visit_generic_type() {
    // Tests: visit_generic_type - generic type parameters
    let source = dedent(
        "
        struct Container<T> {
            value: T,
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Container",
            BindExpect::new(SymKind::Struct).expect_scope(),
        );
        assert_bind_symbol(cc, "T", BindExpect::new(SymKind::TypeParameter));
    });
}

#[serial]
#[test]
fn test_visit_const_item() {
    // Tests: visit_const_item - const declaration with type
    let source = dedent(
        "
        const MAX_SIZE: usize = 100;
        const PI: f64 = 3.14159;
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "MAX_SIZE",
            BindExpect::new(SymKind::Const).with_type_of("usize"),
        );
        assert_bind_symbol(
            cc,
            "PI",
            BindExpect::new(SymKind::Const).with_type_of("f64"),
        );
    });
}

#[serial]
#[test]
fn test_visit_static_item() {
    // Tests: visit_static_item - static declaration with type
    let source = dedent(
        "
        static GLOBAL: i32 = 0;
        static mut COUNTER: u64 = 0;
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "GLOBAL",
            BindExpect::new(SymKind::Static).with_type_of("i32"),
        );
        assert_bind_symbol(
            cc,
            "COUNTER",
            BindExpect::new(SymKind::Static).with_type_of("u64"),
        );
    });
}

#[serial]
#[test]
fn test_visit_type_item() {
    // Tests: visit_type_item - type alias definition
    let source = dedent(
        "
        type Integer = i32;
        type Pair = (i32, i32);
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Integer",
            BindExpect::new(SymKind::TypeAlias).with_type_of("i32"),
        );
        // Pair is a type alias pointing to a composite tuple type
        assert_bind_symbol(cc, "Pair", BindExpect::new(SymKind::TypeAlias));
    });
}

#[serial]
#[test]
fn test_visit_array_type() {
    // Tests: visit_array_type - array type [T; N] with nested element type
    let source = dedent(
        "
        fn arrays() {
            let fixed: [i32; 5] = [1, 2, 3, 4, 5];
            drop(fixed);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "fixed", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn test_visit_tuple_type() {
    // Tests: visit_tuple_type - tuple type (T1, T2, T3)
    let source = dedent(
        "
        fn tuples() {
            let pair: (i32, bool) = (1, true);
            let triple: (i32, i64, f64) = (1, 2, 3.0);
            drop(pair);
            drop(triple);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "pair", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "triple", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn test_visit_abstract_type() {
    // Tests: visit_abstract_type - dyn Trait or impl Trait
    let source = dedent(
        "
        trait Drawable {
            fn draw(&self);
        }

        fn use_trait_object(obj: &dyn Drawable) {
            obj.draw();
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Drawable",
            BindExpect::new(SymKind::Trait).expect_scope(),
        );
        assert_bind_symbol(cc, "obj", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn test_visit_enum_variant() {
    // Tests: visit_enum_variant - enum variants with fields
    let source = dedent(
        "
        enum Message {
            Quit,
            Move { x: i32, y: i32 },
            Write(String),
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "Message", BindExpect::new(SymKind::Enum).expect_scope());
        assert_bind_symbol(
            cc,
            "Quit",
            BindExpect::new(SymKind::EnumVariant).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "Move",
            BindExpect::new(SymKind::EnumVariant).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "Write",
            BindExpect::new(SymKind::EnumVariant).expect_scope(),
        );
    });
}

#[serial]
#[test]
fn test_visit_field_expression() {
    // Tests: visit_field_expression - obj.field access
    let source = dedent(
        "
        struct Point {
            x: i32,
            y: i32,
        }

        fn access(p: Point) {
            let px = p.x;
            let py = p.y;
            drop(px);
            drop(py);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "x",
            BindExpect::new(SymKind::Field)
                .with_type_of("i32")
                .with_field_of("Point"),
        );
        assert_bind_symbol(
            cc,
            "y",
            BindExpect::new(SymKind::Field)
                .with_type_of("i32")
                .with_field_of("Point"),
        );
        assert_bind_symbol(
            cc,
            "px",
            BindExpect::new(SymKind::Variable).with_type_of("i32"),
        );
        assert_bind_symbol(
            cc,
            "py",
            BindExpect::new(SymKind::Variable).with_type_of("i32"),
        );
    });
}

#[serial]
#[test]
fn test_visit_field_expression_tuple_index() {
    // Tests: visit_field_expression - tuple.0 numeric field access
    let source = dedent(
        "
        fn tuple_access() {
            let pair: (i32, bool) = (42, true);
            let first = pair.0;
            let second = pair.1;
            drop(first);
            drop(second);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "pair", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "first", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "second", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn test_visit_scoped_identifier() {
    // Tests: visit_scoped_identifier - path::to::identifier resolution
    let source = dedent(
        "
        mod math {
            pub fn add(a: i32, b: i32) -> i32 {
                a + b
            }
        }

        fn caller() {
            let result = math::add(1, 2);
            drop(result);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "math",
            BindExpect::new(SymKind::Namespace).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "add",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_type_of("i32")
                .with_is_global(true),
        );
    });
}

#[serial]
#[test]
fn test_visit_parameter() {
    // Tests: visit_parameter - function parameter type binding
    let source = dedent(
        "
        struct Config {
            value: i32,
        }

        fn process(config: Config, count: usize, flag: bool) {
            drop(config);
            drop(count);
            drop(flag);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "config",
            BindExpect::new(SymKind::Variable).with_type_of("Config"),
        );
        assert_bind_symbol(
            cc,
            "count",
            BindExpect::new(SymKind::Variable).with_type_of("usize"),
        );
        assert_bind_symbol(
            cc,
            "flag",
            BindExpect::new(SymKind::Variable).with_type_of("bool"),
        );
    });
}

#[serial]
#[test]
fn test_visit_scoped_type_identifier() {
    // Tests: visit_scoped_type_identifier - path::to::Type resolution
    let source = dedent(
        "
        mod types {
            pub struct Inner {
                pub value: i32,
            }
        }

        fn use_type(val: types::Inner) {
            drop(val);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "types",
            BindExpect::new(SymKind::Namespace).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "Inner",
            BindExpect::new(SymKind::Struct)
                .expect_scope()
                .with_is_global(true),
        );
        assert_bind_symbol(
            cc,
            "val",
            BindExpect::new(SymKind::Variable).with_type_of("Inner"),
        );
    });
}

#[serial]
#[test]
fn test_visit_let_declaration() {
    // Tests: visit_let_declaration - variable binding with type inference
    let source = dedent(
        "
        struct Data { value: i32 }

        fn declarations() {
            let explicit: i32 = 10;
            let inferred = Data { value: 5 };
            let pattern: (i32, bool) = (1, true);
            drop(explicit);
            drop(inferred);
            drop(pattern);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "explicit",
            BindExpect::new(SymKind::Variable).with_type_of("i32"),
        );
        assert_bind_symbol(
            cc,
            "inferred",
            BindExpect::new(SymKind::Variable).with_type_of("Data"),
        );
        assert_bind_symbol(cc, "pattern", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn test_visit_tuple_struct_pattern() {
    // Tests: visit_tuple_struct_pattern - TupleStruct(a, b) pattern binding
    let source = dedent(
        "
        struct Coords(i32, i32);

        fn destruct(c: Coords) {
            let Coords(x, y) = c;
            drop(x);
            drop(y);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Coords",
            BindExpect::new(SymKind::Struct).expect_scope(),
        );
        assert_bind_symbol(cc, "x", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "y", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn test_visit_struct_expression() {
    // Tests: visit_struct_expression - Struct { field: value } literal
    let source = dedent(
        "
        struct Rectangle {
            width: u32,
            height: u32,
        }

        fn create() {
            let rect = Rectangle { width: 10, height: 20 };
            drop(rect);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Rectangle",
            BindExpect::new(SymKind::Struct)
                .expect_scope()
                .with_nested_types(vec!["u32", "u32"]),
        );
        assert_bind_symbol(
            cc,
            "rect",
            BindExpect::new(SymKind::Variable).with_type_of("Rectangle"),
        );
    });
}

#[serial]
#[test]
fn test_visit_match_expression() {
    // Tests: visit_match_expression - match scrutinee { arms }
    let source = dedent(
        "
        enum MyOption {
            Some(i32),
            None,
        }

        fn handle(opt: MyOption) -> i32 {
            match opt {
                MyOption::Some(_) => 1,
                MyOption::None => 0,
            }
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "MyOption",
            BindExpect::new(SymKind::Enum).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "Some",
            BindExpect::new(SymKind::EnumVariant).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "None",
            BindExpect::new(SymKind::EnumVariant).expect_scope(),
        );
        assert_bind_symbol(
            cc,
            "opt",
            BindExpect::new(SymKind::Variable).with_type_of("MyOption"),
        );
    });
}

#[serial]
#[test]
fn test_visit_match_block() {
    // Tests: visit_match_block - match arm body block
    let source = dedent(
        "
        fn match_blocks(x: i32) -> i32 {
            match x {
                0 => {
                    let zero = 0;
                    zero
                }
                n => {
                    let result = n * 2;
                    result
                }
            }
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "zero", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "result", BindExpect::new(SymKind::Variable));
    });
}
