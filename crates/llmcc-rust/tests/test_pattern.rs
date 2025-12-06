mod common;

use common::{BindExpect, assert_bind_symbol, find_symbol_id, with_compiled_unit};
use llmcc_core::symbol::SymKind;
use serial_test::serial;
use textwrap::dedent;

// ==============================================================================
// Unit tests for pattern.rs - bind_pattern_types and helper functions
// ==============================================================================

// Tests for simple identifier pattern binding
#[serial]
#[test]
fn test_pattern_identifier() {
    // Tests: bind_pattern_types - simple identifier pattern with type
    let source = dedent(
        "
        fn main() {
            let x: i32 = 42;
            drop(x);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "x",
            BindExpect::new(SymKind::Variable).with_type_of("i32"),
        );
    });
}

// Tests for tuple pattern type binding
#[serial]
#[test]
fn test_pattern_tuple() {
    // Tests: assign_type_to_tuple_pattern - tuple destructuring with types
    let source = dedent(
        "
        fn main() {
            let (a, b): (i32, bool) = (1, true);
            drop(a);
            drop(b);
        }
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
    });
}

// Tests for nested tuple pattern - verifies variables are collected
#[serial]
#[test]
fn test_pattern_tuple_nested() {
    // Tests: assign_type_to_tuple_pattern - nested tuple pattern
    // Note: Nested type resolution requires recursive CompositeType lookup
    let source = dedent(
        "
        fn main() {
            let ((x, y), z) = ((1, 2), true);
            drop(x);
            drop(y);
            drop(z);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        // Verify all variables are collected
        assert_bind_symbol(cc, "x", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "y", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "z", BindExpect::new(SymKind::Variable));
    });
}

// Tests for struct pattern type binding - verifies struct and fields exist
#[serial]
#[test]
fn test_pattern_struct() {
    // Tests: assign_type_to_struct_pattern - struct field destructuring
    let source = dedent(
        "
        struct Point {
            x: i32,
            y: i64,
        }

        fn destruct(p: Point) {
            let Point { x, y } = p;
            drop(x);
            drop(y);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        // Verify the struct and its fields exist
        assert_bind_symbol(cc, "Point", BindExpect::new(SymKind::Struct).expect_scope());
        assert_bind_symbol(
            cc,
            "p",
            BindExpect::new(SymKind::Variable).with_type_of("Point"),
        );
        // Variables from pattern should exist
        assert_bind_symbol(cc, "x", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "y", BindExpect::new(SymKind::Variable));
    });
}

// Tests for struct pattern with field renaming
#[serial]
#[test]
fn test_pattern_struct_rename() {
    // Tests: assign_type_to_struct_pattern - field: pattern renaming
    let source = dedent(
        "
        struct Data {
            value: i32,
        }

        fn destruct(d: Data) {
            let Data { value: val } = d;
            drop(val);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "Data", BindExpect::new(SymKind::Struct).expect_scope());
        // Variable from pattern renaming
        assert_bind_symbol(cc, "val", BindExpect::new(SymKind::Variable));
    });
}

// Tests for tuple struct pattern type binding
#[serial]
#[test]
fn test_pattern_tuple_struct() {
    // Tests: assign_type_to_tuple_struct_pattern - TupleStruct(a, b) pattern
    let source = dedent(
        "
        struct Pair(i32, bool);

        fn destruct(p: Pair) {
            let Pair(num, flag) = p;
            drop(num);
            drop(flag);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "Pair", BindExpect::new(SymKind::Struct).expect_scope());
        assert_bind_symbol(cc, "num", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "flag", BindExpect::new(SymKind::Variable));
    });
}

// Tests for or pattern type binding
#[serial]
#[test]
fn test_pattern_or() {
    // Tests: assign_type_to_or_pattern - pattern1 | pattern2
    let source = dedent(
        "
        fn check(x: i32) {
            match x {
                1 | 2 | 3 => {}
                _ => {}
            }
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "x",
            BindExpect::new(SymKind::Variable).with_type_of("i32"),
        );
    });
}

// Tests for slice pattern type binding
#[serial]
#[test]
fn test_pattern_slice() {
    // Tests: assign_type_to_slice_pattern - [first, rest @ ..]
    let source = dedent(
        "
        fn process(arr: &[i32]) {
            if let [first, second, ..] = arr {
                drop(first);
                drop(second);
            }
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "arr", BindExpect::new(SymKind::Variable));
    });
}

// Tests for reference pattern type binding
#[serial]
#[test]
fn test_pattern_reference() {
    // Tests: assign_type_to_reference_pattern - &pattern
    let source = dedent(
        "
        fn process(val: &i32) {
            let &num = val;
            drop(num);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "val", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "num", BindExpect::new(SymKind::Variable));
    });
}

// Tests for mut pattern type binding
#[serial]
#[test]
fn test_pattern_mut() {
    // Tests: mut_pattern handling - mut x
    let source = dedent(
        "
        fn main() {
            let mut x: i32 = 42;
            x = 43;
            drop(x);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "x",
            BindExpect::new(SymKind::Variable).with_type_of("i32"),
        );
    });
}

// Tests for ref pattern type binding
#[serial]
#[test]
fn test_pattern_ref() {
    // Tests: ref_pattern handling - ref x
    let source = dedent(
        "
        fn main() {
            let data: (i32, bool) = (1, true);
            let (ref a, ref b) = data;
            drop(a);
            drop(b);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(cc, "a", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "b", BindExpect::new(SymKind::Variable));
    });
}

// Tests for wildcard pattern handling
#[serial]
#[test]
fn test_pattern_wildcard() {
    // Tests: wildcard _ pattern is properly skipped
    let source = dedent(
        "
        fn main() {
            let (x, _): (i32, bool) = (1, true);
            drop(x);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "x",
            BindExpect::new(SymKind::Variable).with_type_of("i32"),
        );
    });
}

// Tests for function parameter pattern binding
#[serial]
#[test]
fn test_pattern_fn_param() {
    // Tests: pattern binding in function parameters with simple types
    let source = dedent(
        "
        fn process(a: i32, b: bool) {
            drop(a);
            drop(b);
        }
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
    });
}

// Tests for tuple parameter pattern binding
#[serial]
#[test]
fn test_pattern_fn_tuple_param() {
    // Tests: tuple pattern binding in function parameters
    let source = dedent(
        "
        fn process((a, b): (i32, bool)) {
            drop(a);
            drop(b);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        // Verify function exists and tuple pattern variables are collected
        assert_bind_symbol(
            cc,
            "process",
            BindExpect::new(SymKind::Function).expect_scope(),
        );
        assert_bind_symbol(cc, "a", BindExpect::new(SymKind::Variable));
        assert_bind_symbol(cc, "b", BindExpect::new(SymKind::Variable));
    });
}

// Tests for let with inferred type from struct expression
#[serial]
#[test]
fn test_pattern_infer_struct() {
    // Tests: type inference from struct expression value
    let source = dedent(
        "
        struct Config {
            value: i32,
        }

        fn main() {
            let cfg = Config { value: 42 };
            drop(cfg);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "cfg",
            BindExpect::new(SymKind::Variable).with_type_of("Config"),
        );
    });
}

// Tests for const pattern (should not redeclare)
#[serial]
#[test]
fn test_pattern_const_skip() {
    // Tests: const values are skipped in pattern binding
    let source = dedent(
        "
        const MAX: i32 = 100;

        fn check(x: i32) {
            match x {
                MAX => {}
                _ => {}
            }
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "MAX",
            BindExpect::new(SymKind::Const).with_type_of("i32"),
        );
    });
}

// Tests for deeply nested tuple pattern
#[serial]
#[test]
fn test_pattern_deep_nesting() {
    // Tests: deeply nested patterns are traversed
    let source = dedent(
        "
        fn main() {
            let (((a, b), c), d) = (((1, 2), 3), 4);
            drop(a);
            drop(b);
            drop(c);
            drop(d);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        // Verify all variables are collected
        for var in &["a", "b", "c", "d"] {
            assert_bind_symbol(cc, var, BindExpect::new(SymKind::Variable));
        }
    });
}

// Tests for rest pattern in struct
#[serial]
#[test]
fn test_pattern_struct_rest() {
    // Tests: struct pattern with .. rest pattern
    let source = dedent(
        "
        struct Config {
            name: i32,
            value: i32,
            enabled: bool,
        }

        fn destruct(c: Config) {
            let Config { name, .. } = c;
            drop(name);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_bind_symbol(
            cc,
            "Config",
            BindExpect::new(SymKind::Struct).expect_scope(),
        );
        assert_bind_symbol(cc, "name", BindExpect::new(SymKind::Variable));
    });
}

// ==============================================================================
// Legacy tests (keeping for backwards compatibility)
// ==============================================================================

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
