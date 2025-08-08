use std::collections::VecDeque;
use std::num::NonZeroU16;
use tree_sitter::{Parser, Tree, TreeCursor};

pub trait Visitor<'a> {
    fn visit_node(&mut self, cursor: &mut TreeCursor<'a>);
}

fn dfs_<'a, T: Visitor<'a>>(cursor: &mut TreeCursor<'a>, visitor: &mut T) {
    visitor.visit_node(cursor);

    if cursor.goto_first_child() {
        loop {
            dfs_(cursor, visitor);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

pub fn dfs<'a, T: Visitor<'a>>(tree: &'a Tree, visitor: &mut T) {
    let mut cursor = tree.root_node().walk();
    dfs_(&mut cursor, visitor);
}

pub fn bfs<'a, T: Visitor<'a>>(tree: &'a Tree, visitor: &mut T) {
    let mut queue = VecDeque::new();
    queue.push_back(tree.root_node().walk());

    while let Some(mut cursor) = queue.pop_front() {
        visitor.visit_node(&mut cursor);

        if cursor.goto_first_child() {
            loop {
                queue.push_back(cursor.clone());
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
struct AstPrinter {}

impl<'a> Visitor<'a> for AstPrinter {
    fn visit_node(&mut self, cursor: &mut TreeCursor<'a>) {
        let node = cursor.node();
        let kind = node.kind();
        let kind_id = node.kind_id();
        let field_id = cursor.field_id().unwrap_or(NonZeroU16::new(65535).unwrap());
        println!("{kind},{kind_id}: {field_id}");
    }
}

pub fn print_ast(tree: &Tree) {
    let mut vistor = AstPrinter::default();
    dfs(&tree, &mut vistor);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NodeKindCollector {
        pub kinds: Vec<String>,
    }

    impl<'a> Visitor<'a> for NodeKindCollector {
        fn visit_node(&mut self, cursor: &mut TreeCursor<'a>) {
            self.kinds.push(cursor.node().kind().to_string());
        }
    }

    /// This Visitor collects the actual text of identifier nodes.
    /// It is generic over 'a and stores string slices with that lifetime.
    /// This would not compile without the lifetime on the Visitor trait.
    struct IdentifierCollector<'a> {
        pub identifiers: Vec<&'a str>,
        source: &'a str,
    }

    impl<'a> Visitor<'a> for IdentifierCollector<'a> {
        fn visit_node(&mut self, cursor: &mut TreeCursor<'a>) {
            if cursor.node().kind() == "identifier" {
                let node_text = cursor.node().utf8_text(self.source.as_bytes()).unwrap();
                self.identifiers.push(node_text);
            }
        }
    }

    /// Helper function to set up the parser and parse the source code.
    fn parse_code(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("Error loading Rust grammar");
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn test_dfs_traversal() {
        let source_code = "fn example() { let x = 42; }";
        let tree = parse_code(source_code);
        let mut collector = NodeKindCollector { kinds: Vec::new() };
        dfs(&tree, &mut collector);

        let expected_kinds = vec![
            "source_file",
            "function_item",
            "fn",
            "identifier",
            "parameters",
            "(",
            ")",
            "block",
            "{",
            "let_declaration",
            "let",
            "identifier",
            "=",
            "integer_literal",
            ";",
            "}",
        ];
        assert_eq!(collector.kinds, expected_kinds);
    }

    #[test]
    fn test_bfs_traversal() {
        let source_code = "fn example() { let x = 42; }";
        let tree = parse_code(source_code);
        let mut collector = NodeKindCollector { kinds: Vec::new() };
        bfs(&tree, &mut collector);

        let expected_kinds = vec![
            "source_file",
            "function_item",
            "fn",
            "identifier",
            "parameters",
            "block",
            "(",
            ")",
            "{",
            "let_declaration",
            "}",
            "let",
            "identifier",
            "=",
            "integer_literal",
            ";",
        ];
        assert_eq!(collector.kinds, expected_kinds);
    }

    // --- NEW TESTS FOR LIFETIME ---

    #[test]
    fn test_dfs_lifetime_visitor() {
        // 1. Setup
        let source_code = "fn example() { let x = 42; }";
        let tree = parse_code(source_code);
        let mut collector = IdentifierCollector {
            identifiers: Vec::new(),
            source: source_code,
        };

        // 2. Execute
        dfs(&tree, &mut collector);

        // 3. Assert
        // We expect to have collected the text of the two identifiers.
        let expected_identifiers = vec!["example", "x"];
        assert_eq!(
            collector.identifiers, expected_identifiers,
            "DFS identifier collection is incorrect."
        );
    }

    #[test]
    fn test_bfs_lifetime_visitor() {
        // 1. Setup
        let source_code = "fn example() { let x = 42; }";
        let tree = parse_code(source_code);
        let mut collector = IdentifierCollector {
            identifiers: Vec::new(),
            source: source_code,
        };

        // 2. Execute
        bfs(&tree, &mut collector);

        // 3. Assert
        // The order of identifiers in BFS will be the same as DFS for this simple case.
        let expected_identifiers = vec!["example", "x"];
        assert_eq!(
            collector.identifiers, expected_identifiers,
            "BFS identifier collection is incorrect."
        );
    }
}
