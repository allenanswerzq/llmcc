use llmcc::{arena::ArenaIdNode, *};

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

    // Create a new parser
    let mut parser = Parser::new();

    // Set the language to Rust
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Error loading Rust grammar");

    // Parse the source code
    let tree = parser.parse(source_code, None).unwrap();
    let mut context = AstContext::from_source(source_code.as_bytes());
    print_ast(&tree, &mut context);

    let mut arena = IrArena::new();
    build_llmcc_ir(&tree, &mut context, &mut arena).unwrap();
    print_llmcc_ir(ArenaIdNode(0), &mut context, &mut arena);

    // let stack = collect_llmcc_ast(&tree, &context, arena.clone());
    // print_llmcc_ast(&tree, &mut context, arena.clone());
    // bind_llmcc_ast(&tree, &context, arena.clone(), stack);
    // print_llmcc_ast(&tree, &mut context, arena.clone());
}
