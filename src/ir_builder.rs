use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

use crate::{
    CursorGeneric,
    arena::{ArenaIdNode, IrArena},
    ir::{
        IrKind, IrKindNode, IrNodeBase, IrNodeFile, IrNodeId, IrNodeInternal, IrNodeRoot,
        IrNodeScope, IrNodeText,
    },
    lang::AstContext,
    symbol::{Scope, ScopeStack, Symbol},
    visit::{NodeTrait, Visitor, dfs},
};

use std::num::NonZeroU16;
use std::sync::atomic::{AtomicI64, Ordering};

static DEBUG_ID_COUNTER: AtomicI64 = AtomicI64::new(0);

fn get_debug_id() -> i64 {
    let value = DEBUG_ID_COUNTER.load(Ordering::SeqCst);
    DEBUG_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    value
}

#[derive(Debug)]
struct IrBuilder<'a> {
    stack: Vec<ArenaIdNode>,
    context: &'a mut AstContext,
    arena: &'a mut IrArena,
}

impl<'a> IrBuilder<'a> {
    fn new(context: &'a mut AstContext, arena: &'a mut IrArena) -> Self {
        let root_id = IrNodeRoot::new(arena);
        Self {
            arena: arena,
            stack: vec![root_id],
            context: context,
        }
    }

    fn create_ast_node(&mut self, base: IrNodeBase, kind: IrKind, node: &Node) -> ArenaIdNode {
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
                let scope = Scope::new(self.arena, symbol);
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
        let debug_id = get_debug_id();

        IrNodeBase {
            arena_id,
            debug_id,
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
}

impl<'a> Visitor<TreeCursor<'a>> for IrBuilder<'_> {
    fn visit_node(&mut self, cursor: &mut TreeCursor<'a>) {
        let node = cursor.node();
        let token_id = node.kind_id();
        let field_id = cursor.field_id().unwrap_or(NonZeroU16::new(65535).unwrap());
        let kind = self.context.language.get_token_kind(token_id);

        let base = self.create_base_node(&node, field_id.into());
        let child = self.create_ast_node(base, kind, &node);

        let parent = self.stack[self.stack.len() - 1];
        self.arena.get_node_mut(parent).unwrap().add_child(child);
        self.arena.get_node_mut(child).unwrap().set_parent(parent);

        if node.child_count() > 0 {
            self.stack.push(child);
        }
    }

    fn visit_leave_node(&mut self, cursor: &mut TreeCursor<'a>) {
        let node = cursor.node();

        // Pop the current node from the stack when we're done with it
        if node.child_count() > 0 {
            if let Some(_completed_node) = self.stack.pop() {
                // let mut arena_mut = self.arena.borrow_mut();
                // arena_mut.get_mut(completed_node).unwrap().add_child(child);
                // self.finalize_node(&completed_node);
            }
        }
    }
}

pub fn build_llmcc_ir(
    tree: &Tree,
    context: &mut AstContext,
    arena: &mut IrArena,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut vistor = IrBuilder::new(context, arena);
    let mut cursor = tree.walk();
    dfs(&mut cursor, &mut vistor);
    Ok(())
}

type IrCursor<'a> = CursorGeneric<'a, IrKindNode, Symbol, Scope>;

#[derive(Debug)]
struct IrPrinter<'a> {
    context: &'a AstContext,
    depth: usize,
    output: String,
}

impl<'a> IrPrinter<'a> {
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

impl<'a> Visitor<IrCursor<'a>> for IrPrinter<'a> {
    fn visit_enter_node(&mut self, cursor: &mut IrCursor<'a>) {
        let node = cursor.node().clone(); // Can we remove this clone here?
        let base = node.get_base();
        let text = self.context.file.get_text(base.start_byte, base.end_byte);
        self.output.push_str(&"  ".repeat(self.depth));
        self.output.push('(');
        if let Some(mut text) = text {
            text = text.replace("\n", "");
            self.output.push_str(&format!(
                "{} |{}|",
                node.format_node(cursor.get_arena()),
                text
            ));
        } else {
            self.output
                .push_str(&format!("{}", node.format_node(cursor.get_arena())));
        }

        if node.child_count() == 0 {
            self.output.push(')');
        } else {
            self.output.push('\n');
        }

        self.depth += 1;
    }

    fn visit_leave_node(&mut self, cursor: &mut IrCursor<'a>) {
        self.depth -= 1;
        let node = cursor.node();

        if node.child_count() > 0 {
            self.output.push_str(&"  ".repeat(self.depth));
            self.output.push(')');
        }

        if self.depth > 0 {
            self.output.push('\n');
        }
    }

    fn visit_node(&mut self, _cursor: &mut IrCursor<'a>) {}
}

pub fn print_llmcc_ir(root: ArenaIdNode, context: &AstContext, arena: &mut IrArena) {
    let mut vistor = IrPrinter::new(context);
    let mut cursor = IrCursor::new(root, arena);
    dfs(&mut cursor, &mut vistor);
    vistor.print_output();
}
