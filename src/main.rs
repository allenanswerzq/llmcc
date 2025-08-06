use tree_sitter::Parser;

fn main() {
    // Create a parser
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into()).expect("Error loading Rust grammar");

    // Your Rust source code
    let source_code = r#"
        fn main() {
            println!("Hello, world!");
        }
    "#;

    // Parse the code
    let tree = parser.parse(source_code, None).unwrap();
    let root_node = tree.root_node();

    println!("Root node: {}", root_node.kind());
    print_node_recursive(root_node, source_code, 0);
}

fn print_node_recursive(node: tree_sitter::Node, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    println!(
        "{}{} {} [{}..{}]: {:?}",
        indent,
        node.kind(),
        node.kind_id(),
        node.start_byte(),
        node.end_byte(),
        node.utf8_text(source.as_bytes()).unwrap_or("<error>")
    );

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            print_node_recursive(child, source, depth + 1);
        }
    }
}
