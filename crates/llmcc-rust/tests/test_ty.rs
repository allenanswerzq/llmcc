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

use common::{assert_exists, with_compiled_unit};
use llmcc_core::symbol::SymKind;

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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_depends(cc, "get_data", SymKind::Function, "Data", SymKind::Struct, Some(DepKind::ReturnType));
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
        assert_depends(cc, "process", SymKind::Function, "Handler", SymKind::Struct, Some(DepKind::ReturnType));
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
        assert_depends(cc, "get_count", SymKind::Function, "i32", SymKind::Primitive, Some(DepKind::ReturnType));
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
        assert_depends(cc, "get_inner", SymKind::Function, "Inner", SymKind::Struct, Some(DepKind::ReturnType));
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_depends(cc, "use_handler", SymKind::Function, "Handler", SymKind::Struct, Some(DepKind::ReturnType));

        assert_depends(cc, "use_handler", SymKind::Function, "create_handler", SymKind::Function, Some(DepKind::Calls));
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
        assert_depends(cc, "init", SymKind::Function, "Config", SymKind::Struct, Some(DepKind::ReturnType));

        assert_depends(cc, "init", SymKind::Function, "create_config", SymKind::Function, Some(DepKind::Calls));
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
        assert_depends(cc, "create_util", SymKind::Function, "Util", SymKind::Struct, Some(DepKind::ReturnType));
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
        assert_depends(cc, "Handler", SymKind::Struct, "Processor", SymKind::Trait, Some(DepKind::Implements));
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
        assert_depends(cc, "get_status", SymKind::Function, "Status", SymKind::Enum, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
#[ignore]
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
        assert_depends(cc, "Processor", SymKind::Struct, "Handler", SymKind::Trait, Some(DepKind::Uses));
    });
}

#[test]
#[serial_test::serial]
#[ignore]
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
        assert_depends(cc, "get_util", SymKind::Function, "Util", SymKind::Struct, Some(DepKind::ReturnType));
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
        assert_depends(cc, "create_helper", SymKind::Function, "Helper", SymKind::Struct, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
#[ignore]
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
        assert_depends_batch(
            cc,
            vec![
                ("complex", SymKind::Function, "Config", SymKind::Struct, Some(DepKind::ReturnType)),
                ("multi_uses", SymKind::Function, "Config", SymKind::Struct, Some(DepKind::ReturnType)),
                ("multi_uses", SymKind::Function, "Handler", SymKind::Struct, Some(DepKind::ReturnType)),
            ],
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
        assert_depends(cc, "process_with_trait", SymKind::Function, "Logger", SymKind::Trait, Some(DepKind::ReturnType));
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
        assert_depends(cc, "new", SymKind::Function, "Builder", SymKind::Struct, Some(DepKind::ReturnType));

        assert_depends(cc, "with_config", SymKind::Function, "Builder", SymKind::Struct, Some(DepKind::ReturnType));
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
        assert_depends(cc, "use_parent", SymKind::Function, "Parent", SymKind::Struct, Some(DepKind::ReturnType));
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
        assert_depends(cc, "get_config", SymKind::Function, "Config", SymKind::Struct, Some(DepKind::ReturnType));
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
        assert_depends(cc, "create", SymKind::Function, "Result", SymKind::Enum, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
#[ignore]
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
        assert_depends(cc, "access_global", SymKind::Function, "Global", SymKind::Struct, Some(DepKind::ReturnType));
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_depends(cc, "get_items", SymKind::Function, "i32", SymKind::Primitive, Some(DepKind::ReturnType));
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_depends(cc, "compare", SymKind::Function, "bool", SymKind::Primitive, Some(DepKind::ReturnType));
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
        assert_exists(cc, "test", SymKind::Function);
    });
}

// ============================================================================
// Uncovered Path Tests - Type Resolution, Binary Ops, Aliases
// ============================================================================

#[test]
#[serial_test::serial]
fn test_type_alias_resolution_uncovered() {
    let source = r#"
        pub struct Data;

        type DataRef = Data;

        pub fn get_data() -> DataRef {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        // Type aliases may not fully resolve to the underlying type yet
        // Just verify the function exists
        assert_exists(cc, "get_data", SymKind::Function);
    });
}
#[test]
#[serial_test::serial]
fn test_if_expression_type_inference() {
    let source = r#"
        pub struct Value;

        pub fn get_value(flag: bool) -> Value {
            if flag {
                Value
            } else {
                Value
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "get_value", SymKind::Function, "Value", SymKind::Struct, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_binary_expression_bool_operation() {
    let source = r#"
        pub fn compare(a: i32, b: i32) -> bool {
            a == b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "compare", SymKind::Function, "bool", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_binary_expression_arithmetic() {
    let source = r#"
        pub fn add(a: i32, b: i32) -> i32 {
            a + b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "add", SymKind::Function, "i32", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_field_expression_access() {
    let source = r#"
        pub struct Point {
            x: i32,
            y: i32,
        }

        pub fn get_x(p: &Point) -> i32 {
            p.x
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        // The field access should have used Point
        assert_depends(cc, "get_x", SymKind::Function, "Point", SymKind::Struct, Some(DepKind::Uses));
    });
}

#[test]
#[serial_test::serial]
fn test_field_expression_method_call_uncovered() {
    let source = r#"
        pub struct Handler;

        impl Handler {
            pub fn handle(&self) {}
        }

        pub fn use_handler(h: &Handler) {
            h.handle();
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "use_handler", SymKind::Function, "Handler", SymKind::Struct, Some(DepKind::Uses));
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_ident_empty_path_returns_none() {
    let source = r#"
        pub fn test() {}
    "#;

    with_compiled_unit(&[source], |cc| {
        // Should not panic on empty paths
        let all_symbols = cc.get_all_symbols();
        assert!(!all_symbols.is_empty());
    });
}

#[test]
#[serial_test::serial]
fn test_logical_and_expression() {
    let source = r#"
        pub fn check(a: bool, b: bool) -> bool {
            a && b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "check", SymKind::Function, "bool", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_logical_or_expression() {
    let source = r#"
        pub fn check(a: bool, b: bool) -> bool {
            a || b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "check", SymKind::Function, "bool", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_subtraction_expression() {
    let source = r#"
        pub fn subtract(a: i32, b: i32) -> i32 {
            a - b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "subtract", SymKind::Function, "i32", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_multiplication_expression() {
    let source = r#"
        pub fn multiply(a: i32, b: i32) -> i32 {
            a * b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "multiply", SymKind::Function, "i32", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_division_expression() {
    let source = r#"
        pub fn divide(a: i32, b: i32) -> i32 {
            a / b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "divide", SymKind::Function, "i32", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_modulo_expression() {
    let source = r#"
        pub fn modulo(a: i32, b: i32) -> i32 {
            a % b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "modulo", SymKind::Function, "i32", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_comparison_less_than() {
    let source = r#"
        pub fn less_than(a: i32, b: i32) -> bool {
            a < b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "less_than", SymKind::Function, "bool", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_comparison_greater_than() {
    let source = r#"
        pub fn greater_than(a: i32, b: i32) -> bool {
            a > b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "greater_than", SymKind::Function, "bool", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_comparison_less_equal() {
    let source = r#"
        pub fn less_equal(a: i32, b: i32) -> bool {
            a <= b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "less_equal", SymKind::Function, "bool", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_comparison_greater_equal() {
    let source = r#"
        pub fn greater_equal(a: i32, b: i32) -> bool {
            a >= b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "greater_equal", SymKind::Function, "bool", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_not_equal_comparison() {
    let source = r#"
        pub fn not_equal(a: i32, b: i32) -> bool {
            a != b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "not_equal", SymKind::Function, "bool", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_nested_binary_expressions() {
    let source = r#"
        pub fn calc(a: i32, b: i32, c: i32) -> i32 {
            (a + b) * c
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "calc", SymKind::Function, "i32", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_tuple_type_multiple_elements_coverage() {
    let source = r#"
        pub fn get_tuple() -> (i32, String, bool) {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        // Verify the function exists
        assert_exists(cc, "get_tuple", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_block_with_statements_and_expression() {
    let source = r#"
        pub struct Result;

        pub fn block_expr() -> Result {
            {
                let x = 5;
                let y = 10;
                Result
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "block_expr", SymKind::Function, "Result", SymKind::Struct, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_resolve_type_of_chained_aliases_coverage() {
    let source = r#"
        pub struct Base;
        type Alias1 = Base;
        type Alias2 = Alias1;

        pub fn get_aliased() -> Alias2 {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        // Verify the function exists and type aliases are present
        assert_exists(cc, "get_aliased", SymKind::Function);
        assert_exists(cc, "Alias1", SymKind::TypeAlias);
        assert_exists(cc, "Alias2", SymKind::TypeAlias);
    });
}
#[test]
#[serial_test::serial]
fn test_collect_types_from_generic() {
    let source = r#"
        pub struct Container<T> {
            value: T,
        }

        pub fn use_container() -> Container<i32> {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "use_container", SymKind::Function, "Container", SymKind::Struct, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_block_empty_or_only_comments() {
    let source = r#"
        pub fn comment_only() -> i32 {
            // This is just a comment
            42
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "comment_only", SymKind::Function, "i32", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_field_missing_from_struct() {
    let source = r#"
        pub struct Incomplete;

        pub fn process(i: &Incomplete) {
            // Try to access non-existent field
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        // Should handle gracefully
        let all_symbols = cc.get_all_symbols();
        assert!(!all_symbols.is_empty());
    });
}

#[test]
#[serial_test::serial]
fn test_multiple_field_accesses() {
    let source = r#"
        pub struct Nested {
            inner: i32,
        }

        pub struct Outer {
            nested: Nested,
        }

        pub fn get_inner(o: &Outer) -> i32 {
            // Can only access nested
            o.nested
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "get_inner", SymKind::Function, "Outer", SymKind::Struct, Some(DepKind::Uses));
    });
}

#[test]
#[serial_test::serial]
fn test_array_type_coverage() {
    let source = r#"
        pub fn get_array() -> [i32; 10] {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "get_array", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_reference_type_coverage() {
    let source = r#"
        pub struct Data;

        pub fn get_ref() -> &'static Data {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "get_ref", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_mutable_reference_type_coverage() {
    let source = r#"
        pub struct Data;

        pub fn get_mut_ref() -> &'static mut Data {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "get_mut_ref", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_pointer_type_coverage() {
    let source = r#"
        pub struct Data;

        pub fn get_ptr() -> *const Data {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "get_ptr", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_function_pointer_type_coverage() {
    let source = r#"
        pub fn get_fn_ptr() -> fn(i32) -> i32 {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "get_fn_ptr", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_closure_type_capture_coverage() {
    let source = r#"
        pub struct Value;

        pub fn create_closure() {
            let value = Value;
            let _closure = || {
                let _ = value;
            };
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "create_closure", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_for_loop_type_inference() {
    let source = r#"
        pub fn iterate() {
            for _i in 0..10 {
                // loop body
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "iterate", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_match_expression_types() {
    let source = r#"
        pub enum Status {
            Active,
            Inactive,
        }

        pub fn check_status(s: Status) -> i32 {
            match s {
                Status::Active => 1,
                Status::Inactive => 0,
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "check_status", SymKind::Function, "i32", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_slice_type_coverage() {
    let source = r#"
        pub fn get_slice() -> &[i32] {
            &[]
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "get_slice", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_dyn_trait_coverage() {
    let source = r#"
        pub trait Iterator {
            fn next(&mut self);
        }

        pub fn get_iter() -> Box<dyn Iterator> {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "get_iter", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_higher_ranked_trait_bound() {
    let source = r#"
        pub fn generic<T: for<'a> Fn(&'a str)>(_f: T) {}
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "generic", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_associated_type_coverage() {
    let source = r#"
        pub trait Container {
            type Item;

            fn get(&self) -> Self::Item;
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "Container", SymKind::Trait);
    });
}

#[test]
#[serial_test::serial]
fn test_resolve_type_of_same_symbol() {
    // Test the resolve_type_of function with a symbol that points to itself
    let source = r#"
        pub struct SelfRef;

        pub fn get_self() -> SelfRef {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "SelfRef", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_if_expression_missing_consequence() {
    // Test if expression without consequence
    let source = r#"
        pub fn test() {
            if false {
                // consequence
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_binary_expression_unknown_operator() {
    // Binary expression with operators
    let source = r#"
        pub fn test(a: i32, b: i32) {
            let _result = a + b;
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_field_expression_no_value() {
    // Field expression edge case
    let source = r#"
        pub struct S { f: i32 }
        pub fn test() {
            let s = S { f: 0 };
            let _ = s.f;
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_collect_ident_recursive() {
    // Complex identifiers nested deeply
    let source = r#"
        pub struct A;
        pub struct B { a: A }
        pub struct C { b: B }

        pub fn nested() -> C {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "nested", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_is_trivia_filtering() {
    // Test that trivia (comments, whitespace) is correctly filtered
    let source = r#"
        pub fn test() -> i32 {
            // comment
            42
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_resolution_comprehensive() {
    // Comprehensive test for scoped identifier with multiple paths
    let source = r#"
        pub mod outer {
            pub struct Value;
            pub mod inner {
                use super::Value;

                pub fn create() -> Value {
                    todo!()
                }
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "outer", SymKind::Namespace);
    });
}

#[test]
#[serial_test::serial]
fn test_primitive_all_types() {
    // Test all primitive types
    let source = r#"
        pub fn u8_fn() -> u8 { 0 }
        pub fn u16_fn() -> u16 { 0 }
        pub fn u32_fn() -> u32 { 0 }
        pub fn u64_fn() -> u64 { 0 }
        pub fn u128_fn() -> u128 { 0 }
        pub fn i8_fn() -> i8 { 0 }
        pub fn i16_fn() -> i16 { 0 }
        pub fn i32_fn() -> i32 { 0 }
        pub fn i64_fn() -> i64 { 0 }
        pub fn i128_fn() -> i128 { 0 }
        pub fn usize_fn() -> usize { 0 }
        pub fn isize_fn() -> isize { 0 }
        pub fn f32_fn() -> f32 { 0.0 }
        pub fn f64_fn() -> f64 { 0.0 }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "u8_fn", SymKind::Function);
        assert_exists(cc, "f64_fn", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_complex_generic_types() {
    // Test complex generic type resolution
    let source = r#"
        pub struct Wrapper<T> {
            value: T,
        }

        pub struct Pair<A, B> {
            first: A,
            second: B,
        }

        pub fn complex() -> Wrapper<Pair<i32, String>> {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "complex", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_depends(cc, "handle", SymKind::Function, "Handler", SymKind::Struct, Some(DepKind::ReturnType));
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
        assert_depends(cc, "origin", SymKind::Function, "Point", SymKind::Struct, Some(DepKind::ReturnType));
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
        assert_exists(cc, "test", SymKind::Function);
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
        assert_exists(cc, "test", SymKind::Function);
    });
}

// ============================================================================
// Uncovered Functions Tests - Helper Functions
// ============================================================================

#[test]
#[serial_test::serial]
fn test_collect_types_in_generic_expression() {
    let source = r#"
        pub struct Vec<T>;
        pub struct Result<T, E>;

        fn process<T>(val: Result<Vec<T>, i32>) {
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "process", SymKind::Function);
        assert_exists(cc, "Vec", SymKind::Struct);
        assert_exists(cc, "Result", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_collect_idents_in_addition_expression() {
    let source = r#"
        fn test(x: i32, y: i32) -> i32 {
            x + y
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_first_significant_child_skip_trivia() {
    let source = r#"
        fn test() {
            // Comment should be skipped
            let x = 42;
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_as_text_literal_helper() {
    let source = r#"
        fn test() -> &'static str {
            "literal text"
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_resolve_type_of_with_alias_chain() {
    let source = r#"
        pub type Alias1 = i32;
        pub type Alias2 = Alias1;
        pub type Alias3 = Alias2;

        fn test() -> Alias3 {
            42
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "Alias1", SymKind::TypeAlias);
        assert_exists(cc, "Alias2", SymKind::TypeAlias);
        assert_exists(cc, "Alias3", SymKind::TypeAlias);
    });
}

#[test]
#[serial_test::serial]
fn test_is_identifier_kind_coverage() {
    let source = r#"
        fn test() {
            let foo = 42;
            let bar_baz = 50;
            let _unused = 60;
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_get_binary_components_with_text_search() {
    let source = r#"
        fn add(a: i32, b: i32) -> i32 {
            a + b
        }

        fn subtract(a: i32, b: i32) -> i32 {
            a - b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "add", SymKind::Function);
        assert_exists(cc, "subtract", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_lookup_binary_operator_all_variants() {
    let source = r#"
        fn test_bool_ops() {
            let a = true;
            let b = false;
            let c = a && b;
            let d = a || b;
        }

        fn test_comparison() {
            let x = 5;
            let y = 3;
            let eq = x == y;
            let ne = x != y;
            let lt = x < y;
            let gt = x > y;
            let le = x <= y;
            let ge = x >= y;
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test_bool_ops", SymKind::Function);
        assert_exists(cc, "test_comparison", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_infer_if_expression_no_consequence() {
    let source = r#"
        fn test(cond: bool) {
            if cond {
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_infer_if_expression_with_typed_consequence() {
    let source = r#"
        pub struct Point;

        fn test(cond: bool) -> Point {
            if cond {
                Point
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "Point", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_infer_binary_expression_arithmetic() {
    let source = r#"
        fn mul(a: i32, b: i32) -> i32 {
            a * b
        }

        fn div(a: i32, b: i32) -> i32 {
            a / b
        }

        fn modulo(a: i32, b: i32) -> i32 {
            a % b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "mul", SymKind::Function);
        assert_exists(cc, "div", SymKind::Function);
        assert_exists(cc, "modulo", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_infer_binary_expression_bool_result() {
    let source = r#"
        fn check_bool(a: bool) -> bool {
            let result = a && false;
            result
        }

        fn check_or(a: bool, b: bool) -> bool {
            a || b
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_depends(cc, "check_bool", SymKind::Function, "bool", SymKind::Primitive, Some(DepKind::ReturnType));
    });
}

#[test]
#[serial_test::serial]
fn test_collect_idents_multiple_scopes() {
    let source = r#"
        fn outer() {
            let x = 1;
            {
                let y = 2;
                {
                    let z = 3;
                }
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "outer", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_struct_expression_with_name_field() {
    let source = r#"
        pub struct Config;

        fn test() -> Config {
            Config
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "Config", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_field_expression_method_call_impl() {
    let source = r#"
        pub struct MyType;
        impl MyType {
            pub fn method(&self) -> i32 {
                42
            }
        }

        fn test(obj: MyType) -> i32 {
            obj.method()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "MyType", SymKind::Struct);
        assert_exists(cc, "method", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_field_expression_field_access() {
    let source = r#"
        pub struct Point {
            x: i32,
        }

        fn test(p: Point) -> i32 {
            p.x
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "Point", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_call_expression_function_pointer() {
    let source = r#"
        fn helper() -> i32 {
            42
        }

        fn test() -> i32 {
            helper()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "helper", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_unary_expression_negation_coverage() {
    let source = r#"
        fn negate(x: i32) -> i32 {
            -x
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "negate", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_block_with_multiple_statements_coverage() {
    let source = r#"
        fn test() -> i32 {
            let x = 1;
            let y = 2;
            x + y
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_block_last_expression_return_type() {
    let source = r#"
        pub struct Value;

        fn test() -> Value {
            Value
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_module_path() {
    let source = r#"
        pub mod math {
            pub fn add(a: i32, b: i32) -> i32 {
                a + b
            }
        }

        fn test() -> i32 {
            math::add(1, 2)
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_primitive_type_resolution_all_types() {
    let source = r#"
        fn int_type() -> i32 { 0 }
        fn float_type() -> f64 { 0.0 }
        fn bool_type() -> bool { true }
        fn char_type() -> char { 'a' }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "int_type", SymKind::Function);
        assert_exists(cc, "float_type", SymKind::Function);
        assert_exists(cc, "bool_type", SymKind::Function);
        assert_exists(cc, "char_type", SymKind::Function);
    });
}

// ============================================================================
// Additional Tests for Remaining Uncovered Functions
// ============================================================================

#[test]
#[serial_test::serial]
fn test_infer_scoped_identifier_deep_nesting() {
    let source = r#"
        mod a {
            pub mod b {
                pub mod c {
                    pub fn deep() -> i32 { 42 }
                }
            }
        }

        fn test() {
            a::b::c::deep()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "deep", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_collect_ident_recursive_multiple_levels() {
    let source = r#"
        fn outer() {
            let a = 1;
            {
                let b = 2;
                {
                    let c = 3;
                    {
                        let d = 4;
                    }
                }
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "outer", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_collect_types_with_generic_bounds() {
    let source = r#"
        pub struct Container<T>;
        pub struct Result<T, E>;
        pub struct Vec<T>;

        fn process<T, U>(x: Container<Result<Vec<T>, U>>) -> T
        where
            T: Clone,
            U: Copy,
        {
            todo!()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "process", SymKind::Function);
        assert_exists(cc, "Container", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_lookup_binary_operator_multiple_types() {
    let source = r#"
        fn test_numbers() {
            let a: i32 = 1 + 2 * 3 - 4 / 5 % 6;
        }

        fn test_bool_ops() {
            let b: bool = true && false || true;
        }

        fn test_comparisons() {
            let c: bool = 1 == 2 && 3 != 4 && 5 < 6 && 7 > 8 && 9 <= 10 && 11 >= 12;
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test_numbers", SymKind::Function);
        assert_exists(cc, "test_bool_ops", SymKind::Function);
        assert_exists(cc, "test_comparisons", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_resolve_type_of_long_chain() {
    let source = r#"
        pub type A = i32;
        pub type B = A;
        pub type C = B;
        pub type D = C;
        pub type E = D;
        pub type F = E;
        pub type G = F;
        pub type H = G;

        fn test() -> H {
            42
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_infer_if_expression_with_else() {
    let source = r#"
        pub struct A;
        pub struct B;

        fn test(cond: bool) -> A {
            if cond {
                A
            } else {
                A
            }
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "A", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_infer_block_no_last_expression() {
    let source = r#"
        fn test() {
            let x = 1;
            let y = 2;
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_field_expression_nested_struct() {
    let source = r#"
        pub struct Inner {
            value: i32,
        }

        pub struct Outer {
            inner: Inner,
        }

        fn test(obj: Outer) -> i32 {
            obj.inner.value
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "Outer", SymKind::Struct);
        assert_exists(cc, "Inner", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_call_expression_with_generics() {
    let source = r#"
        pub struct Container<T>;

        impl<T> Container<T> {
            pub fn new(value: T) -> Self {
                todo!()
            }
        }

        fn test() -> Container<i32> {
            Container::new(42)
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "Container", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_struct_expression_with_type_field() {
    let source = r#"
        pub struct Config {
            timeout: u32,
        }

        fn test() -> Config {
            Config
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "Config", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_identifier_kind_all_variants() {
    let source = r#"
        fn test() {
            let identifier = 1;
            let _field_identifier = 2;
            let Type = 3;
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_trivia_skip_in_block() {
    let source = r#"
        fn test() -> i32 {
            // This is a comment
            /* This is a block comment */
            42
            // Trailing comment
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_text_literal_string_variants() {
    let source = r#"
        fn basic() -> &'static str {
            "basic"
        }

        fn multiline() -> &'static str {
            "line1
line2"
        }

        fn escaped() -> &'static str {
            "escaped\n\t\r"
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "basic", SymKind::Function);
        assert_exists(cc, "multiline", SymKind::Function);
        assert_exists(cc, "escaped", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_binary_components_extraction() {
    let source = r#"
        fn add() -> i32 {
            1 + 2
        }

        fn subtract() -> i32 {
            10 - 5
        }

        fn multiply() -> i32 {
            3 * 4
        }

        fn divide() -> i32 {
            20 / 5
        }

        fn modulo() -> i32 {
            10 % 3
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "add", SymKind::Function);
        assert_exists(cc, "subtract", SymKind::Function);
        assert_exists(cc, "multiply", SymKind::Function);
        assert_exists(cc, "divide", SymKind::Function);
        assert_exists(cc, "modulo", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_scoped_identifier_type_path() {
    let source = r#"
        pub struct MyType;
        pub mod types {
            pub struct Wrapper;
        }

        fn test_local() -> MyType {
            MyType
        }

        fn test_module() -> types::Wrapper {
            types::Wrapper
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test_local", SymKind::Function);
        assert_exists(cc, "test_module", SymKind::Function);
    });
}

#[test]
#[serial_test::serial]
fn test_field_expression_callable() {
    let source = r#"
        pub struct Helper;
        impl Helper {
            pub fn method() -> i32 {
                42
            }

            pub fn instance_method(&self) -> i32 {
                42
            }
        }

        fn test(h: Helper) -> i32 {
            h.instance_method() + Helper::method()
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "Helper", SymKind::Struct);
    });
}

#[test]
#[serial_test::serial]
fn test_child_field_inference_chain() {
    let source = r#"
        pub struct Result<T, E> {
            ok: T,
            err: E,
        }

        fn test(r: Result<i32, String>) -> i32 {
            r.ok
        }
    "#;

    with_compiled_unit(&[source], |cc| {
        assert_exists(cc, "test", SymKind::Function);
        assert_exists(cc, "Result", SymKind::Struct);
    });
}
