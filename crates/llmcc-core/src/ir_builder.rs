use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};

use tree_sitter::Node;

use crate::context::{CompileCtxt, ParentedNode};
use crate::ir::{
    Arena, HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirScope, HirText,
};
use crate::lang_def::LanguageTrait;

/// Global atomic counter for HIR ID allocation
static HIR_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Builder that directly assigns HIR nodes to compile context
struct HirBuilder<'a, Language> {
    arena: &'a Arena<'a>,
    hir_map: HashMap<HirId, ParentedNode<'a>>,
    file_path: Option<String>,
    file_content: String,
    _language: PhantomData<Language>,
}

impl<'a, Language: LanguageTrait> HirBuilder<'a, Language> {
    /// Create a new builder that directly assigns to context
    fn new(
        arena: &'a Arena<'a>,
        file_path: Option<String>,
        file_content: String,
    ) -> Self {
        Self {
            arena,
            hir_map: HashMap::new(),
            file_path,
            file_content,
            _language: PhantomData,
        }
    }

    /// Reserve a new HIR ID
    fn reserve_hir_id(&self) -> HirId {
        let id = HIR_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        HirId(id)
    }

    fn build(mut self, root: Node<'a>) -> (HirId, HashMap<HirId, ParentedNode<'a>>) {
        let file_start_id = self.build_node(root, None);
        (file_start_id, self.hir_map)
    }

    fn build_node(&mut self, node: Node<'a>, parent: Option<HirId>) -> HirId {
        let hir_id = self.reserve_hir_id();
        let child_ids = self.collect_children(node, hir_id);

        let kind = Language::hir_kind(node.kind_id());
        let base = self.make_base(hir_id, parent, node, kind, child_ids);

        let hir_node = match kind {
            HirKind::File => {
                let path = self.file_path.clone().unwrap_or_default();
                let file_node = HirFile::new(base, path);
                HirNode::File(self.arena.alloc(file_node))
            }
            HirKind::Text => {
                let text = self.extract_text(&base);
                let text_node = HirText::new(base, text);
                HirNode::Text(self.arena.alloc(text_node))
            }
            HirKind::Internal => {
                let internal = HirInternal::new(base);
                HirNode::Internal(self.arena.alloc(internal))
            }
            HirKind::Scope => {
                let scope = HirScope::new(base, None);
                HirNode::Scope(self.arena.alloc(scope))
            }
            HirKind::Identifier => {
                let text = self.extract_text(&base);
                let ident = HirIdent::new(base, text);
                HirNode::Ident(self.arena.alloc(ident))
            }
            other => panic!("unsupported HIR kind for node {:?}", (other, node)),
        };

        self.hir_map.insert(hir_id, ParentedNode::new(hir_node));
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
    arena: &'a Arena<'a>,
    file_path: Option<String>,
    file_content: String,
    tree: &'a tree_sitter::Tree,
) -> Result<(HirId, HashMap<HirId, ParentedNode<'a>>), Box<dyn std::error::Error>> {
    let builder = HirBuilder::<L>::new(arena, file_path, file_content);
    let root = tree.root_node();
    let result = builder.build(root);
    Ok(result)
}


/// Build IR for all units in the context
/// TODO: make this run in parallel
pub fn build_llmcc_ir<'a, L: LanguageTrait>(
    cc: &'a CompileCtxt<'a>,
) -> Result<(), Box<dyn std::error::Error>> {
    for index in 0..cc.files.len() {
        let unit = cc.compile_unit(index);
        let file_path = unit.file_path().map(|p| p.to_string());
        let file_content = String::from_utf8_lossy(&unit.file().content()).to_string();
        let tree = unit.tree();

        let (_file_start_id, hir_map) = build_llmcc_ir_inner::<L>(&cc.arena, file_path, file_content, tree)?;

        // Insert all nodes into the compile context
        for (hir_id, parented_node) in hir_map {
            cc.hir_map.borrow_mut().insert(hir_id, parented_node);
        }
        cc.set_file_start(index, _file_start_id);
    }
    Ok(())
}
