use llmcc_rust::*;

fn main() {
    let source_code = r#"
        fn foo(a: u16, b: u16) -> u16 {
            let mut x = 0;
            x = a + b;
            x
        }
        fn main() {
            let a = 1;
            let b = 2;
            foo(a, b);
        }
        "#
    .trim();

    let lang = tree_sitter_rust::LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&lang).unwrap();
    let tree = parser.parse(source_code, None).unwrap();

    let gcx = GlobalCtxt::from_source(source_code.as_bytes());
    let ctx = gcx.create_context();
    build_llmcc_ir::<LanguageRust>(&tree, &ctx);

    let root = HirId(0);
    resolve_symbols(root, &ctx);
    print_llmcc_ir(root, &ctx);

    build_llmcc_graph::<LanguageRust>(root, &ctx);
    print_llmcc_graph(BlockId(0), &ctx);
}
