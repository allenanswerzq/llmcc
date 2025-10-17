use std::collections::HashSet;

use llmcc_core::{build_llmcc_graph, graph_builder::{BlockKind, GraphNode, ProjectGraph}};
use llmcc_rust::{bind_symbols, build_llmcc_ir, collect_symbols, CompileCtxt, LangRust};

/// Helper to build a project graph from multiple Rust source files
/// Each source becomes a separate compilation unit in the graph
fn build_graph(sources: &[&str]) -> ProjectGraph<'static> {
    let source_bytes: Vec<Vec<u8>> = sources
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect();

    let cc = Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(&source_bytes)));
    let globals = cc.create_globals();
    let unit_count = sources.len();
    let mut collections = Vec::new();
    let mut graph = ProjectGraph::new(cc);

    for unit_idx in 0..unit_count {
        let unit = graph.cc.compile_unit(unit_idx);
        build_llmcc_ir::<LangRust>(unit).unwrap();
        collections.push(collect_symbols(unit, globals));
    }

    for unit_idx in 0..unit_count {
        let unit = graph.cc.compile_unit(unit_idx);
        bind_symbols(unit, globals);

        let unit_graph = build_llmcc_graph::<LangRust>(unit, unit_idx).unwrap();
        graph.add_child(unit_graph);
    }

    // Link cross-unit dependencies
    graph.link_units();
    drop(collections);

    graph
}

fn block_name(graph: &ProjectGraph<'static>, node: GraphNode) -> Option<String> {
    graph.block_info(node.block_id).and_then(|(_, name, _)| name)
}

#[test]
fn collects_function_blocks_in_unit() {
    let graph = build_graph(&[r#"
        fn helper() {}

        fn caller() {
            helper();
        }
    "#]);

    let helper = graph.block_by_name("helper").expect("helper block");
    let caller = graph.block_by_name("caller").expect("caller block");

    assert_eq!(helper.unit_index, 0);
    assert_eq!(caller.unit_index, 0);

    let helper_info = graph.block_info(helper.block_id).unwrap();
    assert_eq!(helper_info.2, BlockKind::Func);

    let caller_info = graph.block_info(caller.block_id).unwrap();
    assert_eq!(caller_info.2, BlockKind::Func);

    let unit_blocks: HashSet<_> = graph
        .blocks_in(0)
        .into_iter()
        .filter_map(|node| block_name(&graph, node))
        .collect();
    assert_eq!(unit_blocks.len(), 2);
    assert!(unit_blocks.contains("helper"));
    assert!(unit_blocks.contains("caller"));

    let call_nodes = graph.blocks_by_kind_in(BlockKind::Call, 0);
    assert_eq!(call_nodes.len(), 1);

    // The caller function should depend on helper
    let caller_dependencies: HashSet<_> = graph
        .get_block_depends(caller)
        .into_iter()
        .filter_map(|node| block_name(&graph, node))
        .collect();

    assert_eq!(caller_dependencies, HashSet::from(["helper".to_string()]));
}

#[test]
fn finds_transitive_dependencies() {
    let graph = build_graph(&[r#"
        fn leaf() {}

        fn middle_a() {
            leaf();
        }

        fn middle_b() {
            leaf();
        }

        fn top() {
            middle_a();
            middle_b();
        }
    "#]);

    let top = graph.block_by_name("top").expect("top block");

    // Get all dependencies of top (should be middle_a, middle_b)
    let direct_deps: HashSet<_> = graph
        .get_block_depends(top)
        .into_iter()
        .filter_map(|node| block_name(&graph, node))
        .collect();

    assert!(direct_deps.contains("middle_a"));
    assert!(direct_deps.contains("middle_b"));

    // Get transitive dependencies via find_related_blocks_recursive
    let all_related = graph.find_related_blocks_recursive(top);
    let all_names: HashSet<_> = all_related
        .into_iter()
        .filter_map(|node| block_name(&graph, node))
        .collect();

    assert!(all_names.contains("leaf"));
}

#[test]
fn filters_blocks_by_kind_and_unit() {
    let graph = build_graph(&[r#"
        struct Foo;

        impl Foo {
            fn method(&self) {}
        }

        fn top_level() {}
    "#, r#"
        const VALUE: i32 = 42;

        fn helper() {}
    "#]);

    let unit0_funcs: HashSet<_> = graph
        .blocks_by_kind_in(BlockKind::Func, 0)
        .into_iter()
        .filter_map(|node| block_name(&graph, node))
        .collect();

    assert!(unit0_funcs.contains("top_level"));
    assert!(unit0_funcs.contains("method"));

    let unit1_consts: HashSet<_> = graph
        .blocks_by_kind_in(BlockKind::Const, 1)
        .into_iter()
        .filter_map(|node| block_name(&graph, node))
        .collect();

    assert!(unit1_consts.contains("VALUE"));

    let helper = graph
        .block_by_name_in(1, "helper")
        .expect("helper in unit 1");

    assert_eq!(helper.unit_index, 1);
    let helper_info = graph.block_info(helper.block_id).unwrap();
    assert_eq!(helper_info.2, BlockKind::Func);

    let both_helpers = graph.blocks_by_name("helper");
    assert_eq!(both_helpers.len(), 1);
}
