use tree_sitter::{Node, Tree, TreeCursor};

use crate::{AstArena, AstContext};

pub trait CursorTrait {
    fn goto_first_child(&mut self) -> bool;
    fn goto_next_sibling(&mut self) -> bool;
    fn goto_parent(&mut self) -> bool;
}

pub trait NodeTrait {
    fn get_child(&self, index: usize) -> Option<usize>;
    fn child_count(&self) -> usize;
}

impl<'a> CursorTrait for TreeCursor<'a> {
    fn goto_first_child(&mut self) -> bool {
        self.goto_first_child()
    }

    fn goto_next_sibling(&mut self) -> bool {
        self.goto_next_sibling()
    }

    fn goto_parent(&mut self) -> bool {
        self.goto_parent()
    }
}

pub trait Visitor<C> {
    fn visit_enter_node(&mut self, _c: &mut C) {}
    fn visit_node(&mut self, _c: &mut C) {}
    // fn finalize_node(&mut self, _t: &AstKindNode) {}
    fn visit_leave_node(&mut self, _c: &mut C) {}
}

fn dfs_<C, V>(cursor: &mut C, visitor: &mut V)
where
    C: CursorTrait,
    V: Visitor<C>,
{
    visitor.visit_enter_node(cursor);
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

    visitor.visit_leave_node(cursor);
}

pub fn dfs<T, V>(cursor: &mut T, visitor: &mut V)
where
    T: CursorTrait,
    V: Visitor<T>,
{
    dfs_(cursor, visitor);
}

#[derive(Debug)]
struct AstPrinter<'a> {
    context: &'a AstContext,
    depth: usize,
    output: String,
}

impl<'a> AstPrinter<'a> {
    fn new(context: &'a AstContext) -> Self {
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

impl<'a> Visitor<TreeCursor<'a>> for AstPrinter<'a> {
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

pub fn print_ast(tree: &Tree, context: &mut AstContext) {
    let mut vistor = AstPrinter::new(context);
    let mut cursor = tree.walk();
    dfs(&mut cursor, &mut vistor);
    vistor.print_output();
}

#[derive(Debug)]
pub struct CursorGeneric<'a, T> {
    arena: &'a mut AstArena<T>,
    path: Vec<usize>,
    current_node: usize,
    parent_node: Option<usize>,
}

impl<'a, T> CursorGeneric<'a, T>
where
    T: NodeTrait + Default,
{
    pub fn new(arena: &'a mut AstArena<T>) -> Self {
        Self {
            arena,
            path: vec![],
            current_node: 1, // Start at root (id 1)
            parent_node: None,
        }
    }

    pub fn get_arena(&mut self) -> &mut AstArena<T> {
        self.arena
    }

    pub fn node(&mut self) -> &mut T {
        let id = self.current_node;
        self.arena.get_mut(id).unwrap()
    }

    pub fn node_ref(&self) -> &T {
        self.arena.get(self.current_node).unwrap()
    }

    pub fn current_node_id(&self) -> usize {
        self.current_node
    }

    pub fn parent_node_id(&self) -> Option<usize> {
        self.parent_node
    }

    pub fn depth(&self) -> usize {
        self.path.len()
    }

    pub fn is_at_root(&self) -> bool {
        self.path.is_empty()
    }

    pub fn current_path(&self) -> &[usize] {
        &self.path
    }

    pub fn parent(&self) -> Option<&T> {
        self.parent_node.and_then(|id| self.arena.get(id))
    }

    pub fn parent_mut(&mut self) -> Option<&mut T> {
        if let Some(parent_id) = self.parent_node {
            self.arena.get_mut(parent_id)
        } else {
            None
        }
    }

    fn get_node_at_path(&self, path: &[usize]) -> Option<usize> {
        if path.is_empty() {
            return Some(1); // Root node ID
        }

        let mut current_id = 1; // Start at root
        for &index in path {
            if let Some(node) = self.arena.get(current_id) {
                if let Some(child_id) = node.get_child(index) {
                    current_id = child_id;
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        Some(current_id)
    }
}

impl<'a, T> CursorTrait for CursorGeneric<'a, T>
where
    T: NodeTrait + Default,
{
    fn goto_first_child(&mut self) -> bool {
        if let Some(node) = self.arena.get(self.current_node) {
            if node.child_count() > 0 {
                if let Some(child_id) = node.get_child(0) {
                    self.path.push(0);
                    self.parent_node = Some(self.current_node);
                    self.current_node = child_id;
                    return true;
                }
            }
        }
        false
    }

    fn goto_next_sibling(&mut self) -> bool {
        if self.path.is_empty() {
            return false; // Already at root, no siblings
        }

        let last_index = self.path.len() - 1;
        let current_index = self.path[last_index];
        let next_index = current_index + 1;

        // Use parent_node directly
        if let Some(parent_id) = self.parent_node {
            if let Some(parent) = self.arena.get(parent_id) {
                if next_index < parent.child_count() {
                    if let Some(next_sibling_id) = parent.get_child(next_index) {
                        self.path[last_index] = next_index;
                        self.current_node = next_sibling_id;
                        return true;
                    }
                }
            }
        }
        false
    }

    fn goto_parent(&mut self) -> bool {
        if !self.path.is_empty() {
            self.path.pop();

            // Calculate new parent_node
            let new_parent = if self.path.len() >= 1 {
                // Find the parent of our new current node
                self.get_node_at_path(&self.path[..self.path.len().saturating_sub(1)])
            } else {
                // Moving to root, so no parent
                None
            };

            // Move to parent
            if let Some(parent_id) = self.parent_node {
                self.current_node = parent_id;
                self.parent_node = new_parent;
                return true;
            }
        }
        false
    }
}
