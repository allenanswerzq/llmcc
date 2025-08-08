use std::collections::VecDeque;
use std::num::NonZeroU16;
use tree_sitter::{Parser, Tree, TreeCursor};

use crate::{AstContext, AstKindNode};

pub trait Visitor<'a> {
    /// Called when entering a node (before visiting children)
    fn visit_enter_node(&mut self, cursor: &mut TreeCursor<'a>) {
        // Default implementation - can be overridden
    }

    /// Called when visiting a node (main visitor method)
    fn visit_node(&mut self, cursor: &mut TreeCursor<'a>);

    /// Called when left/visited a node
    fn finalize_node(&mut self, ast_node: &AstKindNode) {
        // Default implementation - can be overridden
    }

    /// Called when leaving a node (after visiting all children)
    fn visit_leave_node(&mut self, cursor: &mut TreeCursor<'a>) {
        // Default implementation - can be overridden
    }
}

fn dfs_<'a, T: Visitor<'a>>(cursor: &mut TreeCursor<'a>, visitor: &mut T) {
    // Enter the node
    visitor.visit_enter_node(cursor);

    // Visit the node
    visitor.visit_node(cursor);

    // Visit children if any exist
    if cursor.goto_first_child() {
        loop {
            dfs_(cursor, visitor);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }

    // Leave the node
    visitor.visit_leave_node(cursor);
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

#[derive(Debug)]
struct AstPrinter {
    context: Box<AstContext>,
    depth: usize,
    output: String,
}

impl AstPrinter {
    fn new(context: Box<AstContext>) -> Self {
        Self {
            context,
            depth: 0,
            output: String::new(),
        }
    }

    fn get_output(&self) -> &str {
        &self.output
    }

    fn print_output(&self) {
        println!("{}", self.output);
    }
}

impl<'a> Visitor<'a> for AstPrinter {
    fn visit_enter_node(&mut self, cursor: &mut TreeCursor<'a>) {
        let node = cursor.node();
        let kind = node.kind();
        let kind_id = node.kind_id();
        let start = node.start_byte();
        let end = node.end_byte();

        self.output.push_str(&"  ".repeat(self.depth));
        self.output.push('(');

        if let Some(field_name) = cursor.field_name() {
            let field_id = cursor.field_id().unwrap();
            self.output.push_str(&format!("{}:{}", field_name, kind));
        } else {
            self.output.push_str(kind);
        }

        self.output.push_str(&format!(" [{}]", kind_id));

        if node.child_count() == 0 {
            // For leaf nodes, also include the text content if available
            let text = self.context.file.get_text(start, end).unwrap();
            self.output.push_str(&format!(" \"{}\"", text));
            self.output.push(')');
        } else {
            self.output.push('\n');
        }

        self.depth += 1;
    }

    fn visit_leave_node(&mut self, cursor: &mut TreeCursor<'a>) {
        let node = cursor.node();
        self.depth -= 1;

        if node.child_count() > 0 {
            self.output.push_str(&"  ".repeat(self.depth));
            self.output.push(')');
        }

        if self.depth > 0 {
            self.output.push('\n');
        }
    }

    fn visit_node(&mut self, _cursor: &mut TreeCursor<'a>) {}
}

pub fn print_ast(tree: &Tree, context: Box<AstContext>) {
    let mut vistor = AstPrinter::new(context);
    dfs(&tree, &mut vistor);
    vistor.print_output();
}
