use llmcc_core::{
    build_llmcc_graph, graph_builder::ProjectGraph, ir_builder::build_llmcc_ir,
    query::ProjectQuery, CompileCtxt, GraphBuildConfig, IrBuildConfig,
};
use llmcc_resolver::apply_collected_symbols;
use llmcc_rust::{bind_symbols, collect_symbols, LangRust};

/// Helper to build a project graph from multiple Rust source files
fn build_graph(sources: &[&str]) -> &'static ProjectGraph<'static> {
    let source_bytes: Vec<Vec<u8>> = sources.iter().map(|s| s.as_bytes().to_vec()).collect();

    let cc = Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(
        &source_bytes,
    )));

    build_llmcc_ir::<LangRust>(cc, IrBuildConfig).unwrap();

    let globals = cc.create_globals();
    let unit_count = sources.len();
    let mut collections = Vec::new();
    let mut graph = ProjectGraph::new(cc);

    for unit_idx in 0..unit_count {
        let unit = graph.cc.compile_unit(unit_idx);
        build_llmcc_ir::<LangRust>(cc, IrBuildConfig).unwrap();
        let collection = collect_symbols(unit, globals);
        apply_collected_symbols(unit, globals, &collection);
        collections.push(collection);
    }

    for (unit_idx, collection) in collections.iter().enumerate().take(unit_count) {
        let unit = graph.cc.compile_unit(unit_idx);
        bind_symbols(unit, globals, collection);

        let unit_graph = build_llmcc_graph::<LangRust>(unit, unit_idx, GraphBuildConfig).unwrap();
        graph.add_child(unit_graph);
    }

    graph.link_units();

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

    let query = ProjectQuery::new(graph);
    let results = query.find_by_name("helper");

    // Should find the helper function
    assert!(!results.primary.is_empty(), "Should find 'helper' function");
    assert_eq!(results.primary[0].name, "helper");
    assert_eq!(results.primary[0].kind, "Func");

    let formatted = results.format_for_llm();
    assert!(
        formatted.contains("helper"),
        "Output should contain function name"
    );
    assert!(
        formatted.contains("[Func]"),
        "Output should contain function type"
    );
}

/// Test 2: Query result is consistent across calls
#[test]
fn test_query_consistency() {
    let graph = build_graph(&[r#"
        fn test_func() {}
    "#]);

    let query = ProjectQuery::new(graph);
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

    let query = ProjectQuery::new(graph);
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

    let query = ProjectQuery::new(graph);
    let results = query.find_all_functions();

    // Should have at least first and second functions
    assert!(
        results.primary.len() >= 2,
        "Should find at least 2 functions"
    );

    // Verify functions are found
    let func_names: Vec<&str> = results.primary.iter().map(|f| f.name.as_str()).collect();
    assert!(
        func_names.contains(&"first"),
        "Should find 'first' function"
    );
    assert!(
        func_names.contains(&"second"),
        "Should find 'second' function"
    );

    // Verify output format
    let formatted = results.format_for_llm();
    assert!(formatted.contains("first"), "Output should contain 'first'");
    assert!(
        formatted.contains("second"),
        "Output should contain 'second'"
    );
    assert!(
        formatted.contains("[Func]"),
        "Output should contain function type"
    );
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

    let query = ProjectQuery::new(graph);

    // Should be able to query both
    let results0 = query.find_by_name("file0_func");
    let results1 = query.find_by_name("file1_func");

    // Both queries should find results
    assert!(
        !results0.primary.is_empty(),
        "Should find 'file0_func' from unit 0"
    );
    assert!(
        !results1.primary.is_empty(),
        "Should find 'file1_func' from unit 1"
    );

    assert_eq!(results0.primary[0].name, "file0_func");
    assert_eq!(results1.primary[0].name, "file1_func");

    // Check they're in different units
    assert_eq!(results0.primary[0].unit_index, 0);
    assert_eq!(results1.primary[0].unit_index, 1);
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

    let query = ProjectQuery::new(graph);
    let results = query.file_structure(0);
    let results_u1 = query.file_structure(1);

    // Unit 0 should have ConfigA and handler_a
    assert!(!results.primary.is_empty(), "Unit 0 should have items");
    let names_u0: Vec<&str> = results.primary.iter().map(|b| b.name.as_str()).collect();
    assert!(
        names_u0.contains(&"ConfigA"),
        "Unit 0 should contain ConfigA"
    );
    assert!(
        names_u0.contains(&"handler_a"),
        "Unit 0 should contain handler_a"
    );

    // Unit 1 should have ConfigB and handler_b
    assert!(!results_u1.primary.is_empty(), "Unit 1 should have items");
    let names_u1: Vec<&str> = results_u1.primary.iter().map(|b| b.name.as_str()).collect();
    assert!(
        names_u1.contains(&"ConfigB"),
        "Unit 1 should contain ConfigB"
    );
    assert!(
        names_u1.contains(&"handler_b"),
        "Unit 1 should contain handler_b"
    );
}

/// Test 7: Find depends blocks (what this function depends on)
#[test]
fn test_query_find_related() {
    let graph = build_graph(&[r#"
        fn dep() {}
        fn caller() {
            dep();
        }
    "#]);

    let query = ProjectQuery::new(graph);
    let results = query.find_depends("caller");

    // Should find caller as primary result
    assert!(!results.primary.is_empty(), "Should find 'caller' function");
    assert_eq!(results.primary[0].name, "caller");

    // Should find dep as depends block (caller depends on dep)
    assert!(
        !results.depends.is_empty(),
        "Should find 'dep' function that caller depends on"
    );
    let depends_names: Vec<&str> = results.depends.iter().map(|b| b.name.as_str()).collect();
    assert!(
        depends_names.contains(&"dep"),
        "Depends blocks should contain 'dep'"
    );

    // Verify output includes both
    let formatted = results.format_for_llm();
    assert!(formatted.contains("caller"), "Output should contain caller");
    assert!(formatted.contains("dep"), "Output should contain dep");
    assert!(
        formatted.contains("DEPENDS ON"),
        "Output should have DEPENDS ON section"
    );
}

/// Test 8: Find depends blocks recursively (transitive dependencies)
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

    let query = ProjectQuery::new(graph);
    let results = query.find_depends_recursive("root");

    // Should find root as primary
    assert!(!results.primary.is_empty(), "Should find 'root' function");
    assert_eq!(results.primary[0].name, "root");

    // Should find both middle and leaf as depends (transitively)
    let depends_names: Vec<&str> = results.depends.iter().map(|b| b.name.as_str()).collect();
    assert!(
        depends_names.contains(&"middle"),
        "Should find 'middle' as depends"
    );
    assert!(
        depends_names.contains(&"leaf"),
        "Should find 'leaf' as depends"
    );

    // Verify recursive depth - should have both direct and indirect dependencies
    assert!(
        results.depends.len() >= 2,
        "Should find at least 2 depends blocks"
    );

    let formatted = results.format_for_llm();
    assert!(formatted.contains("root"), "Output should contain root");
    assert!(formatted.contains("middle"), "Output should contain middle");
    assert!(formatted.contains("leaf"), "Output should contain leaf");
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

    let query = ProjectQuery::new(graph);
    let traversal = query.traverse_bfs("root");

    // Should find root first (BFS order)
    assert!(!traversal.is_empty(), "BFS traversal should find nodes");
    assert_eq!(traversal[0].name, "root", "First node should be root");

    // Verify all nodes are found
    let names: Vec<&str> = traversal.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"root"), "Should contain root");
    assert!(names.contains(&"middle"), "Should contain middle");
    assert!(names.contains(&"leaf"), "Should contain leaf");

    // BFS means we visit breadth-first: root -> middle -> leaf
    assert!(
        names.windows(2).any(|w| w[0] == "root" && w[1] == "middle"),
        "BFS should visit middle before leaf"
    );
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

    let query = ProjectQuery::new(graph);
    let traversal = query.traverse_dfs("root");

    // Should find root first
    assert!(!traversal.is_empty(), "DFS traversal should find nodes");
    assert_eq!(traversal[0].name, "root", "First node should be root");

    // Verify all nodes are found
    let names: Vec<&str> = traversal.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"root"), "Should contain root");
    assert!(names.contains(&"middle"), "Should contain middle");
    assert!(names.contains(&"leaf"), "Should contain leaf");
}

/// Test 11: Query formatting includes header when results exist
#[test]
fn test_query_format_headers() {
    let graph = build_graph(&[r#"
        fn sample() {}
    "#]);

    let query = ProjectQuery::new(graph);
    let results = query.find_by_name("sample");

    let formatted = results.format_for_llm();

    // Should have results since we added a function
    assert!(!results.primary.is_empty(), "Should find 'sample' function");

    // Check that formatted output contains primary section header
    assert!(
        formatted.contains("ASK SYMBOL"),
        "Formatted output should contain ASK SYMBOL header"
    );

    // Check that the function details are present
    assert!(
        formatted.contains("sample"),
        "Output should contain function name"
    );
    assert!(
        formatted.contains("[Func]"),
        "Output should contain function type marker"
    );
}

/// Test 12: Large source file
#[test]
fn test_query_large_source() {
    let mut source = String::new();
    for i in 0..50 {
        source.push_str(&format!("fn func_{}() {{}}\n", i));
    }

    let graph = build_graph(&[&source]);
    let query = ProjectQuery::new(graph);

    let results = query.find_all_functions();

    // Should find all 50 functions
    assert!(
        results.primary.len() >= 50,
        "Should find at least 50 functions"
    );

    // Verify some specific functions are found
    let names: Vec<&str> = results.primary.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"func_0"), "Should find func_0");
    assert!(names.contains(&"func_25"), "Should find func_25");
    assert!(names.contains(&"func_49"), "Should find func_49");

    // Test formatting works with large results
    let formatted = results.format_for_llm();
    assert!(
        !formatted.is_empty(),
        "Formatted output should not be empty"
    );
    assert!(
        formatted.len() > 1000,
        "Output should be substantial for large source"
    );
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

    let query = ProjectQuery::new(graph);

    // Query different things
    let func_results = query.find_by_name("process");
    let struct_results = query.find_by_name("Container");
    let _const_results = query.find_by_name("MAX_SIZE");

    // All should be queryable and find results
    assert!(
        !func_results.primary.is_empty(),
        "Should find 'process' function"
    );
    assert_eq!(func_results.primary[0].name, "process");
    assert_eq!(func_results.primary[0].kind, "Func");

    assert!(
        !struct_results.primary.is_empty(),
        "Should find 'Container' struct"
    );
    assert_eq!(struct_results.primary[0].name, "Container");
    assert_eq!(struct_results.primary[0].kind, "Class");

    // Verify both have source code captured
    assert!(
        func_results.primary[0].source_code.is_some(),
        "Function should have source code"
    );
    assert!(
        struct_results.primary[0].source_code.is_some(),
        "Struct should have source code"
    );
}

/// Test 14: Query result inspection
#[test]
fn test_query_result_inspection() {
    let graph = build_graph(&[r#"
        fn test() {}
    "#]);

    let query = ProjectQuery::new(graph);
    let results = query.find_by_name("test");

    // Should be inspectable and contain data
    assert!(
        !results.primary.is_empty(),
        "Primary results should not be empty"
    );

    // Inspect the primary result
    let test_block = &results.primary[0];
    assert_eq!(test_block.name, "test");
    assert_eq!(test_block.kind, "Func");
    assert!(test_block.start_line > 0, "Start line should be positive");
    assert!(
        test_block.end_line >= test_block.start_line,
        "End line should be >= start line"
    );
    assert!(
        test_block.source_code.is_some(),
        "Source code should be available"
    );

    // Should have metadata
    assert_eq!(
        results.depends.len(),
        0,
        "Should have no depends blocks for simple function"
    );
    assert_eq!(
        results.depended.len(),
        0,
        "Should have no depended blocks for simple function"
    );
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

    let query = ProjectQuery::new(graph);

    // Run multiple queries
    let a_result = query.find_by_name("a");
    let b_result = query.find_by_name("b");
    let c_result = query.find_by_name("c");
    let d_result = query.find_by_name("D");
    let funcs = query.find_all_functions();

    // All queries should succeed
    assert!(!a_result.primary.is_empty(), "Should find function 'a'");
    assert!(!b_result.primary.is_empty(), "Should find function 'b'");
    assert!(!c_result.primary.is_empty(), "Should find function 'c'");
    assert!(!d_result.primary.is_empty(), "Should find struct 'D'");
    assert!(funcs.primary.len() >= 3, "Should find at least 3 functions");

    // Verify each result is independent
    assert_eq!(a_result.primary[0].name, "a");
    assert_eq!(b_result.primary[0].name, "b");
    assert_eq!(c_result.primary[0].name, "c");
}

/// Test 16: Query result format consistency
#[test]
fn test_query_result_format_consistency() {
    let graph = build_graph(&[r#"
        fn sample() {}
    "#]);

    let query = ProjectQuery::new(graph);
    let results = query.find_by_name("sample");

    // Multiple format calls should return identical results
    let fmt1 = results.format_for_llm();
    let fmt2 = results.format_for_llm();
    let fmt3 = results.format_for_llm();

    assert_eq!(fmt1, fmt2);
    assert_eq!(fmt2, fmt3);
}

/// Test 17: Find depended (blocks that depend on this block)
#[test]
fn test_query_find_depended() {
    let graph = build_graph(&[r#"
        fn helper() {}
        fn caller() {
            helper();
        }
    "#]);

    let query = ProjectQuery::new(graph);
    let results = query.find_depended("helper");

    // Should find helper as primary result
    assert!(!results.primary.is_empty(), "Should find 'helper' function");
    assert_eq!(results.primary[0].name, "helper");

    // Should find caller as depended block (caller depends on helper)
    assert!(
        !results.depended.is_empty(),
        "Should find 'caller' that depends on helper"
    );
    let depended_names: Vec<&str> = results.depended.iter().map(|b| b.name.as_str()).collect();
    assert!(
        depended_names.contains(&"caller"),
        "Depended blocks should contain 'caller'"
    );

    // Verify output includes both
    let formatted = results.format_for_llm();
    assert!(formatted.contains("helper"), "Output should contain helper");
    assert!(formatted.contains("caller"), "Output should contain caller");
    assert!(
        formatted.contains("DEPENDED BY"),
        "Output should have DEPENDED BY section"
    );
}
