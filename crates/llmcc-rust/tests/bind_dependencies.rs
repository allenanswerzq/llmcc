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

fn symbol(unit: llmcc_core::context::CompileUnit<'static>, hir_id: HirId) -> &'static Symbol {
    unit.get_scope(hir_id).symbol().unwrap()
}

fn assert_depends_on(symbol: &Symbol, target: &Symbol) {
    assert!(symbol.depends.borrow().iter().any(|id| *id == target.id));
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

    let foo_desc = find_struct(&collection, "Foo");
    let method_desc = find_function(&collection, "method");

    let foo_symbol = symbol(unit, foo_desc.hir_id);
    let method_symbol = symbol(unit, method_desc.hir_id);

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

    let helper_desc = find_function(&collection, "helper");
    let caller_desc = find_function(&collection, "caller");

    let helper_symbol = symbol(unit, helper_desc.hir_id);
    let caller_symbol = symbol(unit, caller_desc.hir_id);

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

    let helper_desc = find_function(&collection, "helper");
    let caller_desc = find_function(&collection, "caller");

    let helper_symbol = symbol(unit, helper_desc.hir_id);
    let caller_symbol = symbol(unit, caller_desc.hir_id);

    assert_relation(caller_symbol, helper_symbol);
}

#[test]
fn function_depends_on_argument_type() {
    let source = r#"
        struct Foo;

        fn takes(_: Foo) {}
    "#;

    let (_, unit, collection) = compile(source);

    let foo_desc = find_struct(&collection, "Foo");
    let takes_desc = find_function(&collection, "takes");

    let foo_symbol = symbol(unit, foo_desc.hir_id);
    let takes_symbol = symbol(unit, takes_desc.hir_id);

    assert_relation(takes_symbol, foo_symbol);
}

#[test]
fn const_initializer_records_dependencies() {
    let source = r#"
        fn helper() -> i32 { 5 }

        const VALUE: i32 = helper();
    "#;

    let (_, unit, collection) = compile(source);

    let helper_desc = find_function(&collection, "helper");
    let const_desc = collection
        .variables
        .iter()
        .find(|desc| desc.name == "VALUE")
        .unwrap();

    let helper_symbol = symbol(unit, helper_desc.hir_id);
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

    let bar_desc = find_struct(&collection, "Bar");
    let returns_desc = find_function(&collection, "returns");

    let bar_symbol = symbol(unit, bar_desc.hir_id);
    let returns_symbol = symbol(unit, returns_desc.hir_id);

    assert_relation(returns_symbol, bar_symbol);
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

    let inner_desc = find_struct(&collection, "Inner");
    let outer_desc = find_struct(&collection, "Outer");

    let inner_symbol = symbol(unit, inner_desc.hir_id);
    let outer_symbol = symbol(unit, outer_desc.hir_id);

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

    let status_desc = find_enum(&collection, "Status");
    let holder_desc = find_struct(&collection, "Holder");

    let status_symbol = symbol(unit, status_desc.hir_id);
    let holder_symbol = symbol(unit, holder_desc.hir_id);

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

    let payload_desc = find_struct(&collection, "Payload");
    let message_desc = find_enum(&collection, "Message");

    let payload_symbol = symbol(unit, payload_desc.hir_id);
    let message_symbol = symbol(unit, message_desc.hir_id);

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

    let event_desc = find_enum(&collection, "Event");
    let handle_desc = find_function(&collection, "handle");

    let event_symbol = symbol(unit, event_desc.hir_id);
    let handle_symbol = symbol(unit, handle_desc.hir_id);

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

    let action_desc = find_enum(&collection, "Action");
    let handle_desc = find_function(&collection, "handle");

    let action_symbol = symbol(unit, action_desc.hir_id);
    let handle_symbol = symbol(unit, handle_desc.hir_id);

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

    let a_desc = find_struct(&collection, "A");
    let b_desc = find_struct(&collection, "B");
    let c_desc = find_struct(&collection, "C");

    let a_symbol = symbol(unit, a_desc.hir_id);
    let b_symbol = symbol(unit, b_desc.hir_id);
    let c_symbol = symbol(unit, c_desc.hir_id);

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

    let l1_desc = find_function(&collection, "level1");
    let l2_desc = find_function(&collection, "level2");
    let l3_desc = find_function(&collection, "level3");
    let l4_desc = find_function(&collection, "level4");

    let l1_symbol = symbol(unit, l1_desc.hir_id);
    let l2_symbol = symbol(unit, l2_desc.hir_id);
    let l3_symbol = symbol(unit, l3_desc.hir_id);
    let l4_symbol = symbol(unit, l4_desc.hir_id);

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

    let outer_type_desc = find_struct(&collection, "OuterType");
    let inner_type_desc = find_struct(&collection, "InnerType");
    let uses_desc = find_function(&collection, "uses");

    let outer_type_symbol = symbol(unit, outer_type_desc.hir_id);
    let inner_type_symbol = symbol(unit, inner_type_desc.hir_id);
    let uses_symbol = symbol(unit, uses_desc.hir_id);

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

    let deep_type_desc = find_struct(&collection, "DeepType");
    let access_desc = find_function(&collection, "access");

    let deep_type_symbol = symbol(unit, deep_type_desc.hir_id);
    let access_symbol = symbol(unit, access_desc.hir_id);

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

    let helper1_desc = find_function(&collection, "helper1");
    let helper2_desc = find_function(&collection, "helper2");
    let main_desc = find_function(&collection, "main");

    let helper1_symbol = symbol(unit, helper1_desc.hir_id);
    let helper2_symbol = symbol(unit, helper2_desc.hir_id);
    let main_symbol = symbol(unit, main_desc.hir_id);

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

    let type_a_desc = find_struct(&collection, "TypeA");
    let type_b_desc = find_struct(&collection, "TypeB");
    let type_c_desc = find_struct(&collection, "TypeC");
    let enum_desc = find_enum(&collection, "MultiVariant");

    let type_a_symbol = symbol(unit, type_a_desc.hir_id);
    let type_b_symbol = symbol(unit, type_b_desc.hir_id);
    let type_c_symbol = symbol(unit, type_c_desc.hir_id);
    let enum_symbol = symbol(unit, enum_desc.hir_id);

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

    let inner_desc = find_struct(&collection, "Inner");
    let result_desc = find_enum(&collection, "Result");

    let inner_symbol = symbol(unit, inner_desc.hir_id);
    let result_symbol = symbol(unit, result_desc.hir_id);

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

    let inner_desc = find_enum(&collection, "Inner");
    let outer_desc = find_enum(&collection, "Outer");
    let process_desc = find_function(&collection, "process");

    let inner_symbol = symbol(unit, inner_desc.hir_id);
    let outer_symbol = symbol(unit, outer_desc.hir_id);
    let process_symbol = symbol(unit, process_desc.hir_id);

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

    let entity_desc = find_struct(&collection, "Entity");
    let new_desc = find_function(&collection, "new");
    let process_desc = find_function(&collection, "process");
    let create_desc = find_function(&collection, "create");

    let entity_symbol = symbol(unit, entity_desc.hir_id);
    let new_symbol = symbol(unit, new_desc.hir_id);
    let process_symbol = symbol(unit, process_desc.hir_id);
    let create_symbol = symbol(unit, create_desc.hir_id);

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

    let type_a_desc = find_struct(&collection, "TypeA");
    let type_b_desc = find_struct(&collection, "TypeB");
    let uses_desc = find_function(&collection, "uses");

    let type_a_symbol = symbol(unit, type_a_desc.hir_id);
    let type_b_symbol = symbol(unit, type_b_desc.hir_id);
    let uses_symbol = symbol(unit, uses_desc.hir_id);

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

    let config_desc = find_struct(&collection, "Config");
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
    let get_config_desc = find_function(&collection, "get_config");

    let config_symbol = symbol(unit, config_desc.hir_id);
    let default_size_symbol = symbol(unit, default_size_desc.hir_id);
    let default_config_symbol = symbol(unit, default_config_desc.hir_id);
    let get_config_symbol = symbol(unit, get_config_desc.hir_id);

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

    let state_desc = find_enum(&collection, "State");
    let is_active_desc = find_function(&collection, "is_active");
    let toggle_desc = find_function(&collection, "toggle");

    let state_symbol = symbol(unit, state_desc.hir_id);
    let is_active_symbol = symbol(unit, is_active_desc.hir_id);
    let toggle_symbol = symbol(unit, toggle_desc.hir_id);

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

    let core_type_desc = find_struct(&collection, "CoreType");
    let app_desc = find_struct(&collection, "App");
    let run_desc = find_function(&collection, "run");

    let core_type_symbol = symbol(unit, core_type_desc.hir_id);
    let app_symbol = symbol(unit, app_desc.hir_id);
    let run_symbol = symbol(unit, run_desc.hir_id);

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

    let type_x_desc = find_struct(&collection, "TypeX");
    let type_y_desc = find_struct(&collection, "TypeY");
    let process_x_desc = find_function(&collection, "process_x");
    let process_y_desc = find_function(&collection, "process_y");

    let type_x_symbol = symbol(unit, type_x_desc.hir_id);
    let type_y_symbol = symbol(unit, type_y_desc.hir_id);
    let process_x_symbol = symbol(unit, process_x_desc.hir_id);
    let process_y_symbol = symbol(unit, process_y_desc.hir_id);

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

    let deep_struct_desc = find_struct(&collection, "DeepStruct");
    let deep_function_desc = find_function(&collection, "deep_function");
    let access_deep_desc = find_function(&collection, "access_deep");

    let deep_struct_symbol = symbol(unit, deep_struct_desc.hir_id);
    let deep_function_symbol = symbol(unit, deep_function_desc.hir_id);
    let access_deep_symbol = symbol(unit, access_deep_desc.hir_id);

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

    let status_desc = find_enum(&collection, "Status");
    let task_desc = find_struct(&collection, "Task");
    let create_task_desc = find_function(&collection, "create_task");

    let status_symbol = symbol(unit, status_desc.hir_id);
    let task_symbol = symbol(unit, task_desc.hir_id);
    let create_task_symbol = symbol(unit, create_task_desc.hir_id);

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

    let wrapper_desc = find_struct(&collection, "Wrapper");
    let option_desc = find_enum(&collection, "Option");
    let process_desc = find_function(&collection, "process");

    let wrapper_symbol = symbol(unit, wrapper_desc.hir_id);
    let option_symbol = symbol(unit, option_desc.hir_id);
    let process_symbol = symbol(unit, process_desc.hir_id);

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

    let processor_desc = find_struct(&collection, "Processor");
    let process_desc = find_function(&collection, "process");

    let processor_symbol = symbol(unit, processor_desc.hir_id);
    let process_symbol = symbol(unit, process_desc.hir_id);

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

    let data_type_desc = find_enum(&collection, "DataType");
    let storage_desc = find_struct(&collection, "Storage");
    let add_desc = find_function(&collection, "add");
    let main_app_desc = find_function(&collection, "main_app");

    let data_type_symbol = symbol(unit, data_type_desc.hir_id);
    let storage_symbol = symbol(unit, storage_desc.hir_id);
    let add_symbol = symbol(unit, add_desc.hir_id);
    let main_app_symbol = symbol(unit, main_app_desc.hir_id);

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

    let outer_struct_desc = find_struct(&collection, "OuterStruct");
    let middle_struct_desc = find_struct(&collection, "MiddleStruct");
    let inner_struct_desc = find_struct(&collection, "InnerStruct");
    let inner_fn_desc = find_function(&collection, "inner_fn");
    let middle_fn_desc = find_function(&collection, "middle_fn");
    let outer_fn_desc = find_function(&collection, "outer_fn");
    let root_desc = find_function(&collection, "root");

    let outer_struct_symbol = symbol(unit, outer_struct_desc.hir_id);
    let middle_struct_symbol = symbol(unit, middle_struct_desc.hir_id);
    let inner_struct_symbol = symbol(unit, inner_struct_desc.hir_id);
    let inner_fn_symbol = symbol(unit, inner_fn_desc.hir_id);
    let middle_fn_symbol = symbol(unit, middle_fn_desc.hir_id);
    let outer_fn_symbol = symbol(unit, outer_fn_desc.hir_id);
    let root_symbol = symbol(unit, root_desc.hir_id);

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

    let dep1_desc = find_function(&collection, "dep1");
    let dep2_desc = find_function(&collection, "dep2");
    let dep3_desc = find_function(&collection, "dep3");
    let caller_desc = find_function(&collection, "caller");

    let dep1_symbol = symbol(unit, dep1_desc.hir_id);
    let dep2_symbol = symbol(unit, dep2_desc.hir_id);
    let dep3_symbol = symbol(unit, dep3_desc.hir_id);
    let caller_symbol = symbol(unit, caller_desc.hir_id);

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

    let foo_desc = find_struct(&collection, "Foo");
    let helper_desc = find_function(&collection, "external_helper");
    let method_desc = find_function(&collection, "method");

    let foo_symbol = symbol(unit, foo_desc.hir_id);
    let helper_symbol = symbol(unit, helper_desc.hir_id);
    let method_symbol = symbol(unit, method_desc.hir_id);

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

    let container_desc = find_struct(&collection, "Container");
    let item_desc = find_struct(&collection, "Item");
    let uses_desc = find_function(&collection, "uses");

    let container_symbol = symbol(unit, container_desc.hir_id);
    let item_symbol = symbol(unit, item_desc.hir_id);
    let uses_symbol = symbol(unit, uses_desc.hir_id);

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

    let config_desc = find_struct(&collection, "Config");
    let create_desc = find_function(&collection, "create_config");
    let const_desc = collection
        .variables
        .iter()
        .find(|desc| desc.name == "GLOBAL_CONFIG")
        .unwrap();

    let config_symbol = symbol(unit, config_desc.hir_id);
    let create_symbol = symbol(unit, create_desc.hir_id);
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

    let init_desc = find_function(&collection, "init_value");
    let static_desc = collection
        .variables
        .iter()
        .find(|desc| desc.name == "COUNTER")
        .unwrap();

    let init_symbol = symbol(unit, init_desc.hir_id);
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

    let widget_desc = find_struct(&collection, "Widget");
    let method1_desc = find_function(&collection, "method1");
    let method2_desc = find_function(&collection, "method2");

    let widget_symbol = symbol(unit, widget_desc.hir_id);
    let method1_symbol = symbol(unit, method1_desc.hir_id);
    let method2_symbol = symbol(unit, method2_desc.hir_id);

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

    let helper_desc = find_function(&collection, "internal_helper");
    let public_desc = find_function(&collection, "public_api");
    let another_desc = find_function(&collection, "another_api");

    let helper_symbol = symbol(unit, helper_desc.hir_id);
    let public_symbol = symbol(unit, public_desc.hir_id);
    let another_symbol = symbol(unit, another_desc.hir_id);

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

    let inner_desc = find_struct(&collection, "Inner");
    let wrapper_desc = find_struct(&collection, "Wrapper");

    let inner_symbol = symbol(unit, inner_desc.hir_id);
    let wrapper_symbol = symbol(unit, wrapper_desc.hir_id);

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

    let data_desc = find_struct(&collection, "Data");
    let message_desc = find_enum(&collection, "Message");

    let data_symbol = symbol(unit, data_desc.hir_id);
    let message_symbol = symbol(unit, message_desc.hir_id);

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

    let builder_desc = find_struct(&collection, "Builder");
    let new_desc = find_function(&collection, "new");

    let builder_symbol = symbol(unit, builder_desc.hir_id);
    let new_symbol = symbol(unit, new_desc.hir_id);

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

    let a_desc = find_struct(&collection, "A");
    let b_desc = find_struct(&collection, "B");
    let c_desc = find_struct(&collection, "C");
    let process_a_desc = find_function(&collection, "process_a");
    let process_b_desc = find_function(&collection, "process_b");
    let process_c_desc = find_function(&collection, "process_c");

    let a_symbol = symbol(unit, a_desc.hir_id);
    let b_symbol = symbol(unit, b_desc.hir_id);
    let c_symbol = symbol(unit, c_desc.hir_id);
    let process_a_symbol = symbol(unit, process_a_desc.hir_id);
    let process_b_symbol = symbol(unit, process_b_desc.hir_id);
    let process_c_symbol = symbol(unit, process_c_desc.hir_id);

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

    let outer_desc = find_struct(&collection, "Outer");
    let middle_desc = find_struct(&collection, "Middle");
    let core_desc = find_struct(&collection, "Core");
    let process_desc = find_function(&collection, "process");

    let outer_symbol = symbol(unit, outer_desc.hir_id);
    let middle_symbol = symbol(unit, middle_desc.hir_id);
    let core_symbol = symbol(unit, core_desc.hir_id);
    let process_symbol = symbol(unit, process_desc.hir_id);

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

    let node_desc = find_struct(&collection, "Node");
    let node_symbol = symbol(unit, node_desc.hir_id);

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

    let first_desc = find_struct(&collection, "First");
    let second_desc = find_struct(&collection, "Second");
    let third_desc = find_struct(&collection, "Third");
    let multi_desc = find_function(&collection, "multi_param");

    let first_symbol = symbol(unit, first_desc.hir_id);
    let second_symbol = symbol(unit, second_desc.hir_id);
    let third_symbol = symbol(unit, third_desc.hir_id);
    let multi_symbol = symbol(unit, multi_desc.hir_id);

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

    let handler_desc = find_struct(&collection, "Handler");
    let process_desc = find_function(&collection, "process");

    let handler_symbol = symbol(unit, handler_desc.hir_id);
    let process_symbol = symbol(unit, process_desc.hir_id);

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
