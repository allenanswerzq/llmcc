use std::marker::PhantomData;

use tree_sitter::Node;

use crate::context::{CompileCtxt, CompileUnit};
use crate::ir::{
    HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirScope, HirText,
};
use crate::lang_def::LanguageTrait;

/// Intermediate representation of HirNode data before arena allocation
#[derive(Debug, Clone)]
pub enum HirNodeData {
    File(String),
    Text(String),
    Internal,
    Scope(Option<String>),
    Identifier(String),
}

/// Builder that directly assigns HIR nodes to compile context
struct HirBuilder<'a, Language> {
    unit: CompileUnit<'a>,
    file_path: Option<String>,
    file_content: String,
    _language: PhantomData<Language>,
}

impl<'a, Language: LanguageTrait> HirBuilder<'a, Language> {
    /// Create a new builder that directly assigns to context
    fn new(unit: CompileUnit<'a>, file_path: Option<String>, file_content: String) -> Self {
        Self {
            unit,
            file_path,
            file_content,
            _language: PhantomData,
        }
    }

    fn build(mut self, root: Node<'a>) -> HirId {
        let file_start_id = self.build_node(root, None);
        self.unit.cc.set_file_start(self.unit.index, file_start_id);
        file_start_id
    }

    fn build_node(&mut self, node: Node<'a>, parent: Option<HirId>) -> HirId {
        let hir_id = self.unit.reserve_hir_id();
        let child_ids = self.collect_children(node, hir_id);

        let kind = Language::hir_kind(node.kind_id());
        let base = self.make_base(hir_id, parent, node, kind, child_ids);
        let data = self.make_hir_node_data(&base, node, kind);

        let hir_node = match data {
            HirNodeData::File(path) => {
                let file_node = HirFile::new(base, path);
                HirNode::File(self.unit.cc.arena.alloc(file_node))
            }
            HirNodeData::Text(text) => {
                let text_node = HirText::new(base, text);
                HirNode::Text(self.unit.cc.arena.alloc(text_node))
            }
            HirNodeData::Internal => {
                let internal = HirInternal::new(base);
                HirNode::Internal(self.unit.cc.arena.alloc(internal))
            }
            HirNodeData::Scope(_) => {
                let scope = HirScope::new(base, None);
                HirNode::Scope(self.unit.cc.arena.alloc(scope))
            }
            HirNodeData::Identifier(text) => {
                let ident = HirIdent::new(base, text);
                HirNode::Ident(self.unit.cc.arena.alloc(ident))
            }
        };

        self.unit.insert_hir_node(hir_id, hir_node);
        hir_id
    }

    fn collect_children(&mut self, node: Node<'a>, _parent: HirId) -> Vec<HirId> {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .map(|child| self.build_node(child, None))
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

    fn make_hir_node_data(&self, base: &HirBase<'a>, ts_node: Node<'a>, kind: HirKind) -> HirNodeData {
        match kind {
            HirKind::File => {
                HirNodeData::File(self.file_path.clone().unwrap_or_default())
            }
            HirKind::Text => {
                let text = self.extract_text(&base);
                HirNodeData::Text(text)
            }
            HirKind::Internal => {
                HirNodeData::Internal
            }
            HirKind::Scope => {
                HirNodeData::Scope(None)
            }
            HirKind::Identifier => {
                let text = self.extract_text(&base);
                HirNodeData::Identifier(text)
            }
            other => panic!("unsupported HIR kind for node {:?}", (other, ts_node)),
        }
    }

    fn extract_text(&self, base: &HirBase<'a>) -> String {
        let start = base.node.start_byte();
        let end = base.node.end_byte();
        if end > start && end <= self.file_content.len() {
            self.file_content[start..end].to_string()
        } else {
            String::new()
        }
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

pub fn build_llmcc_ir_inner<'a, L: LanguageTrait>(
    unit: CompileUnit<'a>,
    file_path: Option<String>,
    file_content: String,
    tree: &'a tree_sitter::Tree,
) -> Result<(), Box<dyn std::error::Error>> {
    let builder = HirBuilder::<L>::new(unit, file_path, file_content);
    let root = tree.root_node();
    builder.build(root);
    Ok(())
}


/// Legacy function for backwards compatibility - sequential IR building for a single unit/// Build IR for all units in the context
pub fn build_llmcc_ir<'a, L: LanguageTrait>(
    cc: &'a CompileCtxt<'a>,
) -> Result<(), Box<dyn std::error::Error>> {
    for index in 0..cc.files.len() {
        let unit = cc.compile_unit(index);
        let file_path = unit.file_path().map(|p| p.to_string());
        let file_content = String::from_utf8_lossy(&unit.file().content()).to_string();
        let tree = unit.tree();

        build_llmcc_ir_inner::<L>(unit, file_path, file_content, tree)?;
    }
    Ok(())
}

/// Legacy function for backwards compatibility - sequential IR building for a single unit
pub fn build_llmcc_ir_single<'a, L: LanguageTrait>(
    unit: CompileUnit<'a>,
) -> Result<(), Box<dyn std::error::Error>> {
    let file_path = unit.file_path().map(|p| p.to_string());
    let file_content = String::from_utf8_lossy(&unit.file().content()).to_string();
    build_llmcc_ir_inner::<L>(unit, file_path, file_content, unit.tree())
}
