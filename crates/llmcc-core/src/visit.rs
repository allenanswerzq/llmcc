//! Visitor pattern implementation for HIR (High-level Intermediate Representation) traversal.
use crate::CompileUnit;
use crate::graph_builder::BlockId;
use crate::ir::{HirKind, HirNode};

pub trait HirVisitor<'v> {
    fn visit_children(&mut self, unit: CompileUnit<'v>, node: HirNode<'v>, parent: BlockId) {
        let children = node.child_ids();
        for child_id in children {
            let child = unit.hir_node(*child_id);
            // Use stacker to grow stack for deeply nested structures
            stacker::maybe_grow(32 * 1024, 1024 * 1024, || {
                self.visit_node(unit, child, parent);
            });
        }
    }

    fn visit_file(&mut self, unit: CompileUnit<'v>, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(unit, node, parent);
    }

    fn visit_scope(&mut self, unit: CompileUnit<'v>, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(unit, node, parent);
    }

    fn visit_text(&mut self, unit: CompileUnit<'v>, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(unit, node, parent);
    }

    fn visit_internal(&mut self, unit: CompileUnit<'v>, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(unit, node, parent);
    }

    fn visit_undefined(&mut self, unit: CompileUnit<'v>, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(unit, node, parent);
    }

    fn visit_ident(&mut self, unit: CompileUnit<'v>, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(unit, node, parent);
    }

    fn visit_node(&mut self, unit: CompileUnit<'v>, node: HirNode<'v>, parent: BlockId) {
        match node.kind() {
            HirKind::File => self.visit_file(unit, node, parent),
            HirKind::Scope => self.visit_scope(unit, node, parent),
            HirKind::Text => self.visit_text(unit, node, parent),
            HirKind::Internal => self.visit_internal(unit, node, parent),
            HirKind::Undefined => self.visit_undefined(unit, node, parent),
            HirKind::Identifier => self.visit_ident(unit, node, parent),
            _ => {
                eprintln!("Unhandled node kind: {}", node.format(unit));
            }
        }
    }
}
