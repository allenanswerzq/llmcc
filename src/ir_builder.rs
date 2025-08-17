use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

use crate::{
    arena::{IrArena, NodeId, ScopeId},
    ir::{
        IrKind, IrKindNode, IrNodeBase, IrNodeFile, IrNodeId, IrNodeInternal, IrNodeRoot,
        IrNodeScope, IrNodeText, IrTree,
    },
    lang::AstContext,
    symbol::{Scope, ScopeStack, Symbol},
    visit::Visitor,
};

use std::num::NonZeroU16;
use std::sync::atomic::{AtomicI64, Ordering};

#[derive(Debug)]
struct IrBuilder<'a> {
    context: &'a mut AstContext,
    arena: &'a mut IrArena,
}

impl<'a> IrBuilder<'a> {
    fn new(context: &'a mut AstContext, arena: &'a mut IrArena) -> Self {
        Self {
            arena: arena,
            context: context,
        }
    }

    fn create_ast_node(&mut self, base: IrNodeBase, kind: IrKind, node: &Node) -> NodeId {
        match kind {
            IrKind::File => IrNodeFile::new(self.arena, base),
            IrKind::Text => {
                let text = self.context.file.get_text(base.start_byte, base.end_byte);
                IrNodeText::new(self.arena, base, text.unwrap())
            }
            IrKind::Internal => IrNodeInternal::new(self.arena, base),
            IrKind::Scope => {
                let text = self.context.file.get_text(base.start_byte, base.end_byte);
                let symbol = Symbol::new(self.arena, base.token_id, text.unwrap());
                let scope = Scope::new(self.arena, Some(symbol));
                let scope_node = IrNodeScope::new(self.arena, base, scope, None);
                self.arena.get_scope_mut(scope).unwrap().ast_node = Some(scope_node);
                scope_node
            }
            IrKind::IdentifierUse => {
                let text = self.context.file.get_text(base.start_byte, base.end_byte);
                let text = text.unwrap();
                let symbol = Symbol::new(self.arena, base.token_id, text);
                IrNodeId::new(self.arena, base, symbol)
            }
            _ => {
                panic!("unknown kind: {:?}", node)
            }
        }
    }

    fn create_base_node(&self, node: &Node, field_id: u16) -> IrNodeBase {
        let token_id = node.kind_id();
        let kind = self.context.language.get_token_kind(token_id);
        let arena_id = self.arena.get_next_node_id();

        IrNodeBase {
            arena_id,
            token_id,
            field_id: field_id,
            kind,
            start_pos: node.start_position().into(),
            end_pos: node.end_position().into(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            parent: None,
            children: vec![],
        }
    }

    pub fn field_id_of(node: &Node) -> Option<u16> {
        let parent = node.parent()?;
        let mut cursor = parent.walk();

        if cursor.goto_first_child() {
            loop {
                if cursor.node() == *node {
                    return cursor.field_id().map(|id| id.get());
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        None
    }
}

impl<'a> Visitor<'a, Tree> for IrBuilder<'_> {
    fn visit_node(&mut self, node: &mut Node<'a>, scope: &mut (), parent: NodeId) {
        let token_id = node.kind_id();
        let mut cursor = node.walk();
        let field_id = IrBuilder::field_id_of(&node).unwrap_or(65535);
        let kind = self.context.language.get_token_kind(token_id);

        let base = self.create_base_node(&node, field_id.into());
        let child = self.create_ast_node(base, kind, &node);

        self.arena.get_node_mut(parent).unwrap().add_child(child);
        self.arena.get_node_mut(child).unwrap().set_parent(parent);

        let parent = child;
        let children: Vec<_> = node.children(&mut cursor).collect();
        for mut child in children {
            self.visit_node(&mut child, &mut (), parent);
        }
    }
}

pub fn build_llmcc_ir(
    tree: &Tree,
    context: &mut AstContext,
    arena: &mut IrArena,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = IrNodeRoot::new(arena);
    let mut visitor = IrBuilder::new(context, arena);
    visitor.visit_node(&mut tree.root_node(), &mut (), root);
    Ok(())
}

#[derive(Debug)]
struct IrPrinter<'a> {
    context: &'a AstContext,
    depth: usize,
    output: String,
    arena: &'a mut IrArena,
}

impl<'a> IrPrinter<'a> {
    fn new(context: &'a AstContext, arena: &'a mut IrArena) -> Self {
        Self {
            context,
            depth: 0,
            output: String::new(),
            arena,
        }
    }

    fn get_output(&self) -> &str {
        &self.output
    }

    fn print_output(&self) {
        println!("{}", self.output);
    }

    fn visit_enter_node(&mut self, node: &IrKindNode, scope: &ScopeId, parent: &NodeId) {
        let base = node.get_base();
        let text = self.context.file.get_text(base.start_byte, base.end_byte);
        self.output.push_str(&"  ".repeat(self.depth));
        self.output.push('(');
        if let Some(mut text) = text {
            text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            self.output.push_str(&format!(
                "{}         |{}|",
                node.format_node(self.arena),
                text
            ));
        } else {
            self.output
                .push_str(&format!("{}", node.format_node(self.arena)));
        }

        if base.children.len() == 0 {
            self.output.push(')');
        } else {
            self.output.push('\n');
        }

        self.depth += 1;
    }

    fn visit_leave_node(&mut self, node: &IrKindNode, scope: &ScopeId, parent: &NodeId) {
        self.depth -= 1;
        if node.get_base().children.len() > 0 {
            self.output.push_str(&"  ".repeat(self.depth));
            self.output.push(')');
        }

        if self.depth > 0 {
            self.output.push('\n');
        }
    }
}

impl<'a> Visitor<'a, IrTree> for IrPrinter<'a> {
    fn visit_node(&mut self, node: &mut IrKindNode, scope: &mut ScopeId, parent: NodeId) {
        self.visit_enter_node(&node, &scope, &parent);

        let children = node.children(self.arena);
        for mut child in children {
            self.visit_node(&mut child, scope, parent);
        }

        self.visit_leave_node(&node, &scope, &parent);
    }
}

pub fn print_llmcc_ir(root: NodeId, context: &AstContext, arena: &mut IrArena) {
    let mut root = arena.get_node(root).unwrap().clone();
    let mut vistor = IrPrinter::new(context, arena);
    vistor.visit_node(&mut root, &mut ScopeId(0), NodeId(0));
    vistor.print_output();
}

pub fn find_declaration(root: NodeId, context: &AstContext, arena: &mut IrArena) {
    let global_scope = Scope::new(arena, None);
    let mut scope_stack = ScopeStack::new(global_scope);
    let root = arena.get_node(root).unwrap().clone();
    context
        .language
        .find_child_declaration(arena, &mut scope_stack, root);
}
