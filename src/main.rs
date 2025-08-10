use llmcc::*;

fn main() {
    // // Enum -> number
    // let num: u8 = AstTokenRust::Foo.into();
    // println!("Enum to number: {}", num);

    // // Number -> enum
    // let e = AstTokenRust::try_from(1).unwrap();
    // println!("Number to enum: {}", e.to_string());

    // // // Enum -> string
    // // let s = e.to_string();
    // // println!("Enum to string: {}", s);

    // // // String -> enum
    // // let e2: AstTokenRust = "foo".parse().unwrap();
    // // println!("String to enum: {:?}", e2);
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
    // println!("{}", tree.root_node().to_sexp());
    let arena = AstArena::new();
    let tree = build_llmcc_ast(&tree, &mut context, arena.clone()).unwrap();
    print_llmcc_ast(&tree, &mut context, arena.clone());
    let stack = collect_llmcc_ast(&tree, &context, arena.clone());
    bind_llmcc_ast(&tree, &context, arena.clone(), stack);
}
