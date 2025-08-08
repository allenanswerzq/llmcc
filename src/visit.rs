use tree_sitter::{Parser, Node, Tree, TreeCursor}
use std::collections::VecDeque;

pub trait Visitor {
    fn visit_node(&mut self, node: Node);
}

fn dfs<T: Visitor>(tree: &Tree, visitor: &mut T) {
    let mut cursor = tree.root_node().walk();
    visitor.visit_node(cursor.node());

    if (cursor.goto_first_child()) {
        dfs(cursor, visitor);
        cursor.goto_parent();
    }

    while cursor.goto_next_sibling() {
        dfs(cursor, visitor);
    }

    cursor.goto_parent();
}


fn bfs<T: Visitor>(tree: &Tree, visitor: &mut T) {
    let mut queue = VecDeque::new();
    let root_cursor = tree.root_node().walk();
    queue.push_back(root_cursor);

    while let Some(mut cursor) = queue.pop_front() {
        visitor.visit_node(cursor.node());

        if cursor.goto_first_child() {
            queue.push_back(tree.root_node().walk());
            queue.back_mut().unwrap().reset(cursor.node());

            while cursor.goto_next_sibling() {
               let mut sibling_cursor = tree.root_node().walk();
               sibling_cursor.reset(cursor.node());
               queue.push_back(sibling_cursor);
            }
        }
    }
}
