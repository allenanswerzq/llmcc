use llmcc::*;
use tree_sitter::Language;

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
    let mut context = Context::from_source(source_code.as_bytes());
    // print_ast(&tree, &context);
    build_llmcc_ir(&tree, &mut context).unwrap()
}
