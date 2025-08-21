use std::collections::HashMap;
use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

use crate::context::TyCtxt;
use crate::ir::{
    Arena, HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirRoot, HirScope,
    HirText,
};
use crate::lang::Language;

#[derive(Debug, Clone)]
struct ParentedNode<'tcx> {
    pub parent: HirId,
    pub node: HirNode<'tcx>,
}

impl<'tcx> ParentedNode<'tcx> {
    pub fn new(parent: HirId, node: HirNode<'tcx>) -> Self {
        Self { parent, node }
    }
}

#[derive(Debug)]
struct HirBuilder<'tcx> {
    lang: Language<'tcx>,
    id: u32,
    hir_map: HashMap<HirId, ParentedNode<'tcx>>,
}

impl<'tcx> HirBuilder<'tcx> {
    fn new(ctx: &'tcx TyCtxt<'tcx>) -> Self {
        Self {
            lang: Language::new(ctx),
            id: 0,
            hir_map: HashMap::new(),
        }
    }

    fn create_hir(&mut self, base: HirBase<'tcx>, node: Node, kind: HirKind) -> HirNode<'tcx> {
        let start = base.node.start_byte();
        let end = base.node.end_byte();
        match kind {
            HirKind::File => {
                // file
                HirFile::new(&self.lang.ctx.arena, base, "NONE".into())
            }
            HirKind::Text => {
                let text = self.lang.ctx.file.get_text(start, end);
                HirText::new(&self.lang.ctx.arena, base, text)
            }
            HirKind::Internal => {
                // internal
                HirInternal::new(&self.lang.ctx.arena, base)
            }
            HirKind::Scope => {
                // scope
                HirScope::new(&self.lang.ctx.arena, base)
            }
            HirKind::IdentUse => {
                // ident
                let text = self.lang.ctx.file.get_text(start, end);
                HirIdent::new(&self.lang.ctx.arena, base, text)
            }
            _ => {
                panic!("unknown kind: {:?}", node)
            }
        }
    }

    fn next_id(&mut self) -> HirId {
        let ans = HirId(self.id);
        self.id += 1;
        ans
    }

    fn create_base(
        &mut self,
        hir_id: HirId,
        node: Node<'tcx>,
        kind: HirKind,
        children: Vec<HirId>,
    ) -> HirBase<'tcx> {
        let field_id = self.field_id_of(&node).unwrap_or(65535);

        HirBase {
            hir_id,
            node,
            kind,
            field_id,
            children,
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

    fn visit_node(&mut self, node: Node<'tcx>, parent: HirId) -> HirId {
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        let mut hirs = Vec::new();
        let hir_id = self.next_id();
        for child in children {
            hirs.push(self.visit_node(child, hir_id));
        }

        let kind = self.lang.hir_kind(node.kind_id());
        let base = self.create_base(hir_id, node, kind, hirs);
        let hir_id = base.hir_id;
        let node = self.create_hir(base, node, kind);
        self.hir_map.insert(hir_id, ParentedNode::new(parent, node));
        hir_id
    }
}

pub fn build_llmcc_ir<'tcx>(
    tree: &'tcx Tree,
    ctx: &'tcx mut TyCtxt<'tcx>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut visitor = HirBuilder::new(ctx);
    visitor.visit_node(tree.root_node(), HirId(0));
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
