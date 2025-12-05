mod common;

use llmcc_core::symbol::SymKind;

use common::{BindExpect, assert_bind_symbol, with_compiled_unit};
use serial_test::serial;
use textwrap::dedent;

#[serial]
#[test]
fn bind_function_sets_return_type_and_global_flags() {
    let source = dedent(
        "
        pub fn exported() -> i32 {
            1
        }

        fn main() {}

        fn internal() {}
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "exported",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_type_of("i32")
                .with_is_global(true),
        );

        assert_bind_symbol(
            cc,
            "main",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_is_global(true),
        );

        assert_bind_symbol(
            cc,
            "internal",
            BindExpect::new(SymKind::Function).expect_scope(),
        );
    });
}

#[serial]
#[test]
fn bind_struct_item_sets_field_relationships() {
    let source = dedent(
        "
        struct Point {
            x: i32,
            y: i32,
        }

        fn consume(p: Point) {
            let Point { x, y } = p;
            let sum = x + y;
            let arr: [i32; 1] = [2];
            drop(sum);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Point",
            BindExpect::new(SymKind::Struct)
                .expect_scope()
                .with_nested_types(vec!["i32", "i32"]),
        );

        for field in ["x", "y"] {
            assert_bind_symbol(
                cc,
                field,
                BindExpect::new(SymKind::Field)
                    .with_type_of("i32")
                    .with_field_of("Point"),
            );
        }

        assert_bind_symbol(
            cc,
            "Self",
            BindExpect::new(SymKind::TypeAlias)
                .with_type_of("Point")
                .expect_scope(),
        );

        assert_bind_symbol(
            cc,
            "p",
            BindExpect::new(SymKind::Variable).with_type_of("Point"),
        );

        assert_bind_symbol(cc, "sum", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn bind_const_static_and_type_aliases() {
    let source = dedent(
        "
        const MAX: i64 = 10;
        static FLAG: bool = true;
        type MyInt = i32;
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "MAX",
            BindExpect::new(SymKind::Const).with_type_of("i64"),
        );

        assert_bind_symbol(
            cc,
            "FLAG",
            BindExpect::new(SymKind::Static).with_type_of("bool"),
        );

        assert_bind_symbol(
            cc,
            "MyInt",
            BindExpect::new(SymKind::TypeAlias).with_type_of("i32"),
        );
    });
}

#[serial]
#[test]
fn bind_parameters_and_tuple_struct_patterns() {
    let source = dedent(
        "
        struct Pair(i32, bool);

        fn consume(pair: Pair, (a, b): (i32, bool)) -> bool {
            let Pair(inner, flag) = pair;
            flag || b
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "Pair", BindExpect::new(SymKind::Struct).expect_scope());

        assert_bind_symbol(
            cc,
            "pair",
            BindExpect::new(SymKind::Variable).with_type_of("Pair"),
        );

        assert_bind_symbol(cc, "a", BindExpect::new(SymKind::Variable));

        assert_bind_symbol(cc, "inner", BindExpect::new(SymKind::Variable));

        assert_bind_symbol(cc, "flag", BindExpect::new(SymKind::Variable));
    });
}

#[serial]
#[test]
fn bind_scoped_identifier_and_struct_literal() {
    let source = dedent(
        "
        mod container {
            pub struct Wrapper {
                pub value: i32,
            }
        }

        fn main() {
            let w = container::Wrapper { value: 5 };
            let v = w.value;
            drop(v);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Wrapper",
            BindExpect::new(SymKind::Struct).expect_scope(),
        );

        assert_bind_symbol(
            cc,
            "value",
            BindExpect::new(SymKind::Field)
                .with_type_of("i32")
                .with_field_of("Wrapper"),
        );

        assert_bind_symbol(
            cc,
            "w",
            BindExpect::new(SymKind::Variable).with_type_of("Wrapper"),
        );

        assert_bind_symbol(
            cc,
            "v",
            BindExpect::new(SymKind::Variable).with_type_of("i32"),
        );
    });
}

#[serial]
#[test]
fn bind_module_scopes_and_visibility() {
    let source = dedent(
        "
        mod outer {
            pub fn exposed() {}
            fn hidden() {}
        }

        fn call() {
            outer::exposed();
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "exposed",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_is_global(true),
        );

        assert_bind_symbol(
            cc,
            "hidden",
            BindExpect::new(SymKind::Function).expect_scope(),
        );

        assert_bind_symbol(
            cc,
            "call",
            BindExpect::new(SymKind::Function).expect_scope(),
        );

        assert_bind_symbol(
            cc,
            "outer",
            BindExpect::new(SymKind::Namespace).expect_scope(),
        );
    });
}

#[serial]
#[test]
fn bind_scoped_identifier_resolves_nested_paths() {
    let source = dedent(
        "
        mod outer {
            pub mod inner {
                pub fn target() {}
            }
        }

        fn call() {
            outer::inner::target();
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "outer",
            BindExpect::new(SymKind::Namespace).expect_scope(),
        );

        assert_bind_symbol(
            cc,
            "inner",
            BindExpect::new(SymKind::Namespace)
                .expect_scope()
                .with_is_global(true),
        );

        assert_bind_symbol(
            cc,
            "target",
            BindExpect::new(SymKind::Function)
                .expect_scope()
                .with_is_global(true),
        );

        assert_bind_symbol(
            cc,
            "call",
            BindExpect::new(SymKind::Function).expect_scope(),
        );
    });
}

#[serial]
#[test]
fn bind_let_with_explicit_types_assigns_variables() {
    let source = dedent(
        "
        fn main() {
            let value: i64 = 42;
            let (left, right): (i32, bool) = (1, true);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        common::debug_symbol_types(cc);
        assert_bind_symbol(
            cc,
            "value",
            BindExpect::new(SymKind::Variable).with_type_of("i64"),
        );

        assert_bind_symbol(
            cc,
            "left",
            BindExpect::new(SymKind::Variable).with_type_of("i32"),
        );

        assert_bind_symbol(
            cc,
            "right",
            BindExpect::new(SymKind::Variable).with_type_of("bool"),
        );
    });
}
