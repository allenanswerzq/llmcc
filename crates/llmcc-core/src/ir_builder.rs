use std::marker::PhantomData;
use tree_sitter::{Node, Tree};

use crate::context::{Context, ParentedNode};
use crate::ir::{
    HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirScope, HirText,
};
use crate::lang_def::LanguageTrait;

#[derive(Debug)]
struct HirBuilder<'ctx, Language> {
    ctx: Context<'ctx>,
    next_id: u32,
    _language: PhantomData<Language>,
}

impl<'ctx, Language: LanguageTrait> HirBuilder<'ctx, Language> {
    fn new(ctx: Context<'ctx>) -> Self {
        Self {
            ctx,
            next_id: 0,
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

        let ctx = self.ctx();
        ctx.hir_map
            .borrow_mut()
            .insert(current_id, ParentedNode::new(hir_node));

        current_id
    }

    fn reserve_id(&mut self) -> HirId {
        let id = HirId(self.next_id);
        self.next_id += 1;
        id
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
        self.ctx.file.get_text(start, end)
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

#[derive(Debug)]
struct HirPrinter<'tcx> {
    ctx: Context<'tcx>,
    depth: usize,
    ast: String,
    hir: String,
}

impl<'tcx> HirPrinter<'tcx> {
    fn new(ctx: Context<'tcx>) -> Self {
        Self {
            ctx,
            depth: 0,
            ast: String::new(),
            hir: String::new(),
        }
    }

    pub fn field_name_of(&self, node: &Node) -> Option<String> {
        let parent = node.parent()?;
        let mut cursor = parent.walk();

        if cursor.goto_first_child() {
            loop {
                if cursor.node().id() == node.id() {
                    return cursor.field_name().map(|name| name.to_string());
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        None
    }

    pub fn field_id_of(&self, node: &Node) -> Option<u16> {
        let parent = node.parent()?;
        let mut cursor = parent.walk();

        if cursor.goto_first_child() {
            loop {
                if cursor.node().id() == node.id() {
                    return cursor.field_id().map(|id| id.get());
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        None
    }

    fn format_ast(&mut self, node: &Node<'tcx>) {
        let kind = node.kind();
        let kind_id = node.kind_id();
        let field_name = self.field_name_of(&node);
        let start = node.start_byte();
        let end = node.end_byte();

        let indent = "  ".repeat(self.depth);
        let mut label = "".to_string();
        if let Some(field_name) = field_name {
            let field_id = self.field_id_of(&node).unwrap();
            label.push_str(&format!("({}_{}):{}", field_name, field_id, kind));
        } else {
            label.push_str(&format!("{}", kind));
        }
        label.push_str(&format!(" [{}]", kind_id));

        let snippet = self
            .ctx
            .file
            .opt_get_text(node.start_byte(), node.end_byte())
            .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "));

        const SNIPPET_COL: usize = 60;
        let mut line = format!("{}({}", indent, label);

        if let Some(text) = snippet {
            let padding = SNIPPET_COL.saturating_sub(line.len());
            line.push_str(&" ".repeat(padding));
            line.push('|');
            let trunc = 70;
            line.push_str(&text[..trunc.min(text.len())]);
            if text.len() > trunc {
                line.push_str("...");
            }
            line.push('|');
        }

        if node.child_count() == 0 {
            // For leaf nodes, include text content
            let text = self.ctx.file.get_text(start, end);
            line.push_str(&format!(" \"{}\"", text));
            line.push(')');
        } else {
            line.push('\n');
        }
        self.ast.push_str(&line);
    }

    fn format_hir(&mut self, node: &HirNode<'tcx>) {
        let indent = "  ".repeat(self.depth);
        let label = format!("{}", node.format_node(self.ctx));

        let snippet = self
            .ctx
            .file
            .opt_get_text(node.start_byte(), node.end_byte())
            .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "));

        const SNIPPET_COL: usize = 60;
        let mut line = format!("{}({}", indent, label);

        if let Some(text) = snippet {
            let padding = SNIPPET_COL.saturating_sub(line.len());
            line.push_str(&" ".repeat(padding));
            line.push('|');
            let trunc = 70;
            line.push_str(&text[..trunc.min(text.len())]);
            if text.len() > trunc {
                line.push_str("...");
            }
            line.push('|');
        }

        if node.child_count() == 0 {
            line.push(')');
        }
        self.hir.push_str(&line);
        if node.child_count() != 0 {
            self.hir.push('\n');
        }
    }

    fn visit_node(&mut self, node: &HirNode<'tcx>) {
        self.format_ast(&node.inner_ts_node());
        self.format_hir(node);
        self.depth += 1;

        for id in node.children() {
            let child = self.ctx.hir_node(*id);
            self.visit_node(&child);
        }

        self.depth -= 1;
        if node.child_count() > 0 {
            self.ast.push_str(&"  ".repeat(self.depth));
            self.ast.push(')');

            self.hir.push_str(&"  ".repeat(self.depth));
            self.hir.push(')');
        }

        if self.depth > 0 {
            self.ast.push('\n');
            self.hir.push('\n');
        }
    }
}

pub fn print_llmcc_ir<'tcx>(root: HirId, ctx: Context<'tcx>) {
    let mut vistor = HirPrinter::new(ctx);
    let root_node = vistor.ctx.hir_node(root);
    vistor.visit_node(&root_node);
    println!("{}\n", vistor.ast);
    println!("{}\n", vistor.hir);
}
