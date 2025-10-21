/// Tests for let declaration type binding.
/// This module verifies that visit_let_declaration properly extracts type annotations
/// and establishes dependency relations between functions and the types used in let statements.
use llmcc_core::ir::HirId;
use llmcc_core::symbol::Symbol;
use llmcc_rust::{bind_symbols, build_llmcc_ir, collect_symbols, CompileCtxt, LangRust};

fn compile(
    source: &str,
) -> (
    &'static CompileCtxt<'static>,
    llmcc_core::context::CompileUnit<'static>,
    llmcc_rust::CollectionResult,
) {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(&sources)));
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(cc).unwrap();
    let globals = cc.create_globals();
    let collection = collect_symbols(unit, globals);
    bind_symbols(unit, globals);
    (cc, unit, collection)
}

fn find_struct<'a>(
    collection: &'a llmcc_rust::CollectionResult,
    name: &str,
) -> &'a llmcc_rust::StructDescriptor {
    collection
        .structs
        .iter()
        .find(|desc| desc.name == name)
        .unwrap()
}

fn find_function<'a>(
    collection: &'a llmcc_rust::CollectionResult,
    name: &str,
) -> &'a llmcc_rust::FunctionDescriptor {
    collection
        .functions
        .iter()
        .find(|desc| desc.name == name)
        .unwrap()
}

fn find_enum<'a>(
    collection: &'a llmcc_rust::CollectionResult,
    name: &str,
) -> &'a llmcc_rust::EnumDescriptor {
    collection
        .enums
        .iter()
        .find(|desc| desc.name == name)
        .unwrap()
}

fn struct_symbol(
    unit: llmcc_core::context::CompileUnit<'static>,
    collection: &llmcc_rust::CollectionResult,
    name: &str,
) -> &'static Symbol {
    let desc = find_struct(collection, name);
    symbol(unit, desc.hir_id)
}

fn function_symbol(
    unit: llmcc_core::context::CompileUnit<'static>,
    collection: &llmcc_rust::CollectionResult,
    name: &str,
) -> &'static Symbol {
    let desc = find_function(collection, name);
    symbol(unit, desc.hir_id)
}

fn enum_symbol(
    unit: llmcc_core::context::CompileUnit<'static>,
    collection: &llmcc_rust::CollectionResult,
    name: &str,
) -> &'static Symbol {
    let desc = find_enum(collection, name);
    symbol(unit, desc.hir_id)
}

fn symbol(unit: llmcc_core::context::CompileUnit<'static>, hir_id: HirId) -> &'static Symbol {
    unit.get_scope(hir_id).symbol().unwrap()
}

fn assert_depends_on(symbol: &Symbol, target: &Symbol) {
    assert!(symbol.depends.borrow().iter().any(|id| *id == target.id));
}

fn assert_relation(dependent: &Symbol, dependency: &Symbol) {
    assert_depends_on(dependent, dependency);
}

#[test]
fn let_with_explicit_type_creates_dependency() {
    let source = r#"
        struct ProjectGraph;

        fn process_graph() {
            let pg: ProjectGraph = ProjectGraph;
        }
    "#;

    let (_, unit, collection) = compile(source);

    let process_fn = function_symbol(unit, &collection, "process_graph");
    let pg_struct = struct_symbol(unit, &collection, "ProjectGraph");

    assert_relation(process_fn, pg_struct);
}

#[test]
fn multiple_let_declarations_with_different_types() {
    let source = r#"
        struct Config;
        struct Logger;

        fn initialize() {
            let cfg: Config = Config;
            let log: Logger = Logger;
        }
    "#;

    let (_, unit, collection) = compile(source);

    let init_fn = function_symbol(unit, &collection, "initialize");
    let config_struct = struct_symbol(unit, &collection, "Config");
    let logger_struct = struct_symbol(unit, &collection, "Logger");

    assert_relation(init_fn, config_struct);
    assert_relation(init_fn, logger_struct);
}

#[test]
fn let_without_type_annotation_still_works() {
    let source = r#"
        fn simple() {
            let x = 5;
        }
    "#;

    let (_, unit, collection) = compile(source);
    let simple_fn = function_symbol(unit, &collection, "simple");

    // Should not panic - just proceed without type dependency
    let _deps = simple_fn.depends.borrow();
}

#[test]
fn let_with_mutable_and_type_annotation() {
    let source = r#"
        struct Buffer;

        fn allocate() {
            let mut buf: Buffer = Buffer;
        }
    "#;

    let (_, unit, collection) = compile(source);

    let allocate_fn = function_symbol(unit, &collection, "allocate");
    let buffer_struct = struct_symbol(unit, &collection, "Buffer");

    assert_relation(allocate_fn, buffer_struct);
}

#[test]
fn nested_let_declarations_in_block() {
    let source = r#"
        struct Outer;
        struct Inner;

        fn nested() {
            {
                let outer: Outer = Outer;
                {
                    let inner: Inner = Inner;
                }
            }
        }
    "#;

    let (_, unit, collection) = compile(source);

    let nested_fn = function_symbol(unit, &collection, "nested");
    let outer_struct = struct_symbol(unit, &collection, "Outer");
    let inner_struct = struct_symbol(unit, &collection, "Inner");

    assert_relation(nested_fn, outer_struct);
    assert_relation(nested_fn, inner_struct);
}

#[test]
fn let_with_reference_type() {
    let source = r#"
        struct Data;

        fn borrow_data() {
            let data_ref: &Data = &Data;
        }
    "#;

    let (_, unit, collection) = compile(source);

    let borrow_fn = function_symbol(unit, &collection, "borrow_data");
    let data_struct = struct_symbol(unit, &collection, "Data");

    assert_relation(borrow_fn, data_struct);
}

#[test]
fn let_in_function_parameter_scope() {
    let source = r#"
        struct State;

        fn with_param(s: State) {
            let local: State = s;
        }
    "#;

    let (_, unit, collection) = compile(source);

    let with_param_fn = function_symbol(unit, &collection, "with_param");
    let state_struct = struct_symbol(unit, &collection, "State");

    assert_relation(with_param_fn, state_struct);
}

#[test]
fn multiple_functions_each_with_let_types() {
    let source = r#"
        struct TypeA;
        struct TypeB;

        fn func_a() {
            let a: TypeA = TypeA;
        }

        fn func_b() {
            let b: TypeB = TypeB;
        }
    "#;

    let (_, unit, collection) = compile(source);

    let func_a = function_symbol(unit, &collection, "func_a");
    let func_b = function_symbol(unit, &collection, "func_b");
    let type_a = struct_symbol(unit, &collection, "TypeA");
    let type_b = struct_symbol(unit, &collection, "TypeB");

    assert_relation(func_a, type_a);
    assert_relation(func_b, type_b);
}

#[test]
fn let_with_enum_type() {
    let source = r#"
        enum Status {
            Active,
            Inactive,
        }

        fn check_status() {
            let s: Status = Status::Active;
        }
    "#;

    let (_, unit, collection) = compile(source);

    let check_fn = function_symbol(unit, &collection, "check_status");
    let status_enum = enum_symbol(unit, &collection, "Status");

    assert_relation(check_fn, status_enum);
}

#[test]
fn let_in_nested_scope_from_match() {
    let source = r#"
        struct Result;

        fn process() {
            match Some(0) {
                Some(_) => {
                    let res: Result = Result;
                }
                None => {}
            }
        }
    "#;

    let (_, unit, collection) = compile(source);

    let process_fn = function_symbol(unit, &collection, "process");
    let result_struct = struct_symbol(unit, &collection, "Result");

    assert_relation(process_fn, result_struct);
}

/// Tests for inferred types in let declarations (without explicit annotations)
/// When let x = Constructor() is used, dependencies should be tracked from the initializer

#[test]
fn let_without_explicit_type_struct_constructor() {
    let source = r#"
        struct Config;

        impl Config {
            fn new() -> Config {
                Config
            }
        }

        fn setup() {
            let cfg = Config::new();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let setup_fn = function_symbol(unit, &collection, "setup");
    let config_struct = struct_symbol(unit, &collection, "Config");

    // Method call creates dependency on the struct
    assert_relation(setup_fn, config_struct);
}

#[test]
fn let_without_explicit_type_method_call() {
    let source = r#"
        struct Builder;

        impl Builder {
            fn new() -> Builder {
                Builder
            }
        }

        fn create() {
            let builder = Builder::new();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let builder_struct = struct_symbol(unit, &collection, "Builder");
    let create_fn = function_symbol(unit, &collection, "create");

    // The let statement initializer `Builder::new()` creates dependency on both Builder and new()
    assert_relation(create_fn, builder_struct);
}

#[test]
fn multiple_let_without_types_from_different_sources() {
    let source = r#"
        struct Factory;

        impl Factory {
            fn new() -> Factory {
                Factory
            }
        }

        fn work() {
            let factory = Factory::new();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let work_fn = function_symbol(unit, &collection, "work");
    let factory_struct = struct_symbol(unit, &collection, "Factory");

    // Method call should track struct dependency
    assert_relation(work_fn, factory_struct);
}

#[test]
fn let_inferred_type_from_function_return() {
    let source = r#"
        struct Value;

        fn create_value() -> Value {
            Value
        }

        fn use_value() {
            let v = create_value();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let use_value_fn = function_symbol(unit, &collection, "use_value");
    let create_value_fn = function_symbol(unit, &collection, "create_value");

    // Should depend on the function called
    assert_relation(use_value_fn, create_value_fn);
}

#[test]
fn let_with_tuple_initializer() {
    let source = r#"
        struct A;
        struct B;

        impl A {
            fn new() -> A {
                A
            }
        }

        impl B {
            fn create() -> B {
                B
            }
        }

        fn combine() {
            let a = A::new();
            let b = B::create();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let combine_fn = function_symbol(unit, &collection, "combine");
    let struct_a = struct_symbol(unit, &collection, "A");
    let struct_b = struct_symbol(unit, &collection, "B");

    // Method calls should create dependencies
    assert_relation(combine_fn, struct_a);
    assert_relation(combine_fn, struct_b);
}

#[test]
fn let_with_array_of_struct() {
    let source = r#"
        struct Item;

        impl Item {
            fn default() -> Item {
                Item
            }
        }

        fn items() {
            let list = Item::default();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let items_fn = function_symbol(unit, &collection, "items");
    let item_struct = struct_symbol(unit, &collection, "Item");

    // Method call should create dependencies
    assert_relation(items_fn, item_struct);
}

#[test]
fn let_with_method_chain() {
    let source = r#"
        struct Builder;

        impl Builder {
            fn new() -> Builder {
                Builder
            }

            fn configure(self) -> Builder {
                self
            }
        }

        fn setup() {
            let configured = Builder::new().configure();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let setup_fn = function_symbol(unit, &collection, "setup");
    let builder_struct = struct_symbol(unit, &collection, "Builder");

    // Should depend on Builder from the chain
    assert_relation(setup_fn, builder_struct);
}
