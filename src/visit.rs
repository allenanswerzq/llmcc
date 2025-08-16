use tree_sitter::{Node, Tree};

use crate::arena::{Arena, ArenaIdNode};
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

    fn visit_enter_node(&mut self, node: &mut Node<'a>, scope: &(), parent: &ArenaIdNode) {
        let kind = node.kind();
        let kind_id = node.kind_id();
        let start = node.start_byte();
        let end = node.end_byte();

        self.output.push_str(&"  ".repeat(self.depth));
        self.output.push('(');
        self.output.push_str(kind);
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

    fn visit_leave_node(&mut self, node: &mut Node<'a>, scope: &(), parent: &ArenaIdNode) {
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
    type ParentType = ArenaIdNode;
}

impl<'a> Visitor<'a, Tree> for AstPrinter<'a> {
    fn visit_node(&mut self, node: &mut Node<'a>, scope: &mut (), parent: ArenaIdNode) {
        self.visit_enter_node(node, &scope, &parent);

        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        for mut child in children {
            self.visit_node(&mut child, &mut (), parent);
        }

        // let mut cursor = node.walk();
        // if cursor.goto_first_child() {
        //     loop {
        //         let child_node = cursor.node();
        //         self.visit_node(child_node, scope.clone(), parent);
        //         if !cursor.goto_next_sibling() {
        //             break;
        //         }
        //     }
        //     cursor.goto_parent();
        // }

        self.visit_leave_node(node, &(), &parent);
    }
}

pub fn print_ast(tree: &Tree, context: &mut AstContext) {
    let mut visitor = AstPrinter::new(context);
    let mut root = tree.root_node();
    visitor.visit_node(&mut root, &mut (), ArenaIdNode(0));
    visitor.print_output();
}
