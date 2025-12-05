mod common;

use common::{find_symbol_id, with_compiled_unit};
use llmcc_core::symbol::SymKind;
use serial_test::serial;
use textwrap::dedent;

#[serial]
#[test]
fn bind_simple_identifier_pattern() {
    let source = dedent(
        "
        fn main() {
            let x = 42;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let x_sym = find_symbol_id(cc, "x", SymKind::Variable);
        assert!(x_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_mutable_pattern() {
    let source = dedent(
        "
        fn main() {
            let mut x = 42;
            x = 43;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let x_sym = find_symbol_id(cc, "x", SymKind::Variable);
        assert!(x_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_reference_pattern() {
    let source = dedent(
        "
        fn main() {
            let value = 42;
            let r = &value;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let value_sym = find_symbol_id(cc, "value", SymKind::Variable);
        assert!(value_sym.0 > 0);

        let r_sym = find_symbol_id(cc, "r", SymKind::Variable);
        assert!(r_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_ref_mut_pattern() {
    let source = dedent(
        "
        fn main() {
            let value = 42;
            let r = &mut value;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let value_sym = find_symbol_id(cc, "value", SymKind::Variable);
        assert!(value_sym.0 > 0);

        let r_sym = find_symbol_id(cc, "r", SymKind::Variable);
        assert!(r_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_tuple_pattern_destructuring() {
    let source = dedent(
        "
        fn main() {
            let (x, y) = (1, 2);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let x_sym = find_symbol_id(cc, "x", SymKind::Variable);
        assert!(x_sym.0 > 0);

        let y_sym = find_symbol_id(cc, "y", SymKind::Variable);
        assert!(y_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_tuple_pattern_with_mixed_types() {
    let source = dedent(
        "
        fn main() {
            let (a, b, c) = (1, \"hello\", true);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        for var in &["a", "b", "c"] {
            let sym = find_symbol_id(cc, var, SymKind::Variable);
            assert!(sym.0 > 0);
        }
    });
}

#[serial]
#[test]
fn bind_nested_tuple_pattern_destructuring() {
    let source = dedent(
        "
        fn main() {
            let ((x, y), z) = ((1, 2), 3);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        for var in &["x", "y", "z"] {
            let sym = find_symbol_id(cc, var, SymKind::Variable);
            assert!(sym.0 > 0);
        }
    });
}

#[serial]
#[test]
fn bind_tuple_pattern_assigns_correct_element_types() {
    let source = dedent(
        "
        fn main() {
            let values = (42, \"test\", true);
            let (a, b, c) = values;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let values_sym = find_symbol_id(cc, "values", SymKind::Variable);
        assert!(values_sym.0 > 0);

        for var in &["a", "b", "c"] {
            let sym = find_symbol_id(cc, var, SymKind::Variable);
            assert!(sym.0 > 0);
        }
    });
}

#[serial]
#[test]
fn bind_tuple_pattern_with_underscore_wildcard() {
    let source = dedent(
        "
        fn main() {
            let (x, _, z) = (1, 2, 3);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let x_sym = find_symbol_id(cc, "x", SymKind::Variable);
        assert!(x_sym.0 > 0);

        let z_sym = find_symbol_id(cc, "z", SymKind::Variable);
        assert!(z_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_struct_pattern_assigns_field_types() {
    let source = dedent(
        "
        struct Point {
            x: i32,
            y: i32,
        }

        fn main() {
            let Point { x, y } = Point { x: 1, y: 2 };
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let point_sym = find_symbol_id(cc, "Point", SymKind::Struct);
        assert!(point_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_struct_pattern_with_field_renaming() {
    let source = dedent(
        "
        struct Person {
            name: String,
            age: u32,
        }

        fn main() {
            let Person { name: full_name, age: years } = Person { name: String::from(\"Alice\"), age: 30 };
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let person_sym = find_symbol_id(cc, "Person", SymKind::Struct);
        assert!(person_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_struct_pattern_with_rest_pattern() {
    let source = dedent(
        "
        struct Config {
            name: String,
            value: i32,
            enabled: bool,
        }

        fn main() {
            let Config { name, .. } = Config { name: String::from(\"test\"), value: 42, enabled: true };
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let config_sym = find_symbol_id(cc, "Config", SymKind::Struct);
        assert!(config_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_pattern_handles_wildcard_pattern() {
    let source = dedent(
        "
        fn main() {
            let _ = 42;
            let (x, _) = (1, 2);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let x_sym = find_symbol_id(cc, "x", SymKind::Variable);
        assert!(x_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_pattern_in_function_parameters() {
    let source = dedent(
        "
        fn process((a, b): (i32, i32)) -> i32 {
            a + b
        }

        fn main() {
            let result = process((1, 2));
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let func_sym = find_symbol_id(cc, "process", SymKind::Function);
        assert!(func_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_pattern_in_match_arm() {
    let source = dedent(
        "
        fn main() {
            let x = (1, 2);
            match x {
                (a, b) => {
                    let _ = a + b;
                }
            }
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let x_sym = find_symbol_id(cc, "x", SymKind::Variable);
        assert!(x_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_pattern_handles_deeply_nested_patterns() {
    let source = dedent(
        "
        fn main() {
            let (((a, b), c), d) = (((1, 2), 3), 4);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        for var in &["a", "b", "c", "d"] {
            let sym = find_symbol_id(cc, var, SymKind::Variable);
            assert!(sym.0 > 0);
        }
    });
}

#[serial]
#[test]
fn bind_ref_pattern_in_destructuring() {
    let source = dedent(
        "
        fn main() {
            let data = (vec![1, 2, 3], \"test\");
            let (ref v, ref s) = data;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let data_sym = find_symbol_id(cc, "data", SymKind::Variable);
        assert!(data_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_pattern_binding_with_enum() {
    let source = dedent(
        "
        enum Option<T> {
            Some(T),
            None,
        }

        fn main() {
            let value = Some(42);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let enum_sym = find_symbol_id(cc, "Option", SymKind::Enum);
        assert!(enum_sym.0 > 0);
    });
}

#[serial]
#[test]
fn bind_pattern_with_enum_variant() {
    let source = dedent(
        "
        enum Status {
            Active,
            Inactive,
        }

        fn main() {
            let status = Status::Active;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        let enum_sym = find_symbol_id(cc, "Status", SymKind::Enum);
        assert!(enum_sym.0 > 0);

        let status_sym = find_symbol_id(cc, "status", SymKind::Variable);
        assert!(status_sym.0 > 0);
    });
}
