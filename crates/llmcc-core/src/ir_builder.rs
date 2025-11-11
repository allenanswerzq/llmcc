use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};

use rayon::prelude::*;
use tree_sitter::Node;

use crate::DynError;
use crate::context::{CompileCtxt, ParentedNode};
use crate::ir::{
    Arena, HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirScope, HirText,
};
use crate::lang_def::LanguageTrait;

/// Global atomic counter for HIR ID allocation
static HIR_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, Copy, Default)]
pub struct IrBuildConfig;

#[derive(Clone)]
struct HirNodeSpec<'hir> {
    base: HirBase<'hir>,
    variant: HirNodeVariantSpec<'hir>,
}

#[derive(Clone)]
enum HirNodeVariantSpec<'hir> {
    File {
        file_path: String,
    },
    Text {
        text: String,
    },
    Internal,
    Scope {
        ident: Option<HirScopeIdentSpec<'hir>>,
    },
    Ident {
        name: String,
    },
}

#[derive(Clone)]
struct HirScopeIdentSpec<'hir> {
    base: HirBase<'hir>,
    name: String,
}

/// Builder that directly assigns HIR nodes to compile context
struct HirBuilder<'a, Language> {
    node_specs: HashMap<HirId, HirNodeSpec<'a>>,
    file_path: Option<String>,
    file_bytes: &'a [u8],
    _language: PhantomData<Language>,
}

impl<'a, Language: LanguageTrait> HirBuilder<'a, Language> {
    /// Create a new builder that directly assigns to context
    fn new(file_path: Option<String>, file_bytes: &'a [u8], _config: IrBuildConfig) -> Self {
        Self {
            node_specs: HashMap::new(),
            file_path,
            file_bytes,
            _language: PhantomData,
        }
    }

    /// Reserve a new HIR ID
    fn reserve_hir_id(&self) -> HirId {
        let id = HIR_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        HirId(id)
    }

    fn build(mut self, root: Node<'a>) -> (HirId, HashMap<HirId, HirNodeSpec<'a>>) {
        let file_start_id = self.build_node(root, None);
        (file_start_id, self.node_specs)
    }

    fn build_node(&mut self, node: Node<'a>, parent: Option<HirId>) -> HirId {
        let hir_id = self.reserve_hir_id();
        let kind_id = node.kind_id();
        let kind = Language::hir_kind(kind_id);
        let child_ids = self.collect_children(node, hir_id);
        let base = self.make_base(hir_id, parent, node, kind, child_ids);

        let variant = match kind {
            HirKind::File => {
                let path = self.file_path.clone().unwrap_or_default();
                HirNodeVariantSpec::File { file_path: path }
            }
            HirKind::Text => {
                let text = self.extract_text(&base);
                HirNodeVariantSpec::Text { text }
            }
            HirKind::Internal => HirNodeVariantSpec::Internal,
            HirKind::Scope => {
                // Try to extract the name identifier from the scope node
                let ident = self.extract_scope_ident(&base, node);
                HirNodeVariantSpec::Scope { ident }
            }
            HirKind::Identifier => {
                let text = self.extract_text(&base);
                HirNodeVariantSpec::Ident { name: text }
            }
            other => panic!("unsupported HIR kind for node {:?}", (other, node)),
        };

        self.node_specs
            .insert(hir_id, HirNodeSpec { base, variant });
        hir_id
    }

    fn collect_children(&mut self, node: Node<'a>, parent_id: HirId) -> Vec<HirId> {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .filter_map(|child| {
                if child.is_error() || child.is_extra() || child.is_missing() || !child.is_named() {
                    return None;
                }

                let child_kind = Language::hir_kind(child.kind_id());
                if child_kind == HirKind::Text {
                    return None;
                }

                Some(self.build_node(child, Some(parent_id)))
            })
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
        if end > start && end <= self.file_bytes.len() {
            match std::str::from_utf8(&self.file_bytes[start..end]) {
                Ok(text) => text.to_owned(),
                Err(_) => String::from_utf8_lossy(&self.file_bytes[start..end]).into_owned(),
            }
        } else {
            String::new()
        }
    }

    fn extract_scope_ident(
        &self,
        base: &HirBase<'a>,
        node: Node<'a>,
    ) -> Option<HirScopeIdentSpec<'a>> {
        // Try to get the name field from the tree-sitter node
        // For Rust, the name field is typically "name"
        let name_node = node.child_by_field_name("name")?;

        // Create an identifier for the name node
        let hir_id = self.reserve_hir_id();
        let ident_base = HirBase {
            hir_id,
            parent: Some(base.hir_id),
            node: name_node,
            kind: HirKind::Identifier,
            field_id: u16::MAX,
            children: Vec::new(),
        };

        let text = self.extract_text(&ident_base);
        Some(HirScopeIdentSpec {
            base: ident_base,
            name: text,
        })
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

impl<'hir> HirNodeSpec<'hir> {
    fn into_parented_node(self, arena: &'hir Arena<'hir>) -> ParentedNode<'hir> {
        let HirNodeSpec { base, variant } = self;

        let hir_node = match variant {
            HirNodeVariantSpec::File { file_path } => {
                let node = HirFile::new(base, file_path);
                HirNode::File(arena.alloc(node))
            }
            HirNodeVariantSpec::Text { text } => {
                let node = HirText::new(base, text);
                HirNode::Text(arena.alloc(node))
            }
            HirNodeVariantSpec::Internal => {
                let node = HirInternal::new(base);
                HirNode::Internal(arena.alloc(node))
            }
            HirNodeVariantSpec::Scope { ident } => {
                let ident_ref = ident.map(|spec| {
                    let HirScopeIdentSpec { base, name } = spec;
                    let ident_node = HirIdent::new(base, name);
                    arena.alloc(ident_node)
                });
                let node = HirScope::new(base, ident_ref);
                HirNode::Scope(arena.alloc(node))
            }
            HirNodeVariantSpec::Ident { name } => {
                let node = HirIdent::new(base, name);
                HirNode::Ident(arena.alloc(node))
            }
        };

        ParentedNode::new(hir_node)
    }
}

fn build_llmcc_ir_inner<'a, L: LanguageTrait>(
    file_path: Option<String>,
    file_bytes: &'a [u8],
    tree: &'a tree_sitter::Tree,
    config: IrBuildConfig,
) -> Result<(HirId, HashMap<HirId, HirNodeSpec<'a>>), DynError> {
    let builder = HirBuilder::<L>::new(file_path, file_bytes, config);
    let root = tree.root_node();
    let result = builder.build(root);
    Ok(result)
}

/// Build IR for all units in the context
struct FileIrBuildResult<'hir> {
    index: usize,
    file_start_id: HirId,
    node_specs: HashMap<HirId, HirNodeSpec<'hir>>,
}

/// Build IR for all units in the context with custom config
pub fn build_llmcc_ir<'a, L: LanguageTrait>(
    cc: &'a CompileCtxt<'a>,
    config: IrBuildConfig,
) -> Result<(), DynError> {
    let results: Vec<Result<FileIrBuildResult<'a>, DynError>> = (0..cc.files.len())
        .into_par_iter()
        .map(|index| {
            let unit = cc.compile_unit(index);
            let file_path = unit.file_path().map(|p| p.to_string());
            let file_bytes = unit.file().content();
            let tree = unit.tree();

            build_llmcc_ir_inner::<L>(file_path, file_bytes, tree, config).map(
                |(file_start_id, node_specs)| FileIrBuildResult {
                    index,
                    file_start_id,
                    node_specs,
                },
            )
        })
        .collect();

    let mut results: Vec<FileIrBuildResult<'a>> =
        results.into_iter().collect::<Result<Vec<_>, _>>()?;

    results.sort_by_key(|result| result.index);

    for result in results {
        let FileIrBuildResult {
            index,
            file_start_id,
            node_specs,
        } = result;

        {
            let mut hir_map = cc.hir_map.write();
            for (hir_id, spec) in node_specs {
                let parented_node = spec.into_parented_node(&cc.arena);
                hir_map.insert(hir_id, parented_node);
            }
        }

        cc.set_file_start(index, file_start_id);
    }

    Ok(())
}
