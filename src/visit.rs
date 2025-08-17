use tree_sitter::{Node, Tree};

use crate::arena::NodeId;
use crate::lang::AstContext;

pub trait TreeTrait<'a> {
    type NodeType;
    type ScopeType;
    type ParentType;
}

pub trait Visitor<'a, T: TreeTrait<'a>> {
    fn visit_node(&mut self, _: &mut T::NodeType, _: &mut T::ScopeType, _: T::ParentType) {}
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

    pub fn field_name_of(&self, node: &Node) -> Option<String> {
        let parent = node.parent()?;
        let mut cursor = parent.walk();

        if cursor.goto_first_child() {
            loop {
                if cursor.node() == *node {
                    return cursor.field_name().map(|name| name.to_string());
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        None
    }

    fn visit_enter_node(&mut self, node: &mut Node<'a>, scope: &(), parent: &NodeId) {
        let kind = node.kind();
        let kind_id = node.kind_id();
        let field_name = self.field_name_of(&node);
        let start = node.start_byte();
        let end = node.end_byte();

        self.output.push_str(&"  ".repeat(self.depth));
        self.output.push('(');
        if let Some(field_name) = field_name {
            self.output.push_str(&format!("{}:{}", field_name, kind));
        } else {
            self.output.push_str(&format!("{}", kind));
        }
        self.output.push_str(&format!(" [{}]", kind_id));

        if node.child_count() == 0 {
            // For leaf nodes, include text content
            let text = self.context.file.get_text(start, end).unwrap();
            self.output.push_str(&format!(" \"{}\"", text));
            self.output.push(')');
        } else {
            self.output.push('\n');
        }
        self.depth += 1;
    }

    fn visit_leave_node(&mut self, node: &mut Node<'a>, scope: &(), parent: &NodeId) {
        self.depth -= 1;
        if node.child_count() > 0 {
            self.output.push_str(&"  ".repeat(self.depth));
            self.output.push(')');
        }
        if self.depth > 0 {
            self.output.push('\n');
        }
    }
}

impl<'a> TreeTrait<'a> for Tree {
    type NodeType = Node<'a>;
    type ScopeType = ();
    type ParentType = NodeId;
}

impl<'a> Visitor<'a, Tree> for AstPrinter<'a> {
    fn visit_node(&mut self, node: &mut Node<'a>, scope: &mut (), parent: NodeId) {
        self.visit_enter_node(node, &scope, &parent);

        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        for mut child in children {
            self.visit_node(&mut child, &mut (), parent);
        }

        self.visit_leave_node(node, &(), &parent);
    }
}

pub fn print_ast(tree: &Tree, context: &mut AstContext) {
    let mut visitor = AstPrinter::new(context);
    let mut root = tree.root_node();
    visitor.visit_node(&mut root, &mut (), NodeId(0));
    visitor.print_output();
}
