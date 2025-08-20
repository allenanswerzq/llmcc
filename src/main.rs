use llmcc::*;

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
    let mut context = AstContext::from_source(source_code.as_bytes());
    print_ast(&tree, &mut context);

    let mut arena = HirArena::new();
    build_llmcc_ir(&tree, &mut context, &mut arena).unwrap();
    // print_llmcc_ir(NodeId(0), &mut context, &mut arena);

    find_declaration(NodeId(0), &context, &mut arena);
    print_llmcc_ir(NodeId(0), &mut context, &mut arena);
}
