use std::marker::PhantomData;

use tree_sitter::{Node, Tree, TreeCursor};

use crate::arena::{ArenaIdNode, ir_arena, ir_arena_mut};
use crate::lang::AstContext;

pub trait CursorTrait {
    fn goto_first_child(&mut self) -> bool;
    fn goto_next_sibling(&mut self) -> bool;
    fn goto_parent(&mut self) -> bool;
}

pub trait NodeTrait {
    fn get_child(&self, index: usize) -> Option<ArenaIdNode>;
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
    fn visit_leave_node(&mut self, _c: &mut C) {}
    // fn finalize_node(&mut self, _t: &AstKindNode) {}
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

pub struct CursorGeneric<'a, F, T>
where
    T: NodeTrait + Default,
    F: FnMut(usize) -> Option<&'a mut T>,
{
    // Stack of (node_id, child_index) pairs representing the path from root
    path_stack: Vec<(usize, usize)>,
    current_node: usize,
    arena_fn: F,
    _marker: PhantomData<&'a T>,
}

impl<'a, F, T> CursorGeneric<'a, F, T>
where
    T: NodeTrait + Default,
    F: FnMut(usize) -> Option<&'a mut T>,
{
    pub fn new(root: usize, arena_fn: F) -> Self {
        Self {
            path_stack: vec![],
            current_node: root,
            arena_fn,
            _marker: PhantomData,
        }
    }

    pub fn node(&mut self) -> Option<&mut T> {
        (self.arena_fn)(self.current_node)
    }

    pub fn current_node_id(&self) -> usize {
        self.current_node
    }

    pub fn parent_node_id(&self) -> Option<usize> {
        self.path_stack.last().map(|(parent_id, _)| *parent_id)
    }

    pub fn depth(&self) -> usize {
        self.path_stack.len()
    }

    pub fn is_at_root(&self) -> bool {
        self.path_stack.is_empty()
    }

    // Returns just the child indices for compatibility
    pub fn current_path(&self) -> Vec<usize> {
        self.path_stack.iter().map(|(_, index)| *index).collect()
    }

    // Get the full path including node IDs (useful for debugging)
    pub fn current_path_with_nodes(&self) -> &[(usize, usize)] {
        &self.path_stack
    }

    fn goto_first_child(&mut self) -> bool {
        if let Some(node) = (self.arena_fn)(self.current_node) {
            if node.child_count() > 0 {
                if let Some(child_id) = node.get_child(0) {
                    // Push current node and child index onto stack
                    self.path_stack.push((self.current_node, 0));
                    self.current_node = child_id.into();
                    return true;
                }
            }
        }
        false
    }

    fn goto_next_sibling(&mut self) -> bool {
        if self.path_stack.is_empty() {
            return false; // Already at root, no siblings
        }

        // Get current parent and child index
        let (parent_id, current_index) = self.path_stack[self.path_stack.len() - 1];
        let next_index = current_index + 1;

        // Check if parent has a next child
        let n = self.path_stack.len() - 1;
        if let Some(parent) = (self.arena_fn)(parent_id) {
            if next_index < parent.child_count() {
                if let Some(next_sibling_id) = parent.get_child(next_index) {
                    // Update the child index in the stack
                    self.path_stack[n].1 = next_index;
                    self.current_node = next_sibling_id.into();
                    return true;
                }
            }
        }
        false
    }

    fn goto_parent(&mut self) -> bool {
        if let Some((parent_id, _)) = self.path_stack.pop() {
            self.current_node = parent_id;
            return true;
        }
        false
    }

    // Additional navigation methods for completeness
    fn goto_child(&mut self, index: usize) -> bool {
        if let Some(node) = (self.arena_fn)(self.current_node) {
            if index < node.child_count() {
                if let Some(child_id) = node.get_child(index) {
                    self.path_stack.push((self.current_node, index));
                    self.current_node = child_id.into();
                    return true;
                }
            }
        }
        false
    }

    fn goto_root(&mut self) {
        if let Some((root_id, _)) = self.path_stack.first() {
            let root_id = *root_id;
            self.path_stack.clear();
            self.current_node = root_id;
        }
    }

    // Efficient sibling iteration
    fn goto_previous_sibling(&mut self) -> bool {
        if self.path_stack.is_empty() {
            return false;
        }

        let (parent_id, current_index) = self.path_stack[self.path_stack.len() - 1];
        if current_index > 0 {
            let prev_index = current_index - 1;

            let n = self.path_stack.len() - 1;
            if let Some(parent) = (self.arena_fn)(parent_id) {
                if let Some(prev_sibling_id) = parent.get_child(prev_index) {
                    self.path_stack[n].1 = prev_index;
                    self.current_node = prev_sibling_id.into();
                    return true;
                }
            }
        }
        false
    }
}

impl<'a, F, T> CursorTrait for CursorGeneric<'a, F, T>
where
    T: NodeTrait + Default,
    F: FnMut(usize) -> Option<&'a mut T>,
{
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
