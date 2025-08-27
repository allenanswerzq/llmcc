use crate::Context;
use crate::block::{BlockId, BlockKind};
use crate::ir::{
    HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirRoot, HirScope, HirText,
};

pub trait HirVisitor<'v> {
    fn ctx(&self) -> &'v Context<'v>;

    fn visit_children(&mut self, node: HirNode<'v>, parent: BlockId) {
        let children = node.children();
        for child_id in children {
            let child = self.ctx().hir_node(*child_id);
            self.visit_node(child, parent);
        }
    }

    fn visit_file(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }
    fn visit_scope(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }
    fn visit_text(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }
    fn visit_internal(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }
    fn visit_undefined(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }
    fn visit_ident(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }

    fn visit_node(&mut self, node: HirNode<'v>, parent: BlockId) {
        // println!("{}", node.format_node(self.ctx()));
        match node.kind() {
            HirKind::File => self.visit_file(node, parent),
            HirKind::Scope => self.visit_scope(node, parent),
            HirKind::Text => self.visit_text(node, parent),
            HirKind::Internal => self.visit_internal(node, parent),
            HirKind::Undefined => self.visit_undefined(node, parent),
            HirKind::IdentUse => self.visit_ident(node, parent),
            _ => {
                todo!("{}", node.format_node(self.ctx()))
            }
        }
    }
}
