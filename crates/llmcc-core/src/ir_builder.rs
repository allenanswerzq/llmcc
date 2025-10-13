use std::marker::PhantomData;
use tree_sitter::{Node, Tree};

use crate::context::Context;
use crate::ir::{
    HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirScope, HirText,
};
use crate::lang_def::LanguageTrait;

#[derive(Debug)]
struct HirBuilder<'ctx, Language> {
    ctx: Context<'ctx>,
    _language: PhantomData<Language>,
}

impl<'ctx, Language: LanguageTrait> HirBuilder<'ctx, Language> {
    fn new(ctx: Context<'ctx>) -> Self {
        ctx.register_file_start();
        Self {
            ctx,
            _language: PhantomData,
        }
    }

    fn ctx(&self) -> Context<'ctx> {
        self.ctx
    }

    fn build_node(&mut self, node: Node<'ctx>, parent: Option<HirId>) -> HirId {
        let current_id = self.reserve_id();
        let child_ids = self.collect_children(node, current_id);

        let kind = Language::hir_kind(node.kind_id());
        let base = self.make_base(current_id, parent, node, kind, child_ids);
        let hir_node = self.make_hir_node(base, node, kind);

        self.ctx.insert_hir_node(current_id, hir_node);

        current_id
    }

    fn reserve_id(&mut self) -> HirId {
        self.ctx.reserve_hir_id()
    }

    fn collect_children(&mut self, node: Node<'ctx>, parent: HirId) -> Vec<HirId> {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .map(|child| self.build_node(child, Some(parent)))
            .collect()
    }

    fn make_base(
        &self,
        hir_id: HirId,
        parent: Option<HirId>,
        node: Node<'ctx>,
        kind: HirKind,
        children: Vec<HirId>,
    ) -> HirBase<'ctx> {
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
        base: HirBase<'ctx>,
        ts_node: Node<'ctx>,
        kind: HirKind,
    ) -> HirNode<'ctx> {
        match kind {
            HirKind::File => {
                let file_node = HirFile::new(base, "NONE".into());
                HirNode::File(self.ctx.gcx.arena.alloc(file_node))
            }
            HirKind::Text => {
                let text = self.extract_text(&base);
                let text_node = HirText::new(base, text);
                HirNode::Text(self.ctx.gcx.arena.alloc(text_node))
            }
            HirKind::Internal => {
                let internal = HirInternal::new(base);
                HirNode::Internal(self.ctx.gcx.arena.alloc(internal))
            }
            HirKind::Scope => {
                let scope = self.make_scope(base);
                HirNode::Scope(self.ctx.gcx.arena.alloc(scope))
            }
            HirKind::Identifier => {
                let text = self.extract_text(&base);
                let ident = HirIdent::new(base, text);
                HirNode::Ident(self.ctx.gcx.arena.alloc(ident))
            }
            other => panic!("unsupported HIR kind for node {:?}", (other, ts_node)),
        }
    }

    fn make_scope(&self, base: HirBase<'ctx>) -> HirScope<'ctx> {
        let fields = [Language::name_field(), Language::type_field()];
        let ident = base
            .opt_child_by_fields(self.ctx(), &fields)
            .map(|node| node.expect_ident());
        HirScope::new(base, ident)
    }

    fn extract_text(&self, base: &HirBase<'ctx>) -> String {
        let start = base.node.start_byte();
        let end = base.node.end_byte();
        self.ctx.file().get_text(start, end)
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

pub fn build_llmcc_ir<'ctx, L: LanguageTrait>(
    tree: &'ctx Tree,
    ctx: Context<'ctx>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = HirBuilder::<L>::new(ctx);
    builder.build_node(tree.root_node(), None);
    Ok(())
}
