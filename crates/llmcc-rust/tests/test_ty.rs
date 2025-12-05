mod common;

use llmcc_core::context::CompileCtxt;
use llmcc_core::symbol::SymKind;

use common::{find_symbol_id, with_compiled_unit};
use serial_test::serial;
use textwrap::dedent;

fn assert_infer_type<'tcx>(cc: &'tcx CompileCtxt<'tcx>, name: &str, expected: (&str, SymKind)) {
    let var_id = find_symbol_id(cc, name, SymKind::Variable);
    let var_symbol = cc
        .opt_get_symbol(var_id)
        .unwrap_or_else(|| panic!("missing variable symbol: {name}"));
    let type_id = var_symbol
        .type_of()
        .unwrap_or_else(|| panic!("variable {name} has no inferred type"));
    let type_symbol = cc
        .opt_get_symbol(type_id)
        .unwrap_or_else(|| panic!("missing type symbol {type_id:?} for {name}"));
    let type_name = cc
        .interner
        .resolve_owned(type_symbol.name)
        .unwrap_or_else(|| "<unknown>".to_string());

    assert_eq!(
        (type_name.as_str(), type_symbol.kind()),
        (expected.0, expected.1),
        "expected '{name}' to have type {} but found {}",
        expected.0,
        type_symbol.format(Some(&cc.interner))
    );
}

#[serial]
#[test]
fn infers_boolean_literals_as_bool() {
    let source = dedent(
        "
        fn main() {
            let x = true;
            let y = false;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "x", ("bool", SymKind::Primitive));
        assert_infer_type(cc, "y", ("bool", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn infers_integer_literals_as_i32() {
    let source = dedent(
        "
        fn main() {
            let a = 42;
            let b = -10;
            let c = 0;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        for var in ["a", "b", "c"] {
            assert_infer_type(cc, var, ("i32", SymKind::Primitive));
        }
    });
}

#[serial]
#[test]
fn infers_float_literals_as_f64() {
    let source = dedent(
        "
        fn main() {
            let x = 3.14;
            let y = 2.0;
            let z = -1.5;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        for var in ["x", "y", "z"] {
            assert_infer_type(cc, var, ("f64", SymKind::Primitive));
        }
    });
}

#[serial]
#[test]
fn infers_char_literals_as_char() {
    let source = dedent(
        "
        fn main() {
            let c = 'a';
            let d = '1';
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        for var in ["c", "d"] {
            assert_infer_type(cc, var, ("char", SymKind::Primitive));
        }
    });
}

#[serial]
#[test]
fn infers_struct_literal_as_struct_type() {
    let source = dedent(
        "
        struct Point {
            x: i32,
            y: i32,
        }

        fn main() {
            let p = Point { x: 0, y: 0 };
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(find_symbol_id(cc, "Point", SymKind::Struct).0 > 0);
        assert_infer_type(cc, "p", ("Point", SymKind::Struct));
    });
}

#[serial]
#[test]
fn honors_explicit_type_annotations() {
    let source = dedent(
        "
        fn main() {
            let a: i32 = 10;
            let b: f64 = 3.14;
            let c: bool = true;
            let d: char = 'x';
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "a", ("i32", SymKind::Primitive));
        assert_infer_type(cc, "b", ("f64", SymKind::Primitive));
        assert_infer_type(cc, "c", ("bool", SymKind::Primitive));
        assert_infer_type(cc, "d", ("char", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn binary_comparisons_produce_bool() {
    let source = dedent(
        "
        fn main() {
            let less = 5 < 10;
            let greater = 3 > 2;
            let equals = 4 == 4;
            let not_equals = 5 != 3;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        common::debug_symbol_types(cc);
        for var in ["less", "greater", "equals", "not_equals"] {
            assert_infer_type(cc, var, ("bool", SymKind::Primitive));
        }
    });
}

#[serial]
#[test]
fn arithmetic_operators_keep_operand_type() {
    let source = dedent(
        "
        fn main() {
            let sum = 5 + 3;
            let difference = 10 - 4;
            let product = 6 * 7;
            let quotient = 15 / 3;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        for var in ["sum", "difference", "product", "quotient"] {
            assert_infer_type(cc, var, ("i32", SymKind::Primitive));
        }
    });
}

#[serial]
#[test]
fn logical_operators_return_bool() {
    let source = dedent(
        "
        fn main() {
            let conj = true && false;
            let disj = true || false;
            let neg = !true;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        for var in ["conj", "disj", "neg"] {
            assert_infer_type(cc, var, ("bool", SymKind::Primitive));
        }
    });
}

#[serial]
#[test]
fn field_access_uses_field_type() {
    let source = dedent(
        "
        struct Point {
            x: i32,
            y: i32,
        }

        fn main() {
            let point = Point { x: 1, y: 2 };
            let x_value = point.x;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "point", ("Point", SymKind::Struct));
        assert_infer_type(cc, "x_value", ("i32", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn block_expression_type_matches_last_expr() {
    let source = dedent(
        "
        fn main() {
            let total = {
                let a = 5;
                let b = 10;
                a + b
            };
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "total", ("i32", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn if_expression_type_is_inferred() {
    let source = dedent(
        "
        fn main() {
            let numbers = if true { 5 } else { 6 };
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "numbers", ("i32", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn array_literal_infers_element_type() {
    let source = dedent(
        "
        fn main() {
            let inline = [1, 2, 3];
            let repeated = [7; 4];
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "inline", ("i32", SymKind::Primitive));
        assert_infer_type(cc, "repeated", ("i32", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn range_expression_infers_i32() {
    let source = dedent(
        "
        fn main() {
            let simple = 0..5;
            let inclusive = 1..=3;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "simple", ("i32", SymKind::Primitive));
        assert_infer_type(cc, "inclusive", ("i32", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn type_cast_expression_infers_target_type() {
    let source = dedent(
        "
        fn main() {
            let as_float = 5 as f64;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "as_float", ("f64", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn scoped_function_call_infers_return_type() {
    let source = dedent(
        "
        mod math {
            pub fn identity(x: i32) -> i32 {
                x
            }
        }

        fn main() {
            let result = math::identity(5);
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "result", ("i32", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn infers_string_literals_as_str() {
    let source = dedent(
        "
        fn main() {
            let greeting = \"hello\";
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "greeting", ("str", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn scoped_struct_expression_infers_struct_type() {
    let source = dedent(
        "
        mod container {
            pub struct Wrapper {
                pub value: i32,
            }

            pub struct Unit;
        }

        fn main() {
            let wrapped = container::Wrapper { value: 7 };
            let unit = container::Unit;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "wrapped", ("Wrapper", SymKind::Struct));
        assert_infer_type(cc, "unit", ("Unit", SymKind::Struct));
    });
}

#[serial]
#[test]
fn unary_operators_preserve_operand_type() {
    let source = dedent(
        "
        fn main() {
            let base = 8;
            let negated = -base;
            let reference = &base;
            let dereferenced = *reference;

            let flag = false;
            let inverted = !flag;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "negated", ("i32", SymKind::Primitive));
        assert_infer_type(cc, "reference", ("i32", SymKind::Primitive));
        assert_infer_type(cc, "dereferenced", ("i32", SymKind::Primitive));
        assert_infer_type(cc, "inverted", ("bool", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn modulo_and_relational_operators_infer_expected_types() {
    let source = dedent(
        "
        fn main() {
            let remainder = 10 % 3;
            let leq = 3 <= 5;
            let geq = 8 >= 2;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "remainder", ("i32", SymKind::Primitive));
        assert_infer_type(cc, "leq", ("bool", SymKind::Primitive));
        assert_infer_type(cc, "geq", ("bool", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn range_variants_infer_i32() {
    let source = dedent(
        "
        fn main() {
            let upto = ..5;
            let from = 5..;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "upto", ("i32", SymKind::Primitive));
        assert_infer_type(cc, "from", ("i32", SymKind::Primitive));
    });
}

#[serial]
#[test]
fn composite_type_annotations_resolve_element_types() {
    let source = dedent(
        "
        fn passthrough(x: i32) -> i32 {
            x
        }

        fn main() {
            let numbers: [i32; 2] = [1, 2];
            let reference: &i32 = &numbers[0];
            let pointer: *const i32 = reference as *const i32;
            let function: fn(i32) -> i32 = passthrough;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert_infer_type(cc, "numbers", ("i32", SymKind::Primitive));
        assert_infer_type(cc, "reference", ("i32", SymKind::Primitive));
        assert_infer_type(cc, "pointer", ("i32", SymKind::Primitive));
        assert_infer_type(cc, "function", ("i32", SymKind::Primitive));
    });
}
