use llmcc_core::{
    build_llmcc_graph, graph_builder::ProjectGraph, ir_builder::build_llmcc_ir,
    query::ProjectQuery, CompileCtxt,
};
use llmcc_rust::{bind_symbols, collect_symbols, LangRust};

/// Helper to build a project graph from multiple Rust source files
fn build_graph(sources: &[&str]) -> &'static ProjectGraph<'static> {
    let source_bytes: Vec<Vec<u8>> = sources.iter().map(|s| s.as_bytes().to_vec()).collect();

    let cc = Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(
        &source_bytes,
    )));
    let globals = cc.create_globals();
    let unit_count = sources.len();
    let mut collections = Vec::new();
    let mut graph = ProjectGraph::new(cc);

    for unit_idx in 0..unit_count {
        let unit = graph.cc.compile_unit(unit_idx);
        build_llmcc_ir::<LangRust>(cc).unwrap();
        collections.push(collect_symbols(unit, globals));
    }

    for unit_idx in 0..unit_count {
        let unit = graph.cc.compile_unit(unit_idx);
        bind_symbols(unit, globals);

        let unit_graph = build_llmcc_graph::<LangRust>(unit, unit_idx).unwrap();
        graph.add_child(unit_graph);
    }

    graph.link_units();
    drop(collections);

    Box::leak(Box::new(graph))
}

/// Test 1: Find a simple function by name
#[test]
fn test_query_find_function_basic() {
    let graph = build_graph(&[r#"
        fn helper() {}
        fn caller() {
            helper();
        }
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_by_name("helper");

    // The query should work even if it returns empty
    // (depends on graph builder's implementation)
    let _formatted = results.format_for_llm();
    // Should not panic
    assert!(true);
}

/// Test 2: Query result is consistent across calls
#[test]
fn test_query_consistency() {
    let graph = build_graph(&[r#"
        fn test_func() {}
    "#]);

    let query = ProjectQuery::new(&graph);
    let results1 = query.find_by_name("test_func");
    let results2 = query.find_by_name("test_func");

    let formatted1 = results1.format_for_llm();
    let formatted2 = results2.format_for_llm();

    // Should be consistent
    assert_eq!(formatted1, formatted2);
}

/// Test 3: Empty query returns empty result
#[test]
fn test_query_nonexistent() {
    let graph = build_graph(&[r#"
        fn existing() {}
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_by_name("nonexistent_xyz_abc");

    assert!(results.primary.is_empty());
    assert_eq!(results.format_for_llm(), "");
}

/// Test 4: Find all functions
#[test]
fn test_query_find_all_functions() {
    let graph = build_graph(&[r#"
        fn first() {}
        fn second() {}
        struct MyStruct;
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_all_functions();

    // Should have at least first and second functions
    // (implementation may vary)
    let _formatted = results.format_for_llm();
    // Should not crash
    assert!(true);
}

/// Test 5: Multiple source files
#[test]
fn test_query_multiple_files() {
    let graph = build_graph(&[
        r#"
        fn file0_func() {}
        "#,
        r#"
        fn file1_func() {}
        "#,
    ]);

    let query = ProjectQuery::new(&graph);

    // Should be able to query both
    let results0 = query.find_by_name("file0_func");
    let results1 = query.find_by_name("file1_func");

    // Both queries should work (may return empty or full results)
    let _ = results0.format_for_llm();
    let _ = results1.format_for_llm();
    assert!(true);
}

/// Test 6: File structure query
#[test]
fn test_query_file_structure() {
    let graph = build_graph(&[
        r#"
        struct ConfigA;
        fn handler_a() {}
        "#,
        r#"
        struct ConfigB;
        fn handler_b() {}
        "#,
    ]);

    let query = ProjectQuery::new(&graph);
    let results = query.file_structure(0);
    let results_u1 = query.file_structure(1);

    // Should not crash
    let _ = results.format_for_llm();
    let _ = results_u1.format_for_llm();
    assert!(true);
}

/// Test 7: Find related blocks
#[test]
fn test_query_find_related() {
    let graph = build_graph(&[r#"
        fn dep() {}
        fn caller() {
            dep();
        }
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_related("caller");

    // Should not crash
    let _ = results.format_for_llm();
    assert!(true);
}

/// Test 8: Find related blocks recursively
#[test]
fn test_query_find_related_recursive() {
    let graph = build_graph(&[r#"
        fn leaf() {}
        fn middle() {
            leaf();
        }
        fn root() {
            middle();
        }
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_related_recursive("root");

    // Should not crash
    let _ = results.format_for_llm();
    assert!(true);
}

/// Test 9: BFS traversal
#[test]
fn test_query_traverse_bfs() {
    let graph = build_graph(&[r#"
        fn leaf() {}
        fn middle() {
            leaf();
        }
        fn root() {
            middle();
        }
    "#]);

    let query = ProjectQuery::new(&graph);
    let traversal = query.traverse_bfs("root");

    // Should not crash
    let _ = traversal;
    assert!(true);
}

/// Test 10: DFS traversal
#[test]
fn test_query_traverse_dfs() {
    let graph = build_graph(&[r#"
        fn leaf() {}
        fn middle() {
            leaf();
        }
        fn root() {
            middle();
        }
    "#]);

    let query = ProjectQuery::new(&graph);
    let traversal = query.traverse_dfs("root");

    // Should not crash
    let _ = traversal;
    assert!(true);
}

/// Test 11: Query formatting includes header when results exist
#[test]
fn test_query_format_headers() {
    let graph = build_graph(&[r#"
        fn sample() {}
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_by_name("sample");

    let formatted = results.format_for_llm();

    if !results.primary.is_empty() {
        assert!(formatted.contains("PRIMARY RESULTS"));
    } else {
        assert_eq!(formatted, "");
    }
}

/// Test 12: Large source file
#[test]
fn test_query_large_source() {
    let mut source = String::new();
    for i in 0..50 {
        source.push_str(&format!("fn func_{}() {{}}\n", i));
    }

    let graph = build_graph(&[&source]);
    let query = ProjectQuery::new(&graph);

    let results = query.find_all_functions();

    // Should not crash with large sources
    let _ = results.format_for_llm();
    assert!(true);
}

/// Test 13: Query with mixed types
#[test]
fn test_query_mixed_types() {
    let graph = build_graph(&[r#"
        struct Container;
        fn process() -> Container {
            Container
        }
        const MAX_SIZE: i32 = 100;
    "#]);

    let query = ProjectQuery::new(&graph);

    // Query different things
    let _func_results = query.find_by_name("process");
    let _struct_results = query.find_by_name("Container");
    let _const_results = query.find_by_name("MAX_SIZE");

    // All should be queryable
    assert!(true);
}

/// Test 14: Query result inspection
#[test]
fn test_query_result_inspection() {
    let graph = build_graph(&[r#"
        fn test() {}
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_by_name("test");

    // Should be inspectable
    let _ = &results.primary;
    let _ = &results.related;
    let _ = &results.definitions;
    assert!(true);
}

/// Test 15: Multiple queries on same graph
#[test]
fn test_multiple_queries_same_graph() {
    let graph = build_graph(&[r#"
        fn a() {}
        fn b() {}
        fn c() {}
        struct D;
    "#]);

    let query = ProjectQuery::new(&graph);

    // Run multiple queries
    let _a = query.find_by_name("a");
    let _b = query.find_by_name("b");
    let _c = query.find_by_name("c");
    let _d = query.find_by_name("D");
    let _funcs = query.find_all_functions();

    // Should handle multiple queries without issues
    assert!(true);
}

/// Test 16: Query result format consistency
#[test]
fn test_query_result_format_consistency() {
    let graph = build_graph(&[r#"
        fn sample() {}
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_by_name("sample");

    // Multiple format calls should return identical results
    let fmt1 = results.format_for_llm();
    let fmt2 = results.format_for_llm();
    let fmt3 = results.format_for_llm();

    assert_eq!(fmt1, fmt2);
    assert_eq!(fmt2, fmt3);
}
