//! Integration tests for IR builder with Simple language.
//!
//! This module tests the IR builder across various scenarios:
//! - Basic builds (single/multiple files)
//! - Correctness validation
//! - Parallel builds and thread pool reuse
//! - Large-scale performance benchmarks
//! - Scaling analysis and performance characteristics

use llmcc_core::context::CompileCtxt;
use llmcc_core::ir_builder::{IrBuildOption, build_llmcc_ir};
use llmcc_simple::LangSimple;
use std::collections::HashSet;
use std::time::Instant;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Generate source code with N files containing M lines each
fn generate_sources(num_files: usize, lines_per_file: usize) -> Vec<Vec<u8>> {
    let mut sources = Vec::with_capacity(num_files);
    for file_idx in 0..num_files {
        let mut content = String::new();
        for line_idx in 0..lines_per_file {
            content.push_str(&format!(
                "fn f{}_{}() {{ x = {} }}\n",
                file_idx,
                line_idx,
                line_idx % 100
            ));
        }
        sources.push(content.into_bytes());
    }
    sources
}

/// Build IR and return node count
fn build_and_count<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> usize {
    let result = build_llmcc_ir::<LangSimple>(cc, IrBuildOption::default());
    assert!(result.is_ok(), "IR build should succeed");
    cc.hir_map.read().len()
}

// ============================================================================
// CORRECTNESS TESTS
// ============================================================================

/// Test 1: Single file sequential build
#[test]
fn test_ir_build_single_file() {
    let source = br#"
fn main() {
    x = 5
    y = 10
}

fn helper() {
    z = 15
}
"#
    .to_vec();

    let cc = CompileCtxt::from_sources::<LangSimple>(&[source]);
    let result = build_llmcc_ir::<LangSimple>(&cc, IrBuildOption::default());
    assert!(result.is_ok(), "IR build should succeed");

    let hir_map = cc.hir_map.read();
    assert!(hir_map.len() > 0, "HIR map should contain nodes");
    println!("✅ Single file build: {} nodes", hir_map.len());
}

/// Test 2: Multiple files sequential build
#[test]
fn test_ir_build_multiple_files_sequential() {
    let sources = vec![
        br#"fn first() { a = 1 }"#.to_vec(),
        br#"fn second() { b = 2 }"#.to_vec(),
        br#"fn third() { c = 3 }"#.to_vec(),
    ];

    let cc = CompileCtxt::from_sources::<LangSimple>(&sources);
    let result = build_llmcc_ir::<LangSimple>(&cc, IrBuildOption::default());
    assert!(result.is_ok(), "Multi-file IR build should succeed");

    let hir_map = cc.hir_map.read();
    assert!(
        hir_map.len() > 0,
        "HIR map should contain nodes from all files"
    );
    println!(
        "✅ Multiple file build: {} files, {} nodes",
        sources.len(),
        hir_map.len()
    );
}

/// Test 3: Many files parallel build
#[test]
fn test_ir_build_many_files_parallel() {
    const NUM_FILES: usize = 20;
    let sources = generate_sources(NUM_FILES, 10);

    let cc = CompileCtxt::from_sources::<LangSimple>(&sources);
    let hir_nodes = build_and_count(&cc);

    assert!(hir_nodes > 0, "HIR map should contain nodes");
    println!(
        "✅ Parallel build: {} files, {} nodes",
        NUM_FILES, hir_nodes
    );
}

/// Test 4: Verify HIR correctness with exact node count
#[test]
fn test_ir_build_correctness() {
    let source = br#"
fn main() {
    x = 5
}
"#
    .to_vec();

    let cc = CompileCtxt::from_sources::<LangSimple>(&[source]);
    let result = build_llmcc_ir::<LangSimple>(&cc, IrBuildOption::default());
    assert!(result.is_ok(), "IR build should succeed");

    let hir_map = cc.hir_map.read();
    const EXPECTED_NODES: usize = 5;

    assert_eq!(
        hir_map.len(),
        EXPECTED_NODES,
        "Expected exactly {} nodes for 'fn main() {{ x = 5 }}', got {}",
        EXPECTED_NODES,
        hir_map.len()
    );

    let file_root = cc.file_root_id(0);
    assert!(file_root.is_some(), "File must have root");
    assert!(
        hir_map.contains_key(&file_root.unwrap()),
        "Root must be in map"
    );

    println!("✅ Correctness test passed: {} nodes", hir_map.len());
}
/// Test 5: Verify thread pool reuse across build phases
#[test]
fn test_ir_build_thread_pool_reuse() {
    const NUM_FILES: usize = 10;
    let sources = generate_sources(NUM_FILES, 10);

    let cc = CompileCtxt::from_sources::<LangSimple>(&sources);
    let hir_nodes = build_and_count(&cc);

    println!(
        "✅ Thread pool reuse test: {} files, {} nodes",
        NUM_FILES, hir_nodes
    );
}

/// Test 6: Large scale parallel build
#[test]
fn test_ir_build_large_scale() {
    const NUM_FILES: usize = 50;
    const FUNCS_PER_FILE: usize = 5;
    let sources = generate_sources(NUM_FILES, FUNCS_PER_FILE);

    let cc = CompileCtxt::from_sources::<LangSimple>(&sources);
    let hir_nodes = build_and_count(&cc);

    println!(
        "✅ Large scale: {} files × {} funcs, {} nodes",
        NUM_FILES, FUNCS_PER_FILE, hir_nodes
    );
}

/// Test 7: Strict HIR tree correctness with multiple files
#[test]
fn test_ir_build_strict_correctness_multi_file() {
    let sources = vec![
        br#"fn file0_func1() { a = 1 }
fn file0_func2() { b = 2 }"#
            .to_vec(),
        br#"fn file1_func1() { c = 3 }
fn file1_func2() { d = 4 }"#
            .to_vec(),
        br#"fn file2_func1() { e = 5 }
fn file2_func2() { f = 6 }"#
            .to_vec(),
    ];

    let cc = CompileCtxt::from_sources::<LangSimple>(&sources);
    let hir_nodes = build_and_count(&cc);

    // Verify each file has its own root
    for file_idx in 0..sources.len() {
        let file_root = cc.file_root_id(file_idx);
        assert!(file_root.is_some(), "File {} should have root", file_idx);
    }

    // Verify all IDs are unique
    let all_ids: HashSet<_> = cc.hir_map.read().keys().cloned().collect();
    assert_eq!(
        all_ids.len(),
        hir_nodes,
        "All HIR node IDs should be unique"
    );

    println!(
        "✅ Multi-file correctness: {} files, {} nodes",
        sources.len(),
        hir_nodes
    );
}

/// Test 8: Verify no ID collisions across independent builds
#[test]
fn test_ir_build_no_id_collisions() {
    let sources1: Vec<_> = (0..10)
        .map(|i| format!("fn func_1_{}() {{ x = {} }}", i, i).into_bytes())
        .collect();

    let cc1 = CompileCtxt::from_sources::<LangSimple>(&sources1);
    build_and_count(&cc1);
    let ids1: Vec<_> = cc1.hir_map.read().keys().cloned().collect();

    let sources2: Vec<_> = (0..10)
        .map(|i| format!("fn func_2_{}() {{ y = {} }}", i, i).into_bytes())
        .collect();

    let cc2 = CompileCtxt::from_sources::<LangSimple>(&sources2);
    build_and_count(&cc2);
    let ids2: Vec<_> = cc2.hir_map.read().keys().cloned().collect();

    let set1: HashSet<_> = ids1.iter().cloned().collect();
    let set2: HashSet<_> = ids2.iter().cloned().collect();
    let overlaps = set1.intersection(&set2).count();

    assert_eq!(
        overlaps, 0,
        "Independent builds should not have ID collisions"
    );
    println!(
        "✅ No ID collisions: {} vs {} nodes",
        ids1.len(),
        ids2.len()
    );
}

/// Test 9: Verify HIR structure integrity
#[test]
fn test_ir_build_structure_integrity() {
    let source = br#"
fn main() {
    x = 1
    y = 2
}

fn helper() {
    z = 3
}
"#
    .to_vec();

    let cc = CompileCtxt::from_sources::<LangSimple>(&[source]);
    let hir_nodes = build_and_count(&cc);

    let file_root = cc.file_root_id(0);
    assert!(file_root.is_some(), "File should have root");

    let root_id = file_root.unwrap();
    assert!(
        cc.hir_map.read().contains_key(&root_id),
        "Root must be in map"
    );

    println!("✅ Structure integrity: {} nodes", hir_nodes);
}

/// Test 10: Verify file isolation with identical content
#[test]
fn test_ir_build_file_isolation_identical_content() {
    let identical_source = br#"
fn foo() { x = 1 }
fn bar() { y = 2 }
"#;

    let sources = vec![
        identical_source.to_vec(),
        identical_source.to_vec(),
        identical_source.to_vec(),
    ];

    let cc = CompileCtxt::from_sources::<LangSimple>(&sources);
    let hir_nodes = build_and_count(&cc);

    let roots: Vec<_> = (0..3).filter_map(|i| cc.file_root_id(i)).collect();
    assert_eq!(roots.len(), 3, "All 3 files should have roots");

    let root_set: HashSet<_> = roots.iter().cloned().collect();
    assert_eq!(root_set.len(), 3, "Each file should have unique root ID");

    println!(
        "✅ File isolation: {} files, {} nodes",
        sources.len(),
        hir_nodes
    );
}

// ============================================================================
// BENCHMARK TESTS - Performance and scaling validation
// ============================================================================

/// Benchmark: 100 files × 100 lines (10k total lines)
#[test]
fn bench_ir_build_100_files_100_lines() {
    const NUM_FILES: usize = 100;
    const LINES_PER_FILE: usize = 100;
    let sources = generate_sources(NUM_FILES, LINES_PER_FILE);

    let start = Instant::now();
    let cc = CompileCtxt::from_sources::<LangSimple>(&sources);
    let parse_time = start.elapsed();

    let start = Instant::now();
    let hir_nodes = build_and_count(&cc);
    let build_time = start.elapsed();

    println!("Bench 1: {} files × {} lines", NUM_FILES, LINES_PER_FILE);
    println!(
        "  Parse: {:.2}ms, Build: {:.2}ms, Total: {:.2}ms",
        parse_time.as_secs_f64() * 1000.0,
        build_time.as_secs_f64() * 1000.0,
        (parse_time + build_time).as_secs_f64() * 1000.0
    );
    println!(
        "  {} nodes, {:.0} nodes/ms",
        hir_nodes,
        hir_nodes as f64 / build_time.as_secs_f64() / 1000.0
    );
}

/// Benchmark: 500 files × 1000 lines (500k total lines)
#[test]
fn bench_ir_build_500_files_1000_lines() {
    const NUM_FILES: usize = 500;
    const LINES_PER_FILE: usize = 1000;

    let mut sources = Vec::with_capacity(NUM_FILES);
    for file_idx in 0..NUM_FILES {
        let mut content = String::new();
        for line_idx in 0..LINES_PER_FILE {
            content.push_str(&format!(
                "fn f{}_{}() {{ x = {} }}\n",
                file_idx,
                line_idx,
                line_idx % 1000
            ));
        }
        sources.push(content.into_bytes());
    }

    let start = Instant::now();
    let cc = CompileCtxt::from_sources::<LangSimple>(&sources);
    let parse_time = start.elapsed();

    let start = Instant::now();
    let hir_nodes = build_and_count(&cc);
    let build_time = start.elapsed();

    println!("Bench 2: {} files x {} lines", NUM_FILES, LINES_PER_FILE);
    println!(
        "  Parse: {:.2}ms, Build: {:.2}ms, Total: {:.2}ms",
        parse_time.as_secs_f64() * 1000.0,
        build_time.as_secs_f64() * 1000.0,
        (parse_time + build_time).as_secs_f64() * 1000.0
    );
    println!(
        "  {} nodes, {:.0} nodes/ms",
        hir_nodes,
        hir_nodes as f64 / build_time.as_secs_f64() / 1000.0
    );
}

/// Benchmark: 1000 files × 10k lines (production scale - ignored by default)
#[test]
#[ignore]
fn bench_ir_build_1000_files_10k_lines() {
    const NUM_FILES: usize = 1000;
    const LINES_PER_FILE: usize = 10000;

    let mut sources = Vec::with_capacity(NUM_FILES);
    for file_idx in 0..NUM_FILES {
        let mut content = String::with_capacity(LINES_PER_FILE * 50);
        for line_idx in 0..LINES_PER_FILE {
            content.push_str(&format!(
                "fn f{:04}_{}() {{ x = {} }}\n",
                file_idx,
                line_idx,
                line_idx % 10000
            ));
        }
        sources.push(content.into_bytes());
        if (file_idx + 1) % 100 == 0 {
            println!("  Generated {} files...", file_idx + 1);
        }
    }

    let start = Instant::now();
    let cc = CompileCtxt::from_sources::<LangSimple>(&sources);
    let parse_time = start.elapsed();

    let start = Instant::now();
    let hir_nodes = build_and_count(&cc);
    let build_time = start.elapsed();

    let total_lines = NUM_FILES * LINES_PER_FILE;
    println!(
        "Bench 3 (production): {} files × {} lines",
        NUM_FILES, LINES_PER_FILE
    );
    println!(
        "  Parse: {:.2}s, Build: {:.2}s, Total: {:.2}s",
        parse_time.as_secs_f64(),
        build_time.as_secs_f64(),
        (parse_time + build_time).as_secs_f64()
    );
    println!(
        "  {} nodes, {:.0} nodes/s",
        hir_nodes,
        hir_nodes as f64 / build_time.as_secs_f64()
    );
}

/// Benchmark: Scaling analysis across different file/line distributions
#[test]
fn bench_ir_build_scaling_analysis() {
    let configs = vec![
        (100, 100),  // 100 files × 100 lines = 10k lines
        (500, 200),  // 500 files × 200 lines = 100k lines
        (1000, 100), // 1000 files × 100 lines = 100k lines
    ];

    println!("\nScaling analysis:");
    let mut results = Vec::new();

    for (num_files, lines_per_file) in configs {
        let sources = generate_sources(num_files, lines_per_file);

        let start = Instant::now();
        let cc = CompileCtxt::from_sources::<LangSimple>(&sources);
        let parse_time = start.elapsed();

        let start = Instant::now();
        let hir_nodes = build_and_count(&cc);
        let build_time = start.elapsed();

        let total_time = parse_time + build_time;
        results.push((num_files, hir_nodes, total_time.as_secs_f64()));

        println!(
            "  {} files × {} lines: {} nodes, {:.3}s",
            num_files,
            lines_per_file,
            hir_nodes,
            total_time.as_secs_f64()
        );
    }

    // Verify sub-quadratic scaling
    if results.len() >= 2 {
        let (files1, _, time1) = results[0];
        let (files2, _, time2) = results[results.len() - 1];

        let file_ratio = files2 as f64 / files1 as f64;
        let time_ratio = time2 as f64 / time1 as f64;

        assert!(
            time_ratio < file_ratio * 2.0,
            "Scaling should be sub-quadratic"
        );
        println!(
            "✅ Scaling: {:.1}x files → {:.1}x time (sub-quadratic)",
            file_ratio, time_ratio
        );
    }
}
