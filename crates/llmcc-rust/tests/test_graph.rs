mod common;

use llmcc_core::block::{BasicBlock, BlockKind, BlockRelation};
use llmcc_core::graph::ProjectGraph;
use llmcc_core::graph_builder::{GraphBuildOption, build_llmcc_graph};
use llmcc_rust::LangRust;

use common::with_compiled_unit;
use serial_test::serial;
use textwrap::dedent;

/// Helper to build project graph with connected blocks and run checks
fn with_project_graph<F>(sources: &[&str], check: F)
where
    F: for<'a> FnOnce(&'a llmcc_core::context::CompileCtxt<'a>, &ProjectGraph<'a>),
{
    with_compiled_unit(sources, |cc| {
        let graphs = build_llmcc_graph::<LangRust>(cc, GraphBuildOption::default()).unwrap();
        let mut pg = ProjectGraph::new(cc);
        pg.add_children(graphs);
        pg.connect_blocks();
        check(cc, &pg);
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
// Contains / ContainedBy Tests
// ============================================================================

#[serial]
#[test]
fn test_connect_blocks_contains_relationship() {
    let source = dedent(
        "
        fn outer() {
            fn inner() {}
        }
        ",
    );

    with_project_graph(&[&source], |cc, pg| {
        let graphs = pg.units();
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());

        // Find the outer function
        for child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Func {
                // The outer function should contain inner function
                let contained = cc.related_map.get_related(child_id, BlockRelation::Contains);
                assert!(!contained.is_empty(), "outer func should have Contains edges");

                // Check ContainedBy for children
                for &inner_id in &contained {
                    let parents = cc.related_map.get_related(inner_id, BlockRelation::ContainedBy);
                    assert!(
                        parents.contains(&child_id),
                        "inner block should have ContainedBy edge to parent"
                    );
                }
            }
        }
    });
}

// ============================================================================
// HasField / FieldOf Tests
// ============================================================================

#[serial]
#[test]
fn test_connect_blocks_has_field() {
    let source = dedent(
        "
        struct Config {
            debug: bool,
            level: u32,
        }
        ",
    );

    with_project_graph(&[&source], |cc, pg| {
        let graphs = pg.units();
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        for child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Class {
                // Check HasField relations
                let fields = cc.related_map.get_related(child_id, BlockRelation::HasField);
                assert_eq!(fields.len(), 2, "Struct should have HasField edges to 2 fields");

                // Check reverse FieldOf relations
                for &field_id in &fields {
                    let owners = cc.related_map.get_related(field_id, BlockRelation::FieldOf);
                    assert!(
                        owners.contains(&child_id),
                        "Field should have FieldOf edge to struct"
                    );
                }
            }
        }
    });
}

// ============================================================================
// HasMethod / MethodOf Tests
// ============================================================================

#[serial]
#[test]
fn test_connect_blocks_has_method() {
    let source = dedent(
        "
        struct Calculator;

        impl Calculator {
            fn add(&self, a: i32, b: i32) -> i32 { a + b }
            fn sub(&self, a: i32, b: i32) -> i32 { a - b }
        }
        ",
    );

    with_project_graph(&[&source], |cc, pg| {
        let graphs = pg.units();
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        for child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Impl {
                // Check HasMethod relations
                let methods = cc.related_map.get_related(child_id, BlockRelation::HasMethod);
                assert_eq!(methods.len(), 2, "Impl should have HasMethod edges to 2 methods");

                // Check reverse MethodOf relations
                for &method_id in &methods {
                    let owners = cc.related_map.get_related(method_id, BlockRelation::MethodOf);
                    assert!(
                        owners.contains(&child_id),
                        "Method should have MethodOf edge to impl"
                    );
                }
            }
        }
    });
}

// ============================================================================
// HasParameters / HasReturn Tests
// ============================================================================

#[serial]
#[test]
fn test_connect_blocks_function_parameters() {
    let source = dedent(
        "
        fn compute(x: i32, y: i32) -> i32 {
            x + y
        }
        ",
    );

    with_project_graph(&[&source], |cc, pg| {
        let graphs = pg.units();
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        let mut found_func = false;

        for child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Func {
                found_func = true;

                // Check HasParameters relation
                let params = cc.related_map.get_related(child_id, BlockRelation::HasParameters);
                assert!(!params.is_empty(), "Function should have HasParameters edge");

                // Check HasReturn relation
                let returns = cc.related_map.get_related(child_id, BlockRelation::HasReturn);
                assert!(!returns.is_empty(), "Function with return type should have HasReturn edge");
            }
        }

        assert!(found_func, "Should have found a function");
    });
}

// ============================================================================
// Trait HasMethod Tests
// ============================================================================

#[serial]
#[test]
fn test_connect_blocks_trait_methods() {
    let source = dedent(
        "
        trait Drawable {
            fn draw(&self);
            fn resize(&mut self, width: u32, height: u32);
        }
        ",
    );

    with_project_graph(&[&source], |cc, pg| {
        let graphs = pg.units();
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        for child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Trait {
                // Check HasMethod relations
                let methods = cc.related_map.get_related(child_id, BlockRelation::HasMethod);
                assert_eq!(methods.len(), 2, "Trait should have HasMethod edges to 2 methods");

                // Check reverse MethodOf relations
                for &method_id in &methods {
                    let owners = cc.related_map.get_related(method_id, BlockRelation::MethodOf);
                    assert!(
                        owners.contains(&child_id),
                        "Method should have MethodOf edge to trait"
                    );
                }
            }
        }
    });
}

// ============================================================================
// Enum Variants Tests
// ============================================================================

#[serial]
#[test]
fn test_connect_blocks_enum_variants() {
    let source = dedent(
        "
        enum Color {
            Red,
            Green,
            Blue,
        }
        ",
    );

    with_project_graph(&[&source], |cc, pg| {
        let graphs = pg.units();
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        for child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Enum {
                // Variants should be linked as HasField
                let variants = cc.related_map.get_related(child_id, BlockRelation::HasField);
                assert_eq!(variants.len(), 3, "Enum should have HasField edges to 3 variants");

                // Check reverse FieldOf relations
                for &variant_id in &variants {
                    let owners = cc.related_map.get_related(variant_id, BlockRelation::FieldOf);
                    assert!(
                        owners.contains(&child_id),
                        "Variant should have FieldOf edge to enum"
                    );
                }
            }
        }
    });
}

// ============================================================================
// Impl/Type Relationship Tests
// ============================================================================

#[serial]
#[test]
fn test_connect_blocks_impl_for_struct() {
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

    with_project_graph(&[&source], |cc, pg| {
        let graphs = pg.units();
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        let mut struct_id = None;
        let mut impl_id = None;

        for child_id in root.children() {
            let child = get_block(cc, child_id);
            match child.kind() {
                BlockKind::Class => struct_id = Some(child_id),
                BlockKind::Impl => impl_id = Some(child_id),
                _ => {}
            }
        }

        if let (Some(s_id), Some(i_id)) = (struct_id, impl_id) {
            // Check ImplFor relation from impl to struct
            let targets = cc.related_map.get_related(i_id, BlockRelation::ImplFor);
            assert!(
                targets.contains(&s_id),
                "Impl should have ImplFor edge to struct"
            );

            // Check HasImpl relation from struct to impl
            let impls = cc.related_map.get_related(s_id, BlockRelation::HasImpl);
            assert!(
                impls.contains(&i_id),
                "Struct should have HasImpl edge to impl"
            );
        }
    });
}

// ============================================================================
// Multi-Unit Tests (Cross-File Relationships)
// ============================================================================

#[serial]
#[test]
fn test_connect_blocks_multiple_units() {
    let source1 = dedent(
        "
        struct TypeA {
            value: i32,
        }
        ",
    );

    let source2 = dedent(
        "
        struct TypeB {
            count: u32,
        }
        ",
    );

    with_project_graph(&[&source1, &source2], |cc, pg| {
        let graphs = pg.units();
        assert_eq!(graphs.len(), 2, "Should have 2 unit graphs");

        // Each unit should have its own root and children
        for graph in graphs {
            let root = get_block(cc, graph.root());
            assert!(root.kind() == BlockKind::Root, "Each unit should have a Root block");
            assert!(!root.children().is_empty(), "Each unit should have at least one child");
        }
    });
}

// ============================================================================
// Complex Nested Structure Tests
// ============================================================================

#[serial]
#[test]
fn test_connect_blocks_nested_structs() {
    let source = dedent(
        "
        struct Outer {
            inner: Inner,
        }

        struct Inner {
            value: i32,
        }
        ",
    );

    with_project_graph(&[&source], |cc, pg| {
        let graphs = pg.units();
        assert_eq!(graphs.len(), 1);

        let root = get_block(cc, graphs[0].root());
        let mut struct_count = 0;

        for child_id in root.children() {
            let child = get_block(cc, child_id);
            if child.kind() == BlockKind::Class {
                struct_count += 1;

                // Each struct should have HasField relationships
                let fields = cc.related_map.get_related(child_id, BlockRelation::HasField);
                assert!(!fields.is_empty(), "Struct should have at least one field");
            }
        }

        assert_eq!(struct_count, 2, "Should have 2 structs");
    });
}

#[serial]
#[test]
fn test_type_alias_block() {
    let source = dedent(
        "
        type MyInt = i32;
        type MyPair = (i32, String);
        fn use_alias() -> MyInt { 42 }
        ",
    );

    with_project_graph(&[&source], |cc, pg| {
        // Dump all blocks first
        for i in 0..10 {
            if let Some(blk) = cc.block_arena.bb().get(i) {
                eprintln!("DEBUG block[{}]: kind={:?}", i, blk.kind());
            }
        }

        // Find alias blocks
        let mut alias_count = 0;
        let mut my_int_id = None;
        let graphs = pg.units();
        let root = get_block(cc, graphs[0].root());

        for child_id in root.children() {
            let child = get_block(cc, child_id);
            eprintln!("DEBUG child: {:?} kind={:?}", child_id, child.kind());
            if child.kind() == BlockKind::Alias {
                alias_count += 1;
                if my_int_id.is_none() {
                    my_int_id = Some(child_id);
                }
            }
            if child.kind() == BlockKind::Func {
                // Check func's children
                for sub_id in child.children() {
                    let sub = get_block(cc, sub_id);
                    eprintln!("DEBUG func child: {:?} kind={:?}", sub_id, sub.kind());
                    if let BasicBlock::Return(ret) = &sub {
                        eprintln!("DEBUG return type_name: {:?} type_ref: {:?}",
                            ret.base.get_type_name(), ret.base.get_type_ref());
                    }
                }
                // Check if the return type references the alias via TypeOf
                let has_return = cc.related_map.get_related(child_id, BlockRelation::HasReturn);
                eprintln!("DEBUG use_alias has_return: {:?}", has_return);
                for ret_id in has_return {
                    let type_of = cc.related_map.get_related(ret_id, BlockRelation::TypeOf);
                    eprintln!("DEBUG return type_of: {:?}", type_of);
                }
            }
        }

        assert_eq!(alias_count, 2, "Should have 2 type aliases");

        // Check if MyInt alias is referenced
        if let Some(alias_id) = my_int_id {
            let type_for = cc.related_map.get_related(alias_id, BlockRelation::TypeFor);
            eprintln!("DEBUG MyInt type_for: {:?}", type_for);
        }
    });
}
