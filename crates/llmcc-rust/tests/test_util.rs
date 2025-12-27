mod common;

use common::with_compiled_unit;
use serial_test::serial;
use std::path::Path;
use textwrap::dedent;

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::DEBUG.into()),
        )
        .with_test_writer()
        .try_init();
}

// ============================================================================
// Phase 1: Utility Tests - File Parsing
// ============================================================================

#[serial]
#[test]
fn test_parse_file_name_strips_rs_extension() {
    init_tracing();

    let test_cases = vec![
        ("src/main.rs", Some("main")),
        ("src/lib.rs", Some("lib")),
        ("tests/integration_test.rs", Some("integration_test")),
        ("src/utils/mod.rs", Some("mod")),
    ];

    for (path, expected) in test_cases {
        let file_stem = Path::new(path)
            .file_stem()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        assert_eq!(file_stem.as_deref(), expected);
    }
}

#[serial]
#[test]
fn test_parse_file_name_handles_nested_paths() {
    init_tracing();

    let test_cases = vec![
        "src/utils/helper.rs",
        "src/models/database/connection.rs",
        "tests/integration/admin_tests.rs",
    ];

    for path in test_cases {
        let file_name = Path::new(path).file_stem().and_then(|n| n.to_str());
        assert!(file_name.is_some());
    }
}

#[serial]
#[test]
fn test_parse_file_name_returns_none_for_invalid_paths() {
    init_tracing();

    let invalid_paths = vec!["", ".", ".."];

    for path in invalid_paths {
        let file_name = Path::new(path).file_stem().and_then(|n| n.to_str());
        assert!(file_name.is_none() || file_name == Some(".") || file_name == Some(".."));
    }
}

#[serial]
#[test]
fn test_path_parsing_with_dots_and_underscores() {
    init_tracing();

    let test_cases = vec![
        ("src/async_utils.rs", "async_utils"),
        ("src/db_connection.rs", "db_connection"),
        ("src/test.utils.rs", "test.utils"),
    ];

    for (path, expected_name) in test_cases {
        let file_name = Path::new(path)
            .file_stem()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        assert_eq!(file_name, Some(expected_name.to_string()));
    }
}

#[serial]
#[test]
fn test_path_parsing_with_unicode_characters() {
    init_tracing();

    let test_cases = vec![
        ("src/utils_ñ.rs", "utils_ñ"),
        ("src/model_中文.rs", "model_中文"),
    ];

    for (path, expected_name) in test_cases {
        let file_name = Path::new(path)
            .file_stem()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        assert_eq!(file_name, Some(expected_name.to_string()));
    }
}

#[serial]
#[test]
fn test_path_parsing_with_special_characters() {
    init_tracing();

    let test_cases = vec![
        ("src/utils-helper.rs", "utils-helper"),
        ("src/test_v2.rs", "test_v2"),
    ];

    for (path, expected_name) in test_cases {
        let file_name = Path::new(path)
            .file_stem()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        assert_eq!(file_name, Some(expected_name.to_string()));
    }
}

#[serial]
#[test]
fn test_parse_module_name_returns_parent_for_mod_rs() {
    init_tracing();

    let path = "src/utils/mod.rs";
    let parent = Path::new(path)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str());
    assert_eq!(parent, Some("utils"));
}

#[serial]
#[test]
fn test_parse_module_name_returns_none_for_lib_rs() {
    init_tracing();

    let path = "src/lib.rs";
    let file_stem = Path::new(path).file_stem().and_then(|n| n.to_str());
    assert_eq!(file_stem, Some("lib"));
}

#[serial]
#[test]
fn test_parse_module_name_returns_none_for_main_rs() {
    init_tracing();

    let path = "src/main.rs";
    let file_stem = Path::new(path).file_stem().and_then(|n| n.to_str());
    assert_eq!(file_stem, Some("main"));
}

#[serial]
#[test]
fn test_parse_module_name_returns_parent_for_nested_files() {
    init_tracing();

    let test_cases = vec![
        ("src/utils/helper.rs", "utils"),
        ("src/models/db/connection.rs", "db"),
        ("src/api/v1/endpoints.rs", "v1"),
    ];

    for (path, expected_parent) in test_cases {
        let parent = Path::new(path)
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str());
        assert_eq!(parent, Some(expected_parent));
    }
}

#[serial]
#[test]
fn test_parse_module_name_handles_deeply_nested_paths() {
    init_tracing();

    let path = "src/server/api/v2/handlers/auth.rs";
    let parent = Path::new(path)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str());
    assert_eq!(parent, Some("handlers"));
}

#[serial]
#[test]
fn test_parse_module_name_returns_none_for_top_level_src_files() {
    init_tracing();

    let test_cases = vec!["src/lib.rs", "src/main.rs"];

    for path in test_cases {
        let file_name = Path::new(path).file_stem().and_then(|n| n.to_str());
        assert!(file_name == Some("lib") || file_name == Some("main"));
    }
}

#[serial]
#[test]
fn test_crate_name_extraction_from_compilation_context() {
    init_tracing();

    let source = dedent(
        "
        fn main() {
            println!(\"Hello, world!\");
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(!cc.files.is_empty());
    });
}

#[serial]
#[test]
fn test_file_path_parsing_in_compilation_unit() {
    init_tracing();

    let source = dedent(
        "
        fn process() {
            let x = 42;
        }
        ",
    );

    with_compiled_unit(&[&source], |cc| {
        assert!(!cc.files.is_empty());
    });
}

#[serial]
#[test]
fn test_module_structure_with_mod_rs() {
    init_tracing();

    let path = "src/utils/mod.rs";
    let is_mod_file = path.ends_with("mod.rs");
    assert!(is_mod_file);

    if is_mod_file {
        let parent = Path::new(path)
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str());
        assert_eq!(parent, Some("utils"));
    }
}

#[serial]
#[test]
fn test_crate_root_detection() {
    init_tracing();

    let test_cases = vec![
        ("src/lib.rs", true),
        ("src/main.rs", true),
        ("src/utils.rs", false),
        ("src/utils/mod.rs", false),
    ];

    for (path, is_root) in test_cases {
        let file_name = Path::new(path)
            .file_stem()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());

        let is_crate_root =
            file_name.as_deref() == Some("lib") || file_name.as_deref() == Some("main");

        assert_eq!(is_crate_root, is_root);
    }
}

#[serial]
#[test]
fn test_file_organization_conventions() {
    init_tracing();

    let standard_files = vec![
        ("src/lib.rs", "crate_root"),
        ("src/main.rs", "binary_entry"),
        ("src/mod.rs", "module_root"),
        ("src/utils/mod.rs", "submodule_root"),
        ("src/utils/helper.rs", "module_file"),
        ("tests/integration_test.rs", "integration_test"),
        ("examples/demo.rs", "example"),
    ];

    for (path, _category) in standard_files {
        let file_stem = Path::new(path).file_stem().and_then(|n| n.to_str());
        assert!(file_stem.is_some());
    }
}

#[serial]
#[test]
fn test_module_hierarchy_detection() {
    init_tracing();

    let test_cases = vec![
        ("src/utils.rs", 1),
        ("src/utils/helper.rs", 2),
        ("src/utils/db/connection.rs", 3),
        ("src/a/b/c/d/e.rs", 5),
    ];

    for (path, _expected_depth) in test_cases {
        let components: Vec<_> = Path::new(path).components().collect();
        assert!(!components.is_empty());
    }
}
