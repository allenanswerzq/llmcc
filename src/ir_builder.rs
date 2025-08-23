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
                let node = HirFile::new(base, "NONE".into());
                HirNode::File(arena.alloc(node))
            }
            HirKind::Text => {
                let text = file.get_text(start, end);
                let node = HirText::new(base, text);
                HirNode::Text(arena.alloc(node))
            }
            HirKind::Internal => {
                let node = HirInternal::new(base);
                HirNode::Internal(arena.alloc(node))
            }
            HirKind::Scope => {
                let node = HirScope::new(base);
                HirNode::Scope(arena.alloc(node))
            }
            HirKind::IdentUse => {
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

    fn build(&mut self, node: Node<'tcx>, parent: HirId, ctx: &'tcx Context<'tcx>) -> HirId {
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        let mut hirs = Vec::new();
        let hir_id = self.next_id();
        for child in children {
            hirs.push(self.build(child, hir_id, ctx));
        }

        let kind = Language::hir_kind(node.kind_id());
        let base = self.create_base(hir_id, node, kind, hirs);
        let hir_id = base.hir_id;
        let node = self.create_hir(base, node, kind, &ctx.file, &ctx.arena);
        self.hir_map.insert(hir_id, ParentedNode::new(parent, node));
        hir_id
    }
}

pub fn build_llmcc_ir<'tcx>(
    tree: &'tcx Tree,
    ctx: &'tcx Context<'tcx>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = HirBuilder::new();
    builder.build(tree.root_node(), HirId(0), ctx);
    *ctx.hir_map.borrow_mut() = builder.hir_map;
    Ok(())
}

#[derive(Debug)]
struct HirPrinter<'tcx> {
    ctx: &'tcx Context<'tcx>,
    depth: usize,
    ast: String,
    hir: String,
}

impl<'tcx> HirPrinter<'tcx> {
    fn new(ctx: &'tcx Context<'tcx>) -> Self {
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
                if cursor.node() == *node {
                    return cursor.field_name().map(|name| name.to_string());
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

        let mut indent = "  ".repeat(self.depth);
        let mut label = "".to_string();
        if let Some(field_name) = field_name {
            label.push_str(&format!("{}:{}", field_name, kind));
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

pub fn print_llmcc_ir<'tcx>(root: HirId, ctx: &'tcx Context<'tcx>) {
    let mut vistor = HirPrinter::new(ctx);
    vistor.visit_node(&ctx.hir_node(root));
    println!("{}\n", vistor.ast);
    println!("{}\n", vistor.hir);
}
