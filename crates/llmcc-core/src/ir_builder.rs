use std::marker::PhantomData;

use tree_sitter::Node;

use crate::context::CompileUnit;
use crate::ir::{
    HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirScope, HirText,
};
use crate::lang_def::LanguageTrait;

#[derive(Debug)]
struct HirBuilder<'a, Language> {
    unit: CompileUnit<'a>,
    _language: PhantomData<Language>,
}

impl<'a, Language: LanguageTrait> HirBuilder<'a, Language> {
    fn new(unit: CompileUnit<'a>) -> Self {
        unit.register_file_start();
        Self {
            unit,
            _language: PhantomData,
        }
    }

    fn unit(&self) -> CompileUnit<'a> {
        self.unit
    }

    fn build_node(&mut self, node: Node<'a>, parent: Option<HirId>) -> HirId {
        let current_id = self.reserve_id();
        let child_ids = self.collect_children(node, current_id);

        let kind = Language::hir_kind(node.kind_id());
        let base = self.make_base(current_id, parent, node, kind, child_ids);
        let hir_node = self.make_hir_node(base, node, kind);

        self.unit.insert_hir_node(current_id, hir_node);

        current_id
    }

    fn reserve_id(&mut self) -> HirId {
        self.unit.reserve_hir_id()
    }

    fn collect_children(&mut self, node: Node<'a>, parent: HirId) -> Vec<HirId> {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .map(|child| self.build_node(child, Some(parent)))
            .collect()
    }

    fn make_base(
        &self,
        hir_id: HirId,
        parent: Option<HirId>,
        node: Node<'a>,
        kind: HirKind,
        children: Vec<HirId>,
    ) -> HirBase<'a> {
        let field_id = Self::field_id_of(node).unwrap_or(u16::MAX);
        HirBase {
            hir_id,
            parent,
            node,
            kind,
            field_id,
            children,
        }
    }

    fn make_hir_node(
        &self,
        base: HirBase<'a>,
        ts_node: Node<'a>,
        kind: HirKind,
    ) -> HirNode<'a> {
        match kind {
            HirKind::File => {
                let file_node = HirFile::new(base, "NONE".into());
                HirNode::File(self.unit.cc.arena.alloc(file_node))
            }
            HirKind::Text => {
                let text = self.extract_text(&base);
                let text_node = HirText::new(base, text);
                HirNode::Text(self.unit.cc.arena.alloc(text_node))
            }
            HirKind::Internal => {
                let internal = HirInternal::new(base);
                HirNode::Internal(self.unit.cc.arena.alloc(internal))
            }
            HirKind::Scope => {
                let scope = self.make_scope(base);
                HirNode::Scope(self.unit.cc.arena.alloc(scope))
            }
            HirKind::Identifier => {
                let text = self.extract_text(&base);
                let ident = HirIdent::new(base, text);
                HirNode::Ident(self.unit.cc.arena.alloc(ident))
            }
            other => panic!("unsupported HIR kind for node {:?}", (other, ts_node)),
        }
    }

    fn make_scope(&self, base: HirBase<'a>) -> HirScope<'a> {
        let fields = [Language::name_field(), Language::type_field()];
        let ident = base
            .opt_child_by_fields(self.unit(), &fields)
            .map(|node| node.expect_ident());
        HirScope::new(base, ident)
    }

    fn extract_text(&self, base: &HirBase<'a>) -> String {
        let start = base.node.start_byte();
        let end = base.node.end_byte();
        self.unit.file().get_text(start, end)
    }

    fn field_id_of(node: Node<'_>) -> Option<u16> {
        let parent = node.parent()?;
        let mut cursor = parent.walk();

        if !cursor.goto_first_child() {
            return None;
        }

        loop {
            if cursor.node().id() == node.id() {
                return cursor.field_id().map(|id| id.get());
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }

        None
    }
}

pub fn build_llmcc_ir<'a, L: LanguageTrait>(
    unit: CompileUnit<'a>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = HirBuilder::<L>::new(unit);
    let root = unit.tree().root_node();
    builder.build_node(root, None);
    Ok(())
}
