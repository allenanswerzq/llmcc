use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::HirKind;
use llmcc_core::lang_def::LanguageTrait;
use llmcc_python::LangPython;

#[test]
fn test_language_trait_implemented() {
    // Test that LangPython implements LanguageTrait
    let _ = LangPython::new();
}

#[test]
fn test_parse_simple_python() {
    let code = b"def foo():\n    pass\n";
    let tree = LangPython::parse(code);
    assert!(tree.is_some(), "Failed to parse simple Python function");
}

#[test]
fn test_parse_class_definition() {
    let code = b"class MyClass:\n    def method(self):\n        pass\n";
    let tree = LangPython::parse(code);
    assert!(tree.is_some(), "Failed to parse Python class");
}

#[test]
fn test_parse_import_statement() {
    let code = b"import os\nfrom sys import argv\n";
    let tree = LangPython::parse(code);
    assert!(tree.is_some(), "Failed to parse import statements");
}

#[test]
fn test_token_str_lookup() {
    assert_eq!(LangPython::token_str(LangPython::Text_def), Some("def"));
    assert_eq!(LangPython::token_str(LangPython::Text_class), Some("class"));
    assert_eq!(
        LangPython::token_str(LangPython::Text_import),
        Some("import")
    );
    assert_eq!(
        LangPython::token_str(LangPython::identifier),
        Some("identifier")
    );
    assert_eq!(
        LangPython::token_str(LangPython::function_definition),
        Some("function_definition")
    );
    assert_eq!(
        LangPython::token_str(LangPython::class_definition),
        Some("class_definition")
    );
}

#[test]
fn test_token_str_invalid() {
    assert_eq!(LangPython::token_str(9999), None);
}

#[test]
fn test_hir_kind_text_tokens() {
    assert_eq!(LangPython::hir_kind(LangPython::Text_def), HirKind::Text);
    assert_eq!(LangPython::hir_kind(LangPython::Text_class), HirKind::Text);
    assert_eq!(LangPython::hir_kind(LangPython::Text_COLON), HirKind::Text);
}

#[test]
fn test_hir_kind_identifier() {
    assert_eq!(
        LangPython::hir_kind(LangPython::identifier),
        HirKind::Identifier
    );
}

#[test]
fn test_hir_kind_scope_nodes() {
    assert_eq!(
        LangPython::hir_kind(LangPython::function_definition),
        HirKind::Scope
    );
    assert_eq!(
        LangPython::hir_kind(LangPython::class_definition),
        HirKind::Scope
    );
    assert_eq!(LangPython::hir_kind(LangPython::block), HirKind::Scope);
}

#[test]
fn test_hir_kind_file() {
    assert_eq!(LangPython::hir_kind(LangPython::source_file), HirKind::File);
}

#[test]
fn test_block_kind_root() {
    assert_eq!(
        LangPython::block_kind(LangPython::source_file),
        BlockKind::Root
    );
}

#[test]
fn test_block_kind_func_and_class() {
    assert_eq!(
        LangPython::block_kind(LangPython::function_definition),
        BlockKind::Func
    );
    assert_eq!(
        LangPython::block_kind(LangPython::class_definition),
        BlockKind::Class
    );
}

#[test]
fn test_block_kind_scope() {
    assert_eq!(LangPython::block_kind(LangPython::block), BlockKind::Scope);
}

#[test]
fn test_block_kind_call() {
    assert_eq!(LangPython::block_kind(LangPython::call), BlockKind::Call);
}

#[test]
fn test_block_kind_undefined() {
    assert_eq!(LangPython::block_kind(9999), BlockKind::Undefined);
}

#[test]
fn test_is_valid_token() {
    assert!(LangPython::is_valid_token(LangPython::Text_def));
    assert!(LangPython::is_valid_token(LangPython::function_definition));
    assert!(LangPython::is_valid_token(LangPython::source_file));
    assert!(LangPython::is_valid_token(LangPython::call));
}

#[test]
fn test_is_invalid_token() {
    assert!(!LangPython::is_valid_token(9999));
    assert!(!LangPython::is_valid_token(65535));
}

#[test]
fn test_field_accessors() {
    let name_field = LangPython::name_field();
    let type_field = LangPython::type_field();

    assert_eq!(name_field, LangPython::field_name);
    assert_eq!(type_field, LangPython::field_type);
    assert_eq!(LangPython::token_str(name_field), Some("name"));
    assert_eq!(LangPython::token_str(type_field), Some("type"));
}

#[test]
fn test_token_constants_unique() {
    // Ensure all token constants have unique IDs
    let mut ids = vec![
        LangPython::Text_def,
        LangPython::Text_class,
        LangPython::Text_import,
        LangPython::Text_from,
        LangPython::Text_as,
        LangPython::Text_return,
        LangPython::identifier,
        LangPython::source_file,
        LangPython::function_definition,
        LangPython::class_definition,
        LangPython::import_statement,
        LangPython::call,
        LangPython::assignment,
    ];
    ids.sort();
    ids.dedup();
    assert!(
        ids.len() >= 13,
        "Token IDs should be mostly unique for common tokens"
    );
}
