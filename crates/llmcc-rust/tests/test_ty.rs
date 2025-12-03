/// Comprehensive tests for the type inference system (ty.rs)
///
/// These tests validate:
/// - Primitive type inference (literals, primitive_type nodes)
/// - Struct expression type inference
/// - Binary expression type inference
/// - Field expression type inference
/// - If expression type inference
/// - Block expression type inference
/// - Type resolution (canonical types, aliases)
/// - Scoped identifier resolution with kind filtering
/// - Type collection from generic expressions
/// - Callable resolution
mod common;

use common::{assert_depends, assert_exists, with_compiled_unit};
use llmcc_core::symbol::{DepKind, SymKind};

// ============================================================================
// Primitive Type Tests
// ============================================================================

#[test]
#[serial_test::serial]
fn test_primitive_i32_literal() {
    let source = r#"
        fn test() -> i32 {
            42
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_primitive_f64_literal() {
    let source = r#"
        fn test() -> f64 {
            3.14
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "f64",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_primitive_string_literal() {
    let source = r#"
        fn test() -> str {
            "hello"
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "str",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_primitive_bool_literal() {
    let source = r#"
        fn test() -> bool {
            true
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "bool",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_primitive_char_literal() {
    let source = r#"
        fn test() -> char {
            'a'
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "char",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Binary Expression Tests
// ============================================================================

#[test]
#[serial_test::serial]
fn test_binary_expression_bool_comparison_returns_bool() {
    let source = r#"
        fn test() -> bool {
            5 == 10
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "bool",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_binary_expression_greater_than_returns_bool() {
    let source = r#"
        fn test() -> bool {
            10 > 5
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "bool",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_binary_expression_less_than_returns_bool() {
    let source = r#"
        fn test() -> bool {
            10 < 20
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "bool",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_binary_expression_arithmetic_returns_left_type() {
    let source = r#"
        fn test() -> i32 {
            5 + 10
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_binary_expression_multiply_returns_left_type() {
    let source = r#"
        fn test() -> i32 {
            5 * 10
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_binary_expression_divide_returns_left_type() {
    let source = r#"
        fn test() -> i32 {
            10 / 2
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_binary_expression_modulo_returns_left_type() {
    let source = r#"
        fn test() -> i32 {
            10 % 3
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_binary_expression_logical_and_returns_bool() {
    let source = r#"
        fn test() -> bool {
            true && false
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "bool",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_binary_expression_logical_or_returns_bool() {
    let source = r#"
        fn test() -> bool {
            true || false
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "bool",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Struct Expression Tests
// ============================================================================

#[test]
#[serial_test::serial]
fn test_struct_expression_simple() {
    let source = r#"
        pub struct Point {
            x: i32,
            y: i32,
        }

        fn test() -> Point {
            Point { x: 10, y: 20 }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Point",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_struct_expression_with_nested_structs() {
    let source = r#"
        pub struct Config {
            timeout: i32,
        }

        pub struct Service {
            config: Config,
        }

        fn test() -> Service {
            Service { config: Config { timeout: 30 } }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Service",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_struct_expression_enum_variant() {
    let source = r#"
        pub enum Result<T> {
            Ok(T),
            Err,
        }

        fn test() -> Result<i32> {
            Result::Ok(42)
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Result",
            SymKind::Enum,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// If Expression Tests
// ============================================================================

#[test]
#[serial_test::serial]
fn test_if_expression_returns_consequence_type() {
    let source = r#"
        fn test(cond: bool) -> i32 {
            if cond {
                42
            } else {
                0
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_if_expression_struct_return() {
    let source = r#"
        pub struct Value;

        fn test(cond: bool) -> Value {
            if cond {
                Value
            } else {
                Value
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Value",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_if_expression_string_literal() {
    let source = r#"
        fn test(cond: bool) -> str {
            if cond {
                "yes"
            } else {
                "no"
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "str",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Block Expression Tests
// ============================================================================

#[test]
#[serial_test::serial]
fn test_block_returns_last_expression_type() {
    let source = r#"
        fn test() -> i32 {
            {
                42
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_block_with_multiple_statements() {
    let source = r#"
        pub struct Result;

        fn test() -> Result {
            {
                let x = 10;
                let y = 20;
                Result
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Result",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_nested_blocks() {
    let source = r#"
        fn test() -> i32 {
            {
                {
                    {
                        100
                    }
                }
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Field Expression Tests
// ============================================================================

#[test]
#[serial_test::serial]
fn test_field_expression_simple_struct_field() {
    let source = r#"
        pub struct Data {
            value: i32,
        }

        pub struct Container {
            data: Data,
        }

        impl Container {
            fn get_data(&self) -> Data {
                self.data
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "get_data",
            SymKind::Function,
            "Data",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_field_expression_method_call() {
    let source = r#"
        pub struct Handler;

        pub struct Worker {
            handler: Handler,
        }

        impl Worker {
            pub fn process(&self) -> Handler {
                self.handler
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "process",
            SymKind::Function,
            "Handler",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_field_expression_primitive_field() {
    let source = r#"
        pub struct Counter {
            count: i32,
        }

        impl Counter {
            pub fn get_count(&self) -> i32 {
                self.count
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "get_count",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_field_expression_nested_field_access() {
    let source = r#"
        pub struct Inner {
            value: i32,
        }

        pub struct Middle {
            inner: Inner,
        }

        pub struct Outer {
            middle: Middle,
        }

        impl Outer {
            pub fn get_inner(&self) -> Inner {
                self.middle.inner
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "get_inner",
            SymKind::Function,
            "Inner",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Scoped Identifier Tests - Comprehensive Coverage
// ============================================================================

#[test]
#[serial_test::serial]
fn test_scoped_identifier_simple_local() {
    let source = r#"
        pub struct Point;

        fn test() -> Point {
            Point
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Point",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_self_method_call() {
    let source = r#"
        pub struct Handler;

        pub struct Service;

        impl Service {
            fn create_handler(&self) -> Handler {
                Handler
            }

            fn use_handler(&self) -> Handler {
                Self::create_handler(self)
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "use_handler",
            SymKind::Function,
            "Handler",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );

        assert_depends(
            cc,
            "use_handler",
            SymKind::Function,
            "create_handler",
            SymKind::Function,
            Some(DepKind::Calls),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_self_associated_fn() {
    let source = r#"
        pub struct Config;

        pub struct Service;

        impl Service {
            fn create_config() -> Config {
                Config
            }

            fn init() -> Config {
                Self::create_config()
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "init",
            SymKind::Function,
            "Config",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );

        assert_depends(
            cc,
            "init",
            SymKind::Function,
            "create_config",
            SymKind::Function,
            Some(DepKind::Calls),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_crate_root() {
    let source = r#"
        pub struct Util;

        pub mod utils {
            use crate::Util;

            pub fn create_util() -> Util {
                Util
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "create_util",
            SymKind::Function,
            "Util",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_module_nested_two_levels() {
    let source = r#"
        pub struct Data;

        pub mod outer {
            pub fn create_data() -> Data {
                Data
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "create_data", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_module_nested_three_levels() {
    let source = r#"
        pub mod a {
            pub mod b {
                pub fn get_value() {
                    // nested function
                }
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "get_value", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_trait_associated_type() {
    let source = r#"
        pub struct Output;

        pub trait Processor {
            type Result;
            fn process(&self);
        }

        pub struct Handler;

        impl Processor for Handler {
            type Result = Output;

            fn process(&self) {}
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "Handler",
            SymKind::Struct,
            "Processor",
            SymKind::Trait,
            Some(DepKind::Implements),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_enum_variant_fully_qualified() {
    let source = r#"
        pub enum Status {
            Active,
            Inactive,
        }

        pub struct Service;

        impl Service {
            pub fn get_status() -> Status {
                Status::Active
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "get_status",
            SymKind::Function,
            "Status",
            SymKind::Enum,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_generic_type_parameter() {
    let source = r#"
        pub trait Handler {
            fn handle(&self);
        }

        pub struct Processor<T: Handler> {
            handler: T,
        }

        impl<T: Handler> Processor<T> {
            pub fn new(handler: T) -> Self {
                Processor { handler }
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "Processor",
            SymKind::Struct,
            "Handler",
            SymKind::Trait,
            Some(DepKind::Uses),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_use_statement_simple() {
    let source = r#"
        pub struct Util;

        pub mod utils {
            pub fn get_util() -> crate::Util {
                crate::Util
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "get_util",
            SymKind::Function,
            "Util",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_module_qualified_path() {
    let source = r#"
        pub struct Helper;

        pub fn create_helper() -> Helper {
            Helper
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "create_helper",
            SymKind::Function,
            "Helper",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_multiple_paths_in_same_fn() {
    let source = r#"
        pub struct Config;
        pub struct Handler;
        pub struct Util;

        pub fn complex() -> Config {
            Config
        }

        pub fn multi_uses() -> (Config, Handler) {
            (Config, Handler)
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "complex",
            SymKind::Function,
            "Config",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );

        assert_depends(
            cc,
            "multi_uses",
            SymKind::Function,
            "Config",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );

        assert_depends(
            cc,
            "multi_uses",
            SymKind::Function,
            "Handler",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_trait_with_type_param() {
    let source = r#"
        pub trait Logger {
            fn log(&self);
        }

        pub fn process_with_trait() -> Logger {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "process_with_trait",
            SymKind::Function,
            "Logger",
            SymKind::Trait,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_self_type_in_impl_return() {
    let source = r#"
        pub struct Builder;

        impl Builder {
            pub fn new() -> Builder {
                Builder
            }

            pub fn with_config(&self) -> Builder {
                Builder
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "new",
            SymKind::Function,
            "Builder",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );

        assert_depends(
            cc,
            "with_config",
            SymKind::Function,
            "Builder",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_super_keyword() {
    let source = r#"
        pub struct Parent;

        pub mod child {
            use super::Parent;

            pub fn use_parent() -> Parent {
                Parent
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "use_parent",
            SymKind::Function,
            "Parent",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_self_keyword_field_access() {
    let source = r#"
        pub struct Config {
            value: i32,
        }

        pub struct Service {
            config: Config,
        }

        impl Service {
            pub fn get_config(&self) -> Config {
                self.config
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "get_config",
            SymKind::Function,
            "Config",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_qualified_path_in_generic() {
    let source = r#"
        pub struct Data;

        pub enum Result<T> {
            Ok(T),
            Err(String),
        }

        pub fn create() -> Result<Data> {
            Result::Ok(Data)
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "create",
            SymKind::Function,
            "Result",
            SymKind::Enum,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_absolute_path_crate_colon_colon() {
    let source = r#"
        pub struct Global;

        pub mod submodule {
            pub fn access_global() -> crate::Global {
                crate::Global
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "access_global",
            SymKind::Function,
            "Global",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Unary Expression Tests
// ============================================================================

#[test]
#[serial_test::serial]
fn test_unary_expression_negation() {
    let source = r#"
        fn test() -> i32 {
            -42
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_unary_expression_logical_not() {
    let source = r#"
        fn test() -> bool {
            !true
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "bool",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_unary_expression_dereference() {
    let source = r#"
        fn test(ptr: &i32) -> i32 {
            *ptr
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Call Expression Tests
// ============================================================================

#[test]
#[serial_test::serial]
fn test_call_expression_returns_function_return_type() {
    let source = r#"
        pub struct Result;

        fn get_result() -> Result {
            Result
        }

        fn test() -> Result {
            get_result()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Result",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_call_expression_with_args() {
    let source = r#"
        pub struct Data;

        fn process(x: i32, y: i32) -> Data {
            Data
        }

        fn test() -> Data {
            process(10, 20)
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Data",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_call_expression_method_on_self() {
    let source = r#"
        pub struct Handler;

        pub struct Service;

        impl Service {
            fn helper(&self) -> Handler {
                Handler
            }

            fn test(&self) -> Handler {
                self.helper()
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Handler",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Type Resolution Tests (canonical types and type aliases)
// ============================================================================

#[test]
#[serial_test::serial]
fn test_type_alias_resolution() {
    let source = r#"
        pub struct MyString;
        pub type StringType = MyString;

        fn test() -> StringType {
            MyString
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        // Function return depends on the type alias
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "StringType",
            SymKind::TypeAlias,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_chained_type_alias_resolution() {
    let source = r#"
        pub struct BaseType;
        pub type InnerType = BaseType;
        pub type OuterType = InnerType;

        fn test() -> OuterType {
            BaseType
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        // Function depends on the outer alias
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "OuterType",
            SymKind::TypeAlias,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Complex Expression Combinations
// ============================================================================

#[test]
#[serial_test::serial]
fn test_complex_nested_expressions() {
    let source = r#"
        pub struct Container {
            items: i32,
        }

        impl Container {
            fn get_items(&self) -> i32 {
                if self.items > 0 {
                    self.items
                } else {
                    0
                }
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "get_items",
            SymKind::Function,
            "i32",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_match_like_if_chain() {
    let source = r#"
        pub struct A;
        pub struct B;

        fn test(choice: i32) -> A {
            if choice == 1 {
                A
            } else if choice == 2 {
                A
            } else {
                A
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "A",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_binary_with_struct_operands() {
    let source = r#"
        pub struct Data;

        fn compare(a: Data, b: Data) -> bool {
            a == b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "compare",
            SymKind::Function,
            "bool",
            SymKind::Primitive,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Generic Types Tests
// ============================================================================

#[test]
#[serial_test::serial]
fn test_generic_struct_return() {
    let source = r#"
        pub struct Option<T> {
            value: T,
        }

        fn test() -> Option<i32> {
            Option { value: 42 }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Option",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_generic_with_multiple_type_params() {
    let source = r#"
        pub struct Result<T, E> {
            ok: T,
            err: E,
        }

        fn test() -> Result<i32, str> {
            Result { ok: 42, err: "error" }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Result",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Trait and Impl Tests
// ============================================================================

#[test]
#[serial_test::serial]
fn test_trait_method_call_returns_type() {
    let source = r#"
        pub struct Handler;

        pub trait Worker {
            fn handle(&self) -> Handler;
        }

        pub struct Service;

        impl Worker for Service {
            fn handle(&self) -> Handler {
                Handler
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "handle",
            SymKind::Function,
            "Handler",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_associated_function_return() {
    let source = r#"
        pub struct Point;

        impl Point {
            pub fn origin() -> Point {
                Point
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "origin",
            SymKind::Function,
            "Point",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

// ============================================================================
// Edge Cases and Error Conditions
// ============================================================================

#[test]
#[serial_test::serial]
fn test_empty_block_with_unit_return() {
    let source = r#"
        fn test() {
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_multiple_return_statements_same_type() {
    let source = r#"
        pub struct Value;

        fn test(cond: bool) -> Value {
            if cond {
                Value
            } else {
                Value
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Value",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}

#[test]
#[serial_test::serial]
fn test_deeply_nested_generic() {
    let source = r#"
        pub struct Vec<T>;
        pub struct Option<T>;

        fn test() -> Option<Vec<i32>> {
            Option
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(
            cc,
            "test",
            SymKind::Function,
            "Option",
            SymKind::Struct,
            Some(DepKind::ReturnType),
        );
    });
}
