use std::collections::VecDeque;
use tree_sitter::{Node as TsNode, Tree, TreeCursor};

use crate::{AstContext, AstKindNode};

pub trait CursorTrait {
    fn goto_first_child(&mut self) -> bool;
    fn goto_next_sibling(&mut self) -> bool;
    fn goto_parent(&mut self) -> bool;
}

pub trait TreeTrait<'a> {
    type Cursor: CursorTrait + 'a;
    type Node: NodeTrait + 'a;

    fn root_node(&'a self) -> Self::Node;
    fn walk(&'a self) -> Self::Cursor;
}

pub trait NodeTrait {
    fn get_child(&self, index: usize) -> Option<Box<Self>>;
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

impl<'a> NodeTrait for TsNode<'a> {
    fn get_child(&self, index: usize) -> Option<Box<Self>> {
        self.child(index).map(Box::new)
    }
    fn child_count(&self) -> usize {
        self.child_count()
    }
}

impl<'a> TreeTrait<'a> for Tree {
    type Cursor = TreeCursor<'a>;
    type Node = TsNode<'a>;

    fn root_node(&'a self) -> Self::Node {
        self.root_node()
    }

    fn walk(&'a self) -> Self::Cursor {
        self.root_node().walk()
    }
}

pub trait Visitor<C: CursorTrait> {
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

pub fn dfs<'a, T, V>(tree: &'a T, visitor: &mut V)
where
    T: TreeTrait<'a>,
    V: Visitor<T::Cursor>,
{
    let mut cursor = tree.walk();
    dfs_(&mut cursor, visitor);
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

pub fn print_ast(tree: &Tree, context: &AstContext) {
    let mut vistor = AstPrinter::new(context);
    dfs(tree, &mut vistor);
    vistor.print_output();
}

#[derive(Debug)]
pub struct CursorGeneric<'cursor, T, N>
where
    T: TreeTrait<'cursor, Node = N>,
    N: NodeTrait + 'cursor,
{
    tree: &'cursor T,
    path: Vec<usize>,
    current_node: Option<Box<N>>,
}

impl<'cursor, T, N> CursorGeneric<'cursor, T, N>
where
    T: TreeTrait<'cursor, Node = N>,
    N: NodeTrait + 'cursor,
{
    pub fn new(tree: &'cursor T) -> Self {
        let root = tree.root_node();
        Self {
            tree,
            path: vec![],
            current_node: Some(Box::new(root)),
        }
    }

    pub fn node(&self) -> Option<&Box<N>> {
        self.current_node.as_ref()
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

    // Get node at a specific path from root
    fn get_node_at_path(&self, path: &[usize]) -> Option<Box<N>> {
        let root = Box::new(self.tree.root_node());
        if path.is_empty() {
            return Some(root);
        }

        let mut current = root;
        for &index in path {
            current = current.get_child(index)?;
        }
        Some(current)
    }

    fn update_current_node(&mut self) {
        self.current_node = self.get_node_at_path(&self.path);
    }
}

impl<'cursor, T, N> CursorTrait for CursorGeneric<'cursor, T, N>
where
    T: TreeTrait<'cursor, Node = N>,
    N: NodeTrait + 'cursor,
{
    fn goto_first_child(&mut self) -> bool {
        if let Some(ref node) = self.current_node {
            if node.child_count() > 0 {
                self.path.push(0);
                self.update_current_node();
                return true;
            }
        }
        false
    }

    fn goto_next_sibling(&mut self) -> bool {
        if self.path.is_empty() {
            return false;
        }

        let last_index = self.path.len() - 1;
        let current_index = self.path[last_index];
        let next_index = current_index + 1;

        let parent_path = &self.path[..last_index];
        if let Some(parent) = self.get_node_at_path(parent_path) {
            if next_index < parent.child_count() {
                self.path[last_index] = next_index;
                self.update_current_node();
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    fn goto_parent(&mut self) -> bool {
        if !self.path.is_empty() {
            self.path.pop();
            self.update_current_node();
            true
        } else {
            false
        }
    }
}
