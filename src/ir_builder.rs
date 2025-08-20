use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

use crate::ir::{
    Arena, HirBase, HirFile, HirIdent, HirInternal, HirKind, HirNode, HirRoot, HirScope, HirText,
};

// #[derive(Debug)]
struct HirBuilder<'tcx> {
    // context: &'a mut AstContext,
    arena: &'tcx Arena<'tcx>,
}

impl<'tcx> HirBuilder<'tcx> {
    fn new(arena: &'tcx Arena<'tcx>) -> Self {
        Self { arena }
    }

    fn create_hir(&mut self, base: HirBase<'tcx>, node: &Node, kind: HirKind) -> HirNode<'tcx> {
        match kind {
            HirKind::File => HirFile::new(self.arena, base, "NONE".into()),
            HirKind::Text => {
                // let text = self.context.file.get_text(base.start_byte, base.end_byte);
                HirText::new(self.arena, base, "NONE".into())
            }
            HirKind::Internal => HirInternal::new(self.arena, base),
            HirKind::Scope => {
                HirText::new(self.arena, base, "NONE".into())
                // let text = self.context.file.get_text(base.start_byte, base.end_byte);
                // let id = self.arena.get_next_node_id();
                // let symbol = Symbol::new(self.arena, base.token_id, "NONE".into(), id);
                // let scope = Scope::new(self.arena, Some(symbol));
                // let scope_node = HirScope::new(self.arena, base, scope, None);
                // self.arena.get_scope_mut(scope).unwrap().ast_node = Some(scope_node);
                // scope_node
            }
            HirKind::IdentUse => {
                HirText::new(self.arena, base, "NONE".into())
                // let text = self.context.file.get_text(base.start_byte, base.end_byte);
                // let text = "NONE".into();
                // let id = self.arena.get_next_node_id();
                // let symbol = Symbol::new(self.arena, base.token_id, text, id);
                // HirId::new(self.arena, base, symbol)
            }
            _ => {
                panic!("unknown kind: {:?}", node)
            }
        }
    }

    fn create_base(&self, node: &Node, kind: HirKind) -> HirBase<'tcx> {
        let token_id = node.kind_id();
        let field_id = self.field_id_of(&node).unwrap_or(65535);

        HirBase {
            token_id,
            field_id: field_id,
            kind,
            start_pos: node.start_position().into(),
            end_pos: node.end_position().into(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            children: vec![],
        }
    }

    pub fn field_id_of(&self, node: &Node) -> Option<u16> {
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

    fn visit_node(&mut self, node: &mut Node<'tcx>) -> HirNode<'tcx> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        let mut hir_children = vec![];
        for mut child in children {
            hir_children.push(self.visit_node(&mut child));
        }

        // let kind = self.context.language.get_token_kind(token_id);
        let kind = HirKind::Comment;
        let mut base = self.create_base(&node, kind);
        base.children = hir_children;
        self.create_hir(base, &node, kind)
    }
}

pub fn build_llmcc_ir<'tcx>(
    tree: &'tcx Tree,
    // context: &mut AstContext,
    arena: &'tcx Arena<'tcx>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut visitor = HirBuilder::new(arena);
    visitor.visit_node(&mut tree.root_node());
    Ok(())
}

// #[derive(Debug)]
// struct HirPrinter<'a> {
//     context: &'a AstContext,
//     depth: usize,
//     output: String,
//     arena: &'a mut HirArena,
// }

// impl<'a> HirPrinter<'a> {
//     fn new(context: &'a AstContext, arena: &'a mut HirArena) -> Self {
//         Self {
//             context,
//             depth: 0,
//             output: String::new(),
//             arena,
//         }
//     }

//     fn get_output(&self) -> &str {
//         &self.output
//     }

//     fn print_output(&self) {
//         println!("{}", self.output);
//     }

//     fn visit_enter_node(&mut self, node: &HirKindNode, scope: &ScopeId, parent: &NodeId) {
//         let base = node.get_base();
//         let text = self.context.file.get_text(base.start_byte, base.end_byte);
//         self.output.push_str(&"  ".repeat(self.depth));
//         self.output.push('(');
//         if let Some(mut text) = text {
//             text = text.split_whitespace().collect::<Vec<_>>().join(" ");
//             self.output.push_str(&format!(
//                 "{}         |{}|",
//                 node.format_node(self.arena),
//                 text
//             ));
//         } else {
//             self.output
//                 .push_str(&format!("{}", node.format_node(self.arena)));
//         }

//         if base.children.len() == 0 {
//             self.output.push(')');
//         } else {
//             self.output.push('\n');
//         }

//         self.depth += 1;
//     }

//     fn visit_leave_node(&mut self, node: &HirKindNode, scope: &ScopeId, parent: &NodeId) {
//         self.depth -= 1;
//         if node.get_base().children.len() > 0 {
//             self.output.push_str(&"  ".repeat(self.depth));
//             self.output.push(')');
//         }

//         if self.depth > 0 {
//             self.output.push('\n');
//         }
//     }
// }

// impl<'a> Visitor<'a, HirTree> for HirPrinter<'a> {
//     fn visit_node(&mut self, node: &mut HirKindNode, scope: &mut ScopeId, parent: NodeId) {
//         self.visit_enter_node(&node, &scope, &parent);

//         let children = node.children(self.arena);
//         for mut child in children {
//             self.visit_node(&mut child, scope, parent);
//         }

//         self.visit_leave_node(&node, &scope, &parent);
//     }
// }

// pub fn print_llmcc_ir(root: NodeId, context: &AstContext, arena: &mut HirArena) {
//     let mut root = arena.get_node(root).unwrap().clone();
//     let mut vistor = HirPrinter::new(context, arena);
//     vistor.visit_node(&mut root, &mut ScopeId(0), NodeId(0));
//     vistor.print_output();
// }

// pub fn find_declaration(root: NodeId, context: &AstContext, arena: &mut HirArena) {
//     let global_scope = Scope::new(arena, None);
//     let mut scope_stack = ScopeStack::new(global_scope);
//     let root = arena.get_node(root).unwrap().clone();
//     context
//         .language
//         .find_child_declaration(arena, &mut scope_stack, root);
// }
