mod common;

use llmcc_core::block::BlockKind;
use llmcc_core::graph_builder::{GraphBuildOption, build_llmcc_graph};
use llmcc_rust::LangRust;

use common::with_compiled_unit;
use serial_test::serial;
use textwrap::dedent;

/// Helper to build graph and run checks
fn with_graph<F>(sources: &[&str], check: F)
where
    F: for<'a> FnOnce(&'a llmcc_core::context::CompileCtxt<'a>, Vec<llmcc_core::graph::UnitGraph>),
{
    with_compiled_unit(sources, |cc| {
        let graphs = build_llmcc_graph::<LangRust>(cc, GraphBuildOption::default()).unwrap();
        check(cc, graphs);
    });
}

/// Get a block from CompileCtxt by BlockId
fn get_block<'a>(
    cc: &'a llmcc_core::context::CompileCtxt<'a>,
    id: llmcc_core::block::BlockId,
) -> llmcc_core::block::BasicBlock<'a> {
    let index = (id.0 as usize).saturating_sub(1);
    (*cc.block_arena.bb().get(index).expect("block not found")).clone()
}

// ============================================================================
// BlockFunc Tests
// ============================================================================

#[serial]
#[test]
fn test_block_func_basic() {
    let source = dedent(
        "
        fn hello() {
            println!(\"Hello\");
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);
        let graph = &graphs[0];

        // Find the function block
        let root = get_block(cc, graph.root());
        assert_eq!(root.kind(), BlockKind::Root);

        // Look for function in children
        let mut found_func = false;
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Func {
                found_func = true;
                if let Some(func) = child.as_func() {
                    assert_eq!(func.base.kind, BlockKind::Func);
                }
            }
        }
        assert!(found_func, "Should find a function block");
    });
}

#[serial]
#[test]
fn test_block_func_with_parameters_and_return() {
    let source = dedent(
        "
        fn add(a: i32, b: i32) -> i32 {
            a + b
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        // Find function block
        let root = get_block(cc, graphs[0].root());
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if let Some(func) = child.as_func() {
                // Check that parameters block is set
                let params = func.get_parameters();
                assert!(params.is_some(), "Function should have parameters block");

                // Check that return block is set
                let returns = func.get_returns();
                assert!(returns.is_some(), "Function should have return block");
            }
        }
    });
}

#[serial]
#[test]
fn test_block_method() {
    let source = dedent(
        "
        struct Point {
            x: i32,
            y: i32,
        }

        impl Point {
            fn new(x: i32, y: i32) -> Self {
                Point { x, y }
            }

            fn distance(&self) -> f64 {
                ((self.x * self.x + self.y * self.y) as f64).sqrt()
            }
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        // Find struct block and check it has methods from impl
        let root = get_block(cc, graphs[0].root());
        let mut found_struct = false;
        let mut found_impl = false;

        for &child_id in root.children() {
            let child = get_block(cc, child_id);

            if let Some(class) = child.as_class() {
                found_struct = true;
                // Struct should have methods added from impl
                let methods = class.get_methods();
                assert!(
                    methods.len() >= 2,
                    "Struct should have at least 2 methods from impl, got {}",
                    methods.len()
                );
            }

            if let Some(impl_block) = child.as_impl() {
                found_impl = true;
                // Impl should have methods
                let methods = impl_block.get_methods();
                assert_eq!(methods.len(), 2, "Impl should have 2 methods");

                // Impl should reference the target struct
                let target = impl_block.get_target();
                assert!(target.is_some(), "Impl should have target set");
            }
        }

        assert!(found_struct, "Should find struct block");
        assert!(found_impl, "Should find impl block");
    });
}

// ============================================================================
// BlockClass (Struct) Tests
// ============================================================================

#[serial]
#[test]
fn test_block_struct_with_fields() {
    let source = dedent(
        "
        struct Person {
            name: String,
            age: u32,
            active: bool,
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if let Some(class) = child.as_class() {
                let fields = class.get_fields();
                assert_eq!(fields.len(), 3, "Struct should have 3 fields");

                // Verify each field is a Field block
                for &field_id in &fields {
                    let field_block = get_block(cc, field_id);
                    assert_eq!(field_block.kind(), BlockKind::Field);
                }
            }
        }
    });
}

#[serial]
#[test]
fn test_block_tuple_struct() {
    let source = dedent(
        "
        struct Color(u8, u8, u8);
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        let mut found_struct = false;
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Class {
                found_struct = true;
            }
        }
        assert!(found_struct, "Should find tuple struct block");
    });
}

// ============================================================================
// BlockEnum Tests
// ============================================================================

#[serial]
#[test]
fn test_block_enum_with_variants() {
    let source = dedent(
        "
        enum Status {
            Active,
            Inactive,
            Pending,
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if let Some(enum_block) = child.as_enum() {
                let variants = enum_block.get_variants();
                assert_eq!(variants.len(), 3, "Enum should have 3 variants");
            }
        }
    });
}

#[serial]
#[test]
fn test_block_enum_with_data() {
    let source = dedent(
        "
        enum Message {
            Quit,
            Move { x: i32, y: i32 },
            Write(String),
            ChangeColor(i32, i32, i32),
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        let mut found_enum = false;
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if let Some(enum_block) = child.as_enum() {
                found_enum = true;
                let variants = enum_block.get_variants();
                assert_eq!(variants.len(), 4, "Enum should have 4 variants");
            }
        }
        assert!(found_enum, "Should find enum block");
    });
}

// ============================================================================
// BlockTrait Tests
// ============================================================================

#[serial]
#[test]
fn test_block_trait_with_methods() {
    let source = dedent(
        "
        trait Drawable {
            fn draw(&self);
            fn resize(&mut self, width: u32, height: u32);
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if let Some(trait_block) = child.as_trait() {
                let methods = trait_block.get_methods();
                assert_eq!(methods.len(), 2, "Trait should have 2 methods");
            }
        }
    });
}

#[serial]
#[test]
fn test_block_trait_with_default_impl() {
    let source = dedent(
        "
        trait Greet {
            fn name(&self) -> &str;

            fn greet(&self) {
                println!(\"Hello, {}!\", self.name());
            }
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        let mut found_trait = false;
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if let Some(trait_block) = child.as_trait() {
                found_trait = true;
                let methods = trait_block.get_methods();
                assert_eq!(methods.len(), 2, "Trait should have 2 methods");
            }
        }
        assert!(found_trait, "Should find trait block");
    });
}

// ============================================================================
// BlockImpl Tests
// ============================================================================

#[serial]
#[test]
fn test_block_impl_target() {
    let source = dedent(
        "
        struct Counter {
            value: i32,
        }

        impl Counter {
            fn new() -> Self {
                Counter { value: 0 }
            }

            fn increment(&mut self) {
                self.value += 1;
            }

            fn get(&self) -> i32 {
                self.value
            }
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        let mut struct_block_id = None;

        // First find the struct
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Class {
                struct_block_id = Some(child_id);
                break;
            }
        }

        assert!(struct_block_id.is_some(), "Should find struct block");

        // Then check impl references it
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if let Some(impl_block) = child.as_impl() {
                let target = impl_block.get_target();
                assert!(target.is_some(), "Impl should have target");
                assert_eq!(
                    target.unwrap(),
                    struct_block_id.unwrap(),
                    "Impl target should point to struct"
                );

                let methods = impl_block.get_methods();
                assert_eq!(methods.len(), 3, "Impl should have 3 methods");
            }
        }
    });
}

#[serial]
#[test]
fn test_block_impl_trait_for_struct() {
    let source = dedent(
        "
        trait Display {
            fn display(&self) -> String;
        }

        struct Item {
            name: String,
        }

        impl Display for Item {
            fn display(&self) -> String {
                self.name.clone()
            }
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        let mut found_trait = false;
        let mut found_struct = false;
        let mut found_impl = false;

        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            match child.kind() {
                BlockKind::Trait => found_trait = true,
                BlockKind::Class => found_struct = true,
                BlockKind::Impl => {
                    found_impl = true;
                    if let Some(impl_block) = child.as_impl() {
                        let methods = impl_block.get_methods();
                        assert_eq!(methods.len(), 1, "Impl should have 1 method");
                    }
                }
                _ => {}
            }
        }

        assert!(found_trait, "Should find trait block");
        assert!(found_struct, "Should find struct block");
        assert!(found_impl, "Should find impl block");
    });
}

// ============================================================================
// Block Relations Tests
// ============================================================================

#[serial]
#[test]
fn test_block_relations_has_field() {
    use llmcc_core::block::BlockRelation;

    let source = dedent(
        "
        struct Config {
            debug: bool,
            level: u32,
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);
        let graph = &graphs[0];

        let root = get_block(cc, graph.root());
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Class {
                // Check HasField relations
                let fields = graph.edges().get_related(child_id, BlockRelation::HasField);
                assert_eq!(fields.len(), 2, "Struct should have HasField edges to 2 fields");

                // Check reverse FieldOf relations
                for &field_id in &fields {
                    let owners = graph.edges().get_related(field_id, BlockRelation::FieldOf);
                    assert!(
                        owners.contains(&child_id),
                        "Field should have FieldOf edge to struct"
                    );
                }
            }
        }
    });
}

#[serial]
#[test]
fn test_block_relations_has_method() {
    use llmcc_core::block::BlockRelation;

    let source = dedent(
        "
        struct Calculator;

        impl Calculator {
            fn add(&self, a: i32, b: i32) -> i32 { a + b }
            fn sub(&self, a: i32, b: i32) -> i32 { a - b }
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);
        let graph = &graphs[0];

        let root = get_block(cc, graph.root());
        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Impl {
                // Check HasMethod relations
                let methods = graph.edges().get_related(child_id, BlockRelation::HasMethod);
                assert_eq!(methods.len(), 2, "Impl should have HasMethod edges to 2 methods");

                // Check reverse MethodOf relations
                for &method_id in &methods {
                    let owners = graph.edges().get_related(method_id, BlockRelation::MethodOf);
                    assert!(
                        owners.contains(&child_id),
                        "Method should have MethodOf edge to impl"
                    );
                }
            }
        }
    });
}

#[serial]
#[test]
fn test_block_relations_implements() {
    use llmcc_core::block::BlockRelation;

    let source = dedent(
        "
        struct Widget {
            id: u32,
        }

        impl Widget {
            fn new(id: u32) -> Self {
                Widget { id }
            }
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);
        let graph = &graphs[0];

        let root = get_block(cc, graph.root());
        let mut struct_id = None;
        let mut impl_id = None;

        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            match child.kind() {
                BlockKind::Class => struct_id = Some(child_id),
                BlockKind::Impl => impl_id = Some(child_id),
                _ => {}
            }
        }

        if let (Some(s_id), Some(i_id)) = (struct_id, impl_id) {
            // Check Implements relation from impl to struct
            let targets = graph.edges().get_related(i_id, BlockRelation::Implements);
            assert!(
                targets.contains(&s_id),
                "Impl should have Implements edge to struct"
            );

            // Check ImplementedBy relation from struct to impl
            let impls = graph.edges().get_related(s_id, BlockRelation::ImplementedBy);
            assert!(
                impls.contains(&i_id),
                "Struct should have ImplementedBy edge to impl"
            );
        }
    });
}

// ============================================================================
// Complex Scenario Tests
// ============================================================================

#[serial]
#[test]
fn test_block_complex_struct_with_multiple_impls() {
    let source = dedent(
        "
        struct Database {
            connection: String,
        }

        impl Database {
            fn connect(url: &str) -> Self {
                Database { connection: url.to_string() }
            }

            fn disconnect(&mut self) {
                self.connection.clear();
            }
        }

        impl Database {
            fn query(&self, sql: &str) -> Vec<String> {
                vec![]
            }

            fn execute(&self, sql: &str) -> bool {
                true
            }
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        let mut impl_count = 0;
        let mut total_methods_in_struct = 0;

        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Impl {
                impl_count += 1;
            }
            if let Some(class) = child.as_class() {
                total_methods_in_struct = class.get_methods().len();
            }
        }

        assert_eq!(impl_count, 2, "Should have 2 impl blocks");
        // Struct should have all 4 methods from both impls
        assert_eq!(
            total_methods_in_struct, 4,
            "Struct should have 4 methods total from both impls"
        );
    });
}

#[serial]
#[test]
fn test_block_nested_types() {
    let source = dedent(
        "
        struct Outer {
            inner: Inner,
        }

        struct Inner {
            value: i32,
        }

        impl Outer {
            fn get_value(&self) -> i32 {
                self.inner.value
            }
        }
        ",
    );

    with_graph(&[&source], |cc, graphs| {
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        let mut struct_count = 0;
        let mut impl_count = 0;

        for &child_id in root.children() {
            let child = get_block(cc, child_id);
            match child.kind() {
                BlockKind::Class => struct_count += 1,
                BlockKind::Impl => impl_count += 1,
                _ => {}
            }
        }

        assert_eq!(struct_count, 2, "Should have 2 struct blocks");
        assert_eq!(impl_count, 1, "Should have 1 impl block");
    });
}
