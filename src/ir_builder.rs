use std::collections::HashMap;
use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

use crate::context::{Context, ParentedNode};
use crate::file::File;
use crate::ir::{
    Arena, HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirRoot, HirScope,
    HirText,
};
use crate::lang::Language;

#[derive(Debug)]
struct HirBuilder<'tcx> {
    id: u32,
    hir_map: HashMap<HirId, ParentedNode<'tcx>>,
}

impl<'tcx> HirBuilder<'tcx> {
    fn new() -> Self {
        Self {
            id: 0,
            hir_map: HashMap::new(),
        }
    }

    fn create_hir(
        &mut self,
        base: HirBase<'tcx>,
        node: Node,
        kind: HirKind,
        file: &File,
        arena: &'tcx Arena<'tcx>,
    ) -> HirNode<'tcx> {
        let start = base.node.start_byte();
        let end = base.node.end_byte();
        match kind {
            HirKind::File => {
                // file
                let node = HirFile::new(base, "NONE".into());
                HirNode::File(arena.alloc(node))
            }
            HirKind::Text => {
                let text = file.get_text(start, end);
                let node = HirText::new(base, text);
                HirNode::Text(arena.alloc(node))
            }
            HirKind::Internal => {
                // internal
                let node = HirInternal::new(base);
                HirNode::Internal(arena.alloc(node))
            }
            HirKind::Scope => {
                // scope
                let node = HirScope::new(base);
                HirNode::Scope(arena.alloc(node))
            }
            HirKind::IdentUse => {
                // ident
                let text = file.get_text(start, end);
                let node = HirIdent::new(base, text);
                HirNode::Ident(arena.alloc(node))
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

    fn build(
        &mut self,
        node: Node<'tcx>,
        parent: HirId,
        file: &File,
        arena: &'tcx Arena<'tcx>,
    ) -> HirId {
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        let mut hirs = Vec::new();
        let hir_id = self.next_id();
        for child in children {
            hirs.push(self.build(child, hir_id, file, arena));
        }

        let kind = Language::hir_kind(node.kind_id());
        let base = self.create_base(hir_id, node, kind, hirs);
        let hir_id = base.hir_id;
        let node = self.create_hir(base, node, kind, file, arena);
        self.hir_map.insert(hir_id, ParentedNode::new(parent, node));
        hir_id
    }
}

pub fn build_llmcc_ir<'tcx>(
    tree: &'tcx Tree,
    ctx: &'tcx mut Context<'tcx>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = HirBuilder::new();
    builder.build(tree.root_node(), HirId(0), &ctx.file, &ctx.arena);
    ctx.hir_map = builder.hir_map;
    Ok(())
}

#[derive(Debug)]
struct HirPrinter<'tcx> {
    context: &'tcx Context<'tcx>,
    depth: usize,
    output_ast: String,
    output_hir: String,
}

impl<'tcx> HirPrinter<'tcx> {
    fn new(context: &'tcx Context<'tcx>) -> Self {
        Self {
            context,
            depth: 0,
            output_ast: String::new(),
            output_hir: String::new(),
        }
    }

    fn ouptut_ast(&self) -> &str {
        &self.output_ast
    }

    fn output_hir(&self) -> &str {
        &self.output_hir
    }

    fn format_ast(&mut self) {}

    fn visit_enter_node(&mut self, node: &HirNode<'tcx>) {
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

    fn visit_leave_node(&mut self, node: &HirKindNode, scope: &ScopeId, parent: &NodeId) {
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

// pub fn print_llmcc_ir(root: NodeId, context: &AstContext, arena: &mut HirArena) {
//     let mut root = arena.get_node(root).unwrap().clone();
//     let mut vistor = HirPrinter::new(context, arena);
//     vistor.visit_node(&mut root, &mut ScopeId(0), NodeId(0));
//     vistor.print_output();
// }
