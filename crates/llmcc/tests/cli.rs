use std::fs;

use llmcc::{run_main, LlmccOptions, QueryDirection};
use llmcc_rust::LangRust;
use tempfile::tempdir;

fn fixture_source() -> &'static str {
    r#"
        fn leaf() {}

        fn middle() {
            leaf();
        }

        fn root() {
            middle();
        }
    "#
}

fn write_fixture() -> (tempfile::TempDir, String) {
    let dir = tempdir().expect("tempdir");
    let file_path = dir.path().join("fixture.rs");
    fs::write(&file_path, fixture_source()).expect("write fixture");
    (dir, file_path.display().to_string())
}

fn base_options(file: String) -> LlmccOptions {
    LlmccOptions {
        files: vec![file],
        dirs: Vec::new(),
        print_ir: false,
        print_block: false,
        design_graph: false,
        top_k: None,
        query: None,
        query_direction: QueryDirection::Depends,
        recursive: false,
        summary: false,
    }
}

#[test]
fn depends_recursive_expands_results() {
    let (_dir, file) = write_fixture();

    let mut opts = base_options(file.clone());
    opts.query = Some("root".to_string());
    opts.query_direction = QueryDirection::Depends;
    opts.recursive = false;

    let direct = run_main::<LangRust>(&opts)
        .expect("direct depends run")
        .expect("direct depends output");
    assert!(
        direct.contains("fn middle()"),
        "direct depends missing middle: {direct}"
    );
    assert!(
        !direct.contains("fn leaf()"),
        "direct depends should not include leaf: {direct}"
    );

    opts.recursive = true;
    let recursive = run_main::<LangRust>(&opts)
        .expect("recursive depends run")
        .expect("recursive depends output");
    assert!(
        recursive.contains("fn middle()"),
        "recursive depends missing middle: {recursive}"
    );
    assert!(
        recursive.contains("fn leaf()"),
        "recursive depends missing leaf: {recursive}"
    );
}

#[test]
fn dependents_recursive_expands_results() {
    let (_dir, file) = write_fixture();

    let mut opts = base_options(file);
    opts.query = Some("leaf".to_string());
    opts.query_direction = QueryDirection::Dependents;
    opts.recursive = false;

    let direct = run_main::<LangRust>(&opts)
        .expect("direct dependents run")
        .expect("direct dependents output");
    assert!(
        direct.contains("fn middle()"),
        "direct dependents missing middle: {direct}"
    );
    assert!(
        !direct.contains("fn root()"),
        "direct dependents should not include root: {direct}"
    );

    opts.recursive = true;
    let recursive = run_main::<LangRust>(&opts)
        .expect("recursive dependents run")
        .expect("recursive dependents output");
    assert!(
        recursive.contains("fn middle()"),
        "recursive dependents missing middle: {recursive}"
    );
    assert!(
        recursive.contains("fn root()"),
        "recursive dependents missing root: {recursive}"
    );
}

#[test]
fn files_and_dirs_conflict() {
    let (dir, file) = write_fixture();
    let dir_path = dir.path().display().to_string();

    let mut opts = base_options(file);
    opts.dirs = vec![dir_path];

    let err = run_main::<LangRust>(&opts).expect_err("files and dirs should conflict");
    assert!(
        err.to_string().contains("--file") && err.to_string().contains("--dir"),
        "unexpected error message: {err}"
    );
}

#[test]
fn summary_output_omits_source_code() {
    let (_dir, file) = write_fixture();

    let mut opts = base_options(file);
    opts.query = Some("leaf".to_string());
    opts.query_direction = QueryDirection::Dependents;
    opts.summary = true;

    let output = run_main::<LangRust>(&opts)
        .expect("summary query run")
        .expect("summary output");

    assert!(output.contains("DEPENDENTS"), "missing dependents header: {output}");
    assert!(output.contains("leaf"), "missing symbol name: {output}");
    assert!(output.contains(".rs:"), "expected file path with line info: {output}");
    assert!(!output.contains("┌─"), "summary should not include block rendering: {output}");
}
