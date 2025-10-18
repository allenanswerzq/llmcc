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
    build_llmcc_ir::<LangRust>(unit).unwrap();
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

fn find_function_by_fqn<'a>(
    collection: &'a llmcc_rust::CollectionResult,
    fqn: &str,
) -> &'a llmcc_rust::FunctionDescriptor {
    collection
        .functions
        .iter()
        .find(|desc| desc.fqn == fqn)
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

fn function_symbol_by_fqn(
    unit: llmcc_core::context::CompileUnit<'static>,
    collection: &llmcc_rust::CollectionResult,
    fqn: &str,
) -> &'static Symbol {
    let desc = find_function_by_fqn(collection, fqn);
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

fn assert_no_relation(symbol: &Symbol, target: &Symbol) {
    assert!(
        !symbol.depends.borrow().iter().any(|id| *id == target.id),
        "unexpected dependency on {}",
        target.name.as_str()
    );
    assert!(
        !target.depended.borrow().iter().any(|id| *id == symbol.id),
        "unexpected reverse dependency from {}",
        target.name.as_str()
    );
}

fn assert_depended_by(symbol: &Symbol, source: &Symbol) {
    assert!(symbol.depended.borrow().iter().any(|id| *id == source.id));
}

fn assert_relation(dependent: &Symbol, dependency: &Symbol) {
    assert_depends_on(dependent, dependency);
    assert_depended_by(dependency, dependent);
}

#[test]
fn type_records_dependencies_on_methods() {
    let source = r#"
        struct Foo;

        impl Foo {
            fn method(&self) {}
        }
    "#;

    let (_, unit, collection) = compile(source);

    let foo_symbol = struct_symbol(unit, &collection, "Foo");
    let method_symbol = function_symbol(unit, &collection, "method");

    assert_relation(foo_symbol, method_symbol);
}

#[test]
fn method_depends_on_inherent_method() {
    let source = r#"
        struct Foo;

        impl Foo {
            fn helper(&self) {}

            fn caller(&self) {
                self.helper();
            }
        }
    "#;

    let (_, unit, collection) = compile(source);

    let helper_symbol = function_symbol(unit, &collection, "helper");
    let caller_symbol = function_symbol(unit, &collection, "caller");

    assert_relation(caller_symbol, helper_symbol);
}

#[test]
fn function_depends_on_called_function() {
    let source = r#"
        fn helper() {}

        fn caller() {
            helper();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let helper_symbol = function_symbol(unit, &collection, "helper");
    let caller_symbol = function_symbol(unit, &collection, "caller");

    assert_relation(caller_symbol, helper_symbol);
}

#[test]
fn function_depends_on_argument_type() {
    let source = r#"
        struct Foo;

        fn takes(_: Foo) {}
    "#;

    let (_, unit, collection) = compile(source);

    let foo_symbol = struct_symbol(unit, &collection, "Foo");
    let takes_symbol = function_symbol(unit, &collection, "takes");

    assert_relation(takes_symbol, foo_symbol);
}

#[test]
fn const_initializer_records_dependencies() {
    let source = r#"
        fn helper() -> i32 { 5 }

        const VALUE: i32 = helper();
    "#;

    let (_, unit, collection) = compile(source);

    let const_desc = collection
        .variables
        .iter()
        .find(|desc| desc.name == "VALUE")
        .unwrap();

    let helper_symbol = function_symbol(unit, &collection, "helper");
    let const_symbol = symbol(unit, const_desc.hir_id);

    assert_relation(const_symbol, helper_symbol);
}

#[test]
fn function_depends_on_return_type() {
    let source = r#"
        struct Bar;

        fn returns() -> Bar {
            Bar
        }
    "#;

    let (_, unit, collection) = compile(source);

    let bar_symbol = struct_symbol(unit, &collection, "Bar");
    let returns_symbol = function_symbol(unit, &collection, "returns");

    assert_relation(returns_symbol, bar_symbol);
}

#[test]
fn function_call_resolves_when_struct_shares_name() {
    let source = r#"
        struct Shared;

        #[allow(non_snake_case)]
        fn Shared() {}

        fn caller() {
            Shared();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let shared_struct_symbol = struct_symbol(unit, &collection, "Shared");
    let shared_fn_symbol = function_symbol_by_fqn(unit, &collection, "Shared");
    let caller_symbol = function_symbol(unit, &collection, "caller");

    assert_relation(caller_symbol, shared_fn_symbol);
    assert_no_relation(caller_symbol, shared_struct_symbol);
}

#[test]
fn type_dependency_resolves_when_function_shares_name() {
    let source = r#"
        struct Shared;

        #[allow(non_snake_case)]
        fn Shared() {}

        fn consume(_: Shared) {}
    "#;

    let (_, unit, collection) = compile(source);

    let shared_struct_symbol = struct_symbol(unit, &collection, "Shared");
    let shared_fn_symbol = function_symbol_by_fqn(unit, &collection, "Shared");
    let consume_symbol = function_symbol(unit, &collection, "consume");

    assert_relation(consume_symbol, shared_struct_symbol);
    assert_no_relation(consume_symbol, shared_fn_symbol);
}

#[test]
fn method_call_prefers_inherent_method_with_same_name_as_function() {
    let source = r#"
        struct Processor;

        impl Processor {
            fn process(&self) {}

            fn trigger(&self) {
                self.process();
            }
        }

        fn process() {}
    "#;

    let (_, unit, collection) = compile(source);

    let processor_symbol = struct_symbol(unit, &collection, "Processor");
    let method_process_symbol = function_symbol_by_fqn(unit, &collection, "Processor::process");
    let trigger_symbol = function_symbol_by_fqn(unit, &collection, "Processor::trigger");
    let free_process_symbol = function_symbol_by_fqn(unit, &collection, "process");

    assert_relation(processor_symbol, method_process_symbol);
    assert_relation(trigger_symbol, method_process_symbol);
    assert_no_relation(trigger_symbol, free_process_symbol);
}

#[test]
fn struct_field_creates_dependency() {
    let source = r#"
        struct Inner;

        struct Outer {
            field: Inner,
        }
    "#;

    let (_, unit, collection) = compile(source);

    let inner_symbol = struct_symbol(unit, &collection, "Inner");
    let outer_symbol = struct_symbol(unit, &collection, "Outer");

    assert_relation(outer_symbol, inner_symbol);
}

#[test]
fn struct_field_depends_on_enum_type() {
    let source = r#"
        enum Status {
            Ready,
            Busy,
        }

        struct Holder {
            status: Status,
        }
    "#;

    let (_, unit, collection) = compile(source);

    let status_symbol = enum_symbol(unit, &collection, "Status");
    let holder_symbol = struct_symbol(unit, &collection, "Holder");

    assert_relation(holder_symbol, status_symbol);
}

#[test]
fn enum_variant_depends_on_struct_type() {
    let source = r#"
        struct Payload;

        enum Message {
            Empty,
            With(Payload),
        }
    "#;

    let (_, unit, collection) = compile(source);

    let payload_symbol = struct_symbol(unit, &collection, "Payload");
    let message_symbol = enum_symbol(unit, &collection, "Message");

    assert_relation(message_symbol, payload_symbol);
}

#[test]
fn match_expression_depends_on_enum_variants() {
    let source = r#"
        enum Event {
            Click,
            Key(char),
        }

        fn handle(event: Event) -> i32 {
            match event {
                Event::Click => 1,
                Event::Key(_) => 2,
            }
        }
    "#;

    let (_, unit, collection) = compile(source);

    let event_symbol = enum_symbol(unit, &collection, "Event");
    let handle_symbol = function_symbol(unit, &collection, "handle");

    assert_relation(handle_symbol, event_symbol);
}

#[test]
fn nested_match_expressions_depend_on_variants() {
    let source = r#"
        enum Action {
            Move { x: i32, y: i32 },
            Click,
        }

        fn handle(action: Action) -> i32 {
            match action {
                Action::Move { x, y } => match (x, y) {
                    (0, 0) => 0,
                    _ => 1,
                },
                Action::Click => 2,
            }
        }
    "#;

    let (_, unit, collection) = compile(source);

    let action_symbol = enum_symbol(unit, &collection, "Action");
    let handle_symbol = function_symbol(unit, &collection, "handle");

    assert_relation(handle_symbol, action_symbol);
}

#[test]
fn nested_struct_fields_create_chain() {
    let source = r#"
        struct A;

        struct B {
            a: A,
        }

        struct C {
            b: B,
        }
    "#;

    let (_, unit, collection) = compile(source);

    let a_symbol = struct_symbol(unit, &collection, "A");
    let b_symbol = struct_symbol(unit, &collection, "B");
    let c_symbol = struct_symbol(unit, &collection, "C");

    assert_relation(b_symbol, a_symbol);
    assert_relation(c_symbol, b_symbol);
}

#[test]
fn function_chain_dependencies() {
    let source = r#"
        fn level1() {}

        fn level2() {
            level1();
        }

        fn level3() {
            level2();
        }

        fn level4() {
            level3();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let l1_symbol = function_symbol(unit, &collection, "level1");
    let l2_symbol = function_symbol(unit, &collection, "level2");
    let l3_symbol = function_symbol(unit, &collection, "level3");
    let l4_symbol = function_symbol(unit, &collection, "level4");

    assert_relation(l2_symbol, l1_symbol);
    assert_relation(l3_symbol, l2_symbol);
    assert_relation(l4_symbol, l3_symbol);
}

#[test]
fn module_with_nested_types() {
    let source = r#"
        mod outer {
            pub struct OuterType;

            pub mod inner {
                pub struct InnerType;
            }
        }

        fn uses(_: outer::OuterType, _: outer::inner::InnerType) {}
    "#;

    let (_, unit, collection) = compile(source);

    let outer_type_symbol = struct_symbol(unit, &collection, "OuterType");
    let inner_type_symbol = struct_symbol(unit, &collection, "InnerType");
    let uses_symbol = function_symbol(unit, &collection, "uses");

    assert_relation(uses_symbol, outer_type_symbol);
    assert_relation(uses_symbol, inner_type_symbol);
}

#[test]
fn deeply_nested_modules() {
    let source = r#"
        mod level1 {
            pub mod level2 {
                pub mod level3 {
                    pub mod level4 {
                        pub struct DeepType;
                    }
                }
            }
        }

        fn access(_: level1::level2::level3::level4::DeepType) {}
    "#;

    let (_, unit, collection) = compile(source);

    let deep_type_symbol = struct_symbol(unit, &collection, "DeepType");
    let access_symbol = function_symbol(unit, &collection, "access");

    assert_relation(access_symbol, deep_type_symbol);
}

#[test]
fn module_functions_calling_each_other() {
    let source = r#"
        mod tools {
            pub fn helper1() {}

            pub fn helper2() {
                helper1();
            }
        }

        fn main() {
            tools::helper2();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let helper1_symbol = function_symbol(unit, &collection, "helper1");
    let helper2_symbol = function_symbol(unit, &collection, "helper2");
    let main_symbol = function_symbol(unit, &collection, "main");

    assert_relation(helper2_symbol, helper1_symbol);
    assert_relation(main_symbol, helper2_symbol);
}

#[test]
fn enum_with_multiple_variant_types() {
    let source = r#"
        struct TypeA;
        struct TypeB;
        struct TypeC;

        enum MultiVariant {
            VariantA(TypeA),
            VariantB(TypeB),
            VariantC(TypeC),
            Empty,
        }
    "#;

    let (_, unit, collection) = compile(source);

    let type_a_symbol = struct_symbol(unit, &collection, "TypeA");
    let type_b_symbol = struct_symbol(unit, &collection, "TypeB");
    let type_c_symbol = struct_symbol(unit, &collection, "TypeC");
    let enum_symbol = enum_symbol(unit, &collection, "MultiVariant");

    assert_relation(enum_symbol, type_a_symbol);
    assert_relation(enum_symbol, type_b_symbol);
    assert_relation(enum_symbol, type_c_symbol);
}

#[test]
fn enum_with_struct_variants() {
    let source = r#"
        struct Inner;

        enum Result {
            Ok { value: Inner },
            Err { message: String },
        }
    "#;

    let (_, unit, collection) = compile(source);

    let inner_symbol = struct_symbol(unit, &collection, "Inner");
    let result_symbol = enum_symbol(unit, &collection, "Result");

    assert_relation(result_symbol, inner_symbol);
}

#[test]
fn nested_enums_with_dependencies() {
    let source = r#"
        enum Inner {
            Value(i32),
        }

        enum Outer {
            Nested(Inner),
        }

        fn process(_: Outer) {}
    "#;

    let (_, unit, collection) = compile(source);

    let inner_symbol = enum_symbol(unit, &collection, "Inner");
    let outer_symbol = enum_symbol(unit, &collection, "Outer");
    let process_symbol = function_symbol(unit, &collection, "process");

    assert_relation(outer_symbol, inner_symbol);
    assert_relation(process_symbol, outer_symbol);
}

#[test]
fn module_with_impl_block() {
    let source = r#"
        mod domain {
            pub struct Entity;

            impl Entity {
                pub fn new() -> Entity {
                    Entity
                }

                pub fn process(&self) {}
            }
        }

        fn create() -> domain::Entity {
            domain::Entity::new()
        }
    "#;

    let (_, unit, collection) = compile(source);

    let entity_symbol = struct_symbol(unit, &collection, "Entity");
    let new_symbol = function_symbol(unit, &collection, "new");
    let process_symbol = function_symbol(unit, &collection, "process");
    let create_symbol = function_symbol(unit, &collection, "create");

    assert_relation(entity_symbol, new_symbol);
    assert_relation(entity_symbol, process_symbol);
    assert_relation(create_symbol, entity_symbol);
    assert_relation(create_symbol, new_symbol);
}

#[test]
fn cross_module_type_dependencies() {
    let source = r#"
        mod module_a {
            pub struct TypeA;
        }

        mod module_b {
            use super::module_a::TypeA;

            pub struct TypeB {
                field: TypeA,
            }
        }

        fn uses(_: module_b::TypeB) {}
    "#;

    let (_, unit, collection) = compile(source);

    let type_a_symbol = struct_symbol(unit, &collection, "TypeA");
    let type_b_symbol = struct_symbol(unit, &collection, "TypeB");
    let uses_symbol = function_symbol(unit, &collection, "uses");

    assert_relation(type_b_symbol, type_a_symbol);
    assert_relation(uses_symbol, type_b_symbol);
}

#[test]
fn module_with_const_dependencies() {
    let source = r#"
        mod config {
            pub const DEFAULT_SIZE: usize = 100;

            pub struct Config {
                size: usize,
            }

            pub const DEFAULT_CONFIG: Config = Config { size: DEFAULT_SIZE };
        }

        fn get_config() -> config::Config {
            config::DEFAULT_CONFIG
        }
    "#;

    let (_, unit, collection) = compile(source);

    let default_size_desc = collection
        .variables
        .iter()
        .find(|desc| desc.name == "DEFAULT_SIZE")
        .unwrap();
    let default_config_desc = collection
        .variables
        .iter()
        .find(|desc| desc.name == "DEFAULT_CONFIG")
        .unwrap();

    let config_symbol = struct_symbol(unit, &collection, "Config");
    let default_size_symbol = symbol(unit, default_size_desc.hir_id);
    let default_config_symbol = symbol(unit, default_config_desc.hir_id);
    let get_config_symbol = function_symbol(unit, &collection, "get_config");

    assert_relation(default_config_symbol, config_symbol);
    assert_relation(default_config_symbol, default_size_symbol);
    assert_relation(get_config_symbol, config_symbol);
    assert_relation(get_config_symbol, default_config_symbol);
}

#[test]
fn enum_method_impl() {
    let source = r#"
        enum State {
            Active,
            Inactive,
        }

        impl State {
            fn is_active(&self) -> bool {
                matches!(self, State::Active)
            }

            fn toggle(&mut self) {
                *self = match self {
                    State::Active => State::Inactive,
                    State::Inactive => State::Active,
                };
            }
        }
    "#;

    let (_, unit, collection) = compile(source);

    let state_symbol = enum_symbol(unit, &collection, "State");
    let is_active_symbol = function_symbol(unit, &collection, "is_active");
    let toggle_symbol = function_symbol(unit, &collection, "toggle");

    assert_relation(state_symbol, is_active_symbol);
    assert_relation(state_symbol, toggle_symbol);
}

#[test]
fn complex_module_hierarchy_with_re_exports() {
    let source = r#"
        mod core {
            pub mod types {
                pub struct CoreType;
            }

            pub use types::CoreType;
        }

        mod application {
            use super::core::CoreType;

            pub struct App {
                core: CoreType,
            }
        }

        fn run(_: application::App) {}
    "#;

    let (_, unit, collection) = compile(source);

    let core_type_symbol = struct_symbol(unit, &collection, "CoreType");
    let app_symbol = struct_symbol(unit, &collection, "App");
    let run_symbol = function_symbol(unit, &collection, "run");

    assert_relation(app_symbol, core_type_symbol);
    assert_relation(run_symbol, app_symbol);
}

#[test]
fn sibling_modules_with_cross_dependencies() {
    let source = r#"
        mod module_x {
            pub struct TypeX;

            pub fn process_x(_: super::module_y::TypeY) {}
        }

        mod module_y {
            pub struct TypeY;

            pub fn process_y(_: super::module_x::TypeX) {}
        }
    "#;

    let (_, unit, collection) = compile(source);

    let type_x_symbol = struct_symbol(unit, &collection, "TypeX");
    let type_y_symbol = struct_symbol(unit, &collection, "TypeY");
    let process_x_symbol = function_symbol(unit, &collection, "process_x");
    let process_y_symbol = function_symbol(unit, &collection, "process_y");

    assert_relation(process_x_symbol, type_y_symbol);
    assert_relation(process_y_symbol, type_x_symbol);
}

#[test]
fn five_level_nested_modules() {
    let source = r#"
        mod l1 {
            pub mod l2 {
                pub mod l3 {
                    pub mod l4 {
                        pub mod l5 {
                            pub struct DeepStruct;

                            pub fn deep_function() {}
                        }
                    }
                }
            }
        }

        fn access_deep(_: l1::l2::l3::l4::l5::DeepStruct) {
            l1::l2::l3::l4::l5::deep_function();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let deep_struct_symbol = struct_symbol(unit, &collection, "DeepStruct");
    let deep_function_symbol = function_symbol(unit, &collection, "deep_function");
    let access_deep_symbol = function_symbol(unit, &collection, "access_deep");

    assert_relation(access_deep_symbol, deep_struct_symbol);
    assert_relation(access_deep_symbol, deep_function_symbol);
}

#[test]
fn enum_as_struct_field() {
    let source = r#"
        enum Status {
            Ready,
            Processing,
            Done,
        }

        struct Task {
            status: Status,
        }

        fn create_task() -> Task {
            Task { status: Status::Ready }
        }
    "#;

    let (_, unit, collection) = compile(source);

    let status_symbol = enum_symbol(unit, &collection, "Status");
    let task_symbol = struct_symbol(unit, &collection, "Task");
    let create_task_symbol = function_symbol(unit, &collection, "create_task");

    assert_relation(task_symbol, status_symbol);
    assert_relation(create_task_symbol, task_symbol);
}

#[test]
fn generic_enum_with_constraints() {
    let source = r#"
        struct Wrapper<T> {
            value: T,
        }

        enum Option<T> {
            Some(T),
            None,
        }

        fn process(_: Option<Wrapper<i32>>) {}
    "#;

    let (_, unit, collection) = compile(source);

    let wrapper_symbol = struct_symbol(unit, &collection, "Wrapper");
    let option_symbol = enum_symbol(unit, &collection, "Option");
    let process_symbol = function_symbol(unit, &collection, "process");

    assert_relation(process_symbol, option_symbol);
    assert_relation(process_symbol, wrapper_symbol);
}

#[test]
fn module_with_trait_and_impl() {
    let source = r#"
        mod traits {
            pub trait Processable {
                fn process(&self);
            }
        }

        mod types {
            use super::traits::Processable;

            pub struct Processor;

            impl Processable for Processor {
                fn process(&self) {}
            }
        }
    "#;

    let (_, unit, collection) = compile(source);

    let processor_symbol = struct_symbol(unit, &collection, "Processor");
    let process_symbol = function_symbol(unit, &collection, "process");

    assert_relation(processor_symbol, process_symbol);
}

#[test]
fn complex_cross_module_enum_struct_dependencies() {
    let source = r#"
        mod data {
            pub enum DataType {
                Integer(i32),
                Float(f64),
            }
        }

        mod storage {
            use super::data::DataType;

            pub struct Storage {
                items: Vec<DataType>,
            }

            impl Storage {
                pub fn add(&mut self, item: DataType) {}
            }
        }

        fn main_app() {
            let mut s = storage::Storage { items: vec![] };
            s.add(data::DataType::Integer(42));
        }
    "#;

    let (_, unit, collection) = compile(source);

    let data_type_symbol = enum_symbol(unit, &collection, "DataType");
    let storage_symbol = struct_symbol(unit, &collection, "Storage");
    let add_symbol = function_symbol(unit, &collection, "add");
    let main_app_symbol = function_symbol(unit, &collection, "main_app");

    assert_relation(storage_symbol, data_type_symbol);
    assert_relation(storage_symbol, add_symbol);
    assert_relation(add_symbol, data_type_symbol);
    assert_relation(main_app_symbol, storage_symbol);
    assert_relation(main_app_symbol, data_type_symbol);
}

#[test]
fn nested_modules_with_multiple_types_and_functions() {
    let source = r#"
        mod outer {
            pub struct OuterStruct;

            pub mod middle {
                pub struct MiddleStruct;

                pub mod inner {
                    pub struct InnerStruct;

                    pub fn inner_fn() {}
                }

                pub fn middle_fn(_: inner::InnerStruct) {
                    inner::inner_fn();
                }
            }

            pub fn outer_fn(_: middle::MiddleStruct) {
                middle::middle_fn(middle::inner::InnerStruct);
            }
        }

        fn root(_: outer::OuterStruct) {
            outer::outer_fn(outer::middle::MiddleStruct);
        }
    "#;

    let (_, unit, collection) = compile(source);

    let outer_struct_symbol = struct_symbol(unit, &collection, "OuterStruct");
    let middle_struct_symbol = struct_symbol(unit, &collection, "MiddleStruct");
    let inner_struct_symbol = struct_symbol(unit, &collection, "InnerStruct");
    let inner_fn_symbol = function_symbol(unit, &collection, "inner_fn");
    let middle_fn_symbol = function_symbol(unit, &collection, "middle_fn");
    let outer_fn_symbol = function_symbol(unit, &collection, "outer_fn");
    let root_symbol = function_symbol(unit, &collection, "root");

    assert_relation(middle_fn_symbol, inner_struct_symbol);
    assert_relation(middle_fn_symbol, inner_fn_symbol);
    assert_relation(outer_fn_symbol, middle_struct_symbol);
    assert_relation(outer_fn_symbol, middle_fn_symbol);
    assert_relation(root_symbol, outer_struct_symbol);
    assert_relation(root_symbol, outer_fn_symbol);
}

#[test]
fn multiple_dependencies_same_function() {
    let source = r#"
        fn dep1() {}
        fn dep2() {}
        fn dep3() {}

        fn caller() {
            dep1();
            dep2();
            dep3();
        }
    "#;

    let (_, unit, collection) = compile(source);

    let dep1_symbol = function_symbol(unit, &collection, "dep1");
    let dep2_symbol = function_symbol(unit, &collection, "dep2");
    let dep3_symbol = function_symbol(unit, &collection, "dep3");
    let caller_symbol = function_symbol(unit, &collection, "caller");

    assert_relation(caller_symbol, dep1_symbol);
    assert_relation(caller_symbol, dep2_symbol);
    assert_relation(caller_symbol, dep3_symbol);
}

#[test]
fn method_depends_on_type_and_function() {
    let source = r#"
        struct Foo;

        fn external_helper() {}

        impl Foo {
            fn method(&self) {
                external_helper();
            }
        }
    "#;

    let (_, unit, collection) = compile(source);

    let foo_symbol = struct_symbol(unit, &collection, "Foo");
    let helper_symbol = function_symbol(unit, &collection, "external_helper");
    let method_symbol = function_symbol(unit, &collection, "method");

    assert_relation(foo_symbol, method_symbol);
    assert_relation(method_symbol, helper_symbol);
}

#[test]
fn generic_type_parameter_creates_dependency() {
    let source = r#"
        struct Container<T> {
            value: T,
        }

        struct Item;

        fn uses(_: Container<Item>) {}
    "#;

    let (_, unit, collection) = compile(source);

    let container_symbol = struct_symbol(unit, &collection, "Container");
    let item_symbol = struct_symbol(unit, &collection, "Item");
    let uses_symbol = function_symbol(unit, &collection, "uses");

    assert_relation(uses_symbol, container_symbol);
    assert_relation(uses_symbol, item_symbol);
}

#[test]
fn const_depends_on_type_and_function() {
    let source = r#"
        struct Config;

        fn create_config() -> Config {
            Config
        }

        const GLOBAL_CONFIG: Config = create_config();
    "#;

    let (_, unit, collection) = compile(source);

    let const_desc = collection
        .variables
        .iter()
        .find(|desc| desc.name == "GLOBAL_CONFIG")
        .unwrap();

    let config_symbol = struct_symbol(unit, &collection, "Config");
    let create_symbol = function_symbol(unit, &collection, "create_config");
    let const_symbol = symbol(unit, const_desc.hir_id);

    assert_relation(const_symbol, config_symbol);
    assert_relation(const_symbol, create_symbol);
}

#[test]
fn static_variable_dependencies() {
    let source = r#"
        fn init_value() -> i32 { 42 }

        static COUNTER: i32 = init_value();
    "#;

    let (_, unit, collection) = compile(source);

    let static_desc = collection
        .variables
        .iter()
        .find(|desc| desc.name == "COUNTER")
        .unwrap();

    let init_symbol = function_symbol(unit, &collection, "init_value");
    let static_symbol = symbol(unit, static_desc.hir_id);

    assert_relation(static_symbol, init_symbol);
}

#[test]
fn multiple_impl_blocks_same_type() {
    let source = r#"
        struct Widget;

        impl Widget {
            fn method1(&self) {}
        }

        impl Widget {
            fn method2(&self) {}
        }
    "#;

    let (_, unit, collection) = compile(source);

    let widget_symbol = struct_symbol(unit, &collection, "Widget");
    let method1_symbol = function_symbol(unit, &collection, "method1");
    let method2_symbol = function_symbol(unit, &collection, "method2");

    assert_relation(widget_symbol, method1_symbol);
    assert_relation(widget_symbol, method2_symbol);
}

#[test]
fn cross_method_dependencies_in_impl() {
    let source = r#"
        struct Service;

        impl Service {
            fn internal_helper(&self) {}

            fn public_api(&self) {
                self.internal_helper();
            }

            fn another_api(&self) {
                self.internal_helper();
                self.public_api();
            }
        }
    "#;

    let (_, unit, collection) = compile(source);

    let helper_symbol = function_symbol(unit, &collection, "internal_helper");
    let public_symbol = function_symbol(unit, &collection, "public_api");
    let another_symbol = function_symbol(unit, &collection, "another_api");

    assert_relation(public_symbol, helper_symbol);
    assert_relation(another_symbol, helper_symbol);
    assert_relation(another_symbol, public_symbol);
}

#[test]
fn tuple_struct_dependency() {
    let source = r#"
        struct Inner;

        struct Wrapper(Inner);
    "#;

    let (_, unit, collection) = compile(source);

    let inner_symbol = struct_symbol(unit, &collection, "Inner");
    let wrapper_symbol = struct_symbol(unit, &collection, "Wrapper");

    assert_relation(wrapper_symbol, inner_symbol);
}

#[test]
fn enum_variant_type_dependencies() {
    let source = r#"
        struct Data;

        enum Message {
            Empty,
            WithData(Data),
        }
    "#;

    let (_, unit, collection) = compile(source);

    let data_symbol = struct_symbol(unit, &collection, "Data");
    let message_symbol = enum_symbol(unit, &collection, "Message");

    assert_relation(message_symbol, data_symbol);
}

#[test]
fn associated_function_depends_on_type() {
    let source = r#"
        struct Builder;

        impl Builder {
            fn new() -> Builder {
                Builder
            }
        }
    "#;

    let (_, unit, collection) = compile(source);

    let builder_symbol = struct_symbol(unit, &collection, "Builder");
    let new_symbol = function_symbol(unit, &collection, "new");

    assert_relation(builder_symbol, new_symbol);
    assert_relation(new_symbol, builder_symbol);
}

#[test]
fn nested_function_calls_with_types() {
    let source = r#"
        struct A;
        struct B;
        struct C;

        fn process_a(_: A) {}
        fn process_b(_: B) { process_a(A); }
        fn process_c(_: C) { process_b(B); }
    "#;

    let (_, unit, collection) = compile(source);

    let a_symbol = struct_symbol(unit, &collection, "A");
    let b_symbol = struct_symbol(unit, &collection, "B");
    let c_symbol = struct_symbol(unit, &collection, "C");
    let process_a_symbol = function_symbol(unit, &collection, "process_a");
    let process_b_symbol = function_symbol(unit, &collection, "process_b");
    let process_c_symbol = function_symbol(unit, &collection, "process_c");

    assert_relation(process_a_symbol, a_symbol);
    assert_relation(process_b_symbol, b_symbol);
    assert_relation(process_b_symbol, process_a_symbol);
    assert_relation(process_c_symbol, c_symbol);
    assert_relation(process_c_symbol, process_b_symbol);
}

#[test]
fn complex_nested_generics() {
    let source = r#"
        struct Outer<T> {
            inner: T,
        }

        struct Middle<U> {
            data: U,
        }

        struct Core;

        fn process(_: Outer<Middle<Core>>) {}
    "#;

    let (_, unit, collection) = compile(source);

    let outer_symbol = struct_symbol(unit, &collection, "Outer");
    let middle_symbol = struct_symbol(unit, &collection, "Middle");
    let core_symbol = struct_symbol(unit, &collection, "Core");
    let process_symbol = function_symbol(unit, &collection, "process");

    assert_relation(process_symbol, outer_symbol);
    assert_relation(process_symbol, middle_symbol);
    assert_relation(process_symbol, core_symbol);
}

#[test]
fn circular_type_references_via_pointers() {
    let source = r#"
        struct Node {
            next: Option<Box<Node>>,
        }
    "#;

    let (_, unit, collection) = compile(source);

    let node_symbol = struct_symbol(unit, &collection, "Node");

    // Verify node shouldn't depends on itself
    assert!(node_symbol.depended.borrow().is_empty());
    assert!(node_symbol.depends.borrow().is_empty());
}

#[test]
fn multiple_parameters_multiple_types() {
    let source = r#"
        struct First;
        struct Second;
        struct Third;

        fn multi_param(_a: First, _b: Second, _c: Third) {}
    "#;

    let (_, unit, collection) = compile(source);

    let first_symbol = struct_symbol(unit, &collection, "First");
    let second_symbol = struct_symbol(unit, &collection, "Second");
    let third_symbol = struct_symbol(unit, &collection, "Third");
    let multi_symbol = function_symbol(unit, &collection, "multi_param");

    assert_relation(multi_symbol, first_symbol);
    assert_relation(multi_symbol, second_symbol);
    assert_relation(multi_symbol, third_symbol);
}

#[test]
fn trait_impl_method_dependencies() {
    let source = r#"
        trait Processor {
            fn process(&self);
        }

        struct Handler;

        impl Processor for Handler {
            fn process(&self) {}
        }
    "#;

    let (_, unit, collection) = compile(source);

    let handler_symbol = struct_symbol(unit, &collection, "Handler");
    let process_symbol = function_symbol(unit, &collection, "process");

    assert_relation(handler_symbol, process_symbol);
}

#[test]
fn const_references_other_const() {
    let source = r#"
        const BASE: i32 = 10;
        const DERIVED: i32 = BASE * 2;
    "#;

    let (_, unit, collection) = compile(source);

    let base_desc = collection
        .variables
        .iter()
        .find(|desc| desc.name == "BASE")
        .unwrap();
    let derived_desc = collection
        .variables
        .iter()
        .find(|desc| desc.name == "DERIVED")
        .unwrap();

    let base_symbol = symbol(unit, base_desc.hir_id);
    let derived_symbol = symbol(unit, derived_desc.hir_id);

    assert_relation(derived_symbol, base_symbol);
}


#[test]
fn test_impl_from_with_qualified_type() {
    let code = r#"
struct SandboxWorkspaceWrite {
    writable_roots: Vec<String>,
    network_access: bool,
    exclude_tmpdir_env_var: bool,
    exclude_slash_tmp: bool,
}

mod codex_app_server_protocol {
    pub struct SandboxSettings {
        pub writable_roots: Vec<String>,
        pub network_access: Option<bool>,
        pub exclude_tmpdir_env_var: Option<bool>,
        pub exclude_slash_tmp: Option<bool>,
    }
}

impl From<SandboxWorkspaceWrite> for codex_app_server_protocol::SandboxSettings {
    fn from(sandbox_workspace_write: SandboxWorkspaceWrite) -> Self {
        Self {
            writable_roots: sandbox_workspace_write.writable_roots,
            network_access: Some(sandbox_workspace_write.network_access),
            exclude_tmpdir_env_var: Some(sandbox_workspace_write.exclude_tmpdir_env_var),
            exclude_slash_tmp: Some(sandbox_workspace_write.exclude_slash_tmp),
        }
    }
}
"#;

    let cc = CompileCtxt::from_sources::<LangRust>(&[code.as_bytes().to_vec()]);
    let unit = cc.compile_unit(0);

    // Build IR
    build_llmcc_ir::<LangRust>(unit).expect("failed to build IR");

    // Collect symbols
    let globals = cc.create_globals();
    collect_symbols(unit, globals);

    // Bind symbols
    bind_symbols(unit, globals);

    // Verify the impl block was processed without panicking
    let all_symbols = globals.all_symbols();
    println!("Collected {} symbols", all_symbols.len());
    for sym in all_symbols.iter() {
        println!("  - {} (kind: {:?})", sym.name, sym.kind());
    }

    assert!(!all_symbols.is_empty(), "Should have collected some symbols");

    // Check that we have the impl symbol or at least the types it uses
    let has_struct = all_symbols.iter().any(|sym| {
        sym.kind() == llmcc_core::symbol::SymbolKind::Struct
    });
    println!("Has struct: {}", has_struct);

    let has_impl = all_symbols.iter().any(|sym| {
        sym.kind() == llmcc_core::symbol::SymbolKind::Impl
    });
    println!("Has impl: {}", has_impl);
}