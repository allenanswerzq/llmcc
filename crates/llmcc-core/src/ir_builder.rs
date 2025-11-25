//! IR Builder: Transform parse trees into High-level Intermediate Representation (HIR).
//!
//! Uses per-unit arenas for parallel building, then merges results into global context.
//! This avoids locks during parallel builds and ensures deterministic ID allocation.
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::DynError;
use crate::context::CompileCtxt;
use crate::context::ParentedNode;
use crate::ir::{
    Arena, HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirScope, HirText,
};
use crate::lang_def::{LanguageTrait, ParseNode, ParseTree};

/// Global atomic counter for HIR ID allocation (used during parallel builds).
static HIR_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Reserve a new globally-unique HIR ID.
pub fn next_hir_id() -> HirId {
    let id = HIR_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    HirId(id)
}

/// Configuration for IR building behavior.
///
/// This configuration controls how the IR builder processes files.
/// By default, files are processed in parallel for better performance.
#[derive(Debug, Clone, Copy, Default)]
pub struct IrBuildOption {
    /// When true, process files sequentially to ensure deterministic ordering.
    /// When false (default), process files in parallel for better performance.
    pub sequential: bool,
}

impl IrBuildOption {
    /// Create a new IrBuildOption with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set whether to process files sequentially.
    pub fn with_sequential(mut self, sequential: bool) -> Self {
        self.sequential = sequential;
        self
    }
}

/// IR builder that transforms parse trees into HIR nodes using a per-unit arena.
struct HirBuilder<'unit, Language> {
    /// Per-unit arena for allocating all HIR nodes during this build
    arena: &'unit Arena<'unit>,
    /// Optional file path for the File node
    file_path: Option<String>,
    /// Source file content bytes for text extraction
    file_bytes: &'unit [u8],
    /// Language-specific handler (used via PhantomData for compile-time only)
    _language: PhantomData<Language>,
}

impl<'unit, Language: LanguageTrait> HirBuilder<'unit, Language> {
    /// Create a new HIR builder for a single file using a per-unit arena.
    fn new(
        arena: &'unit Arena<'unit>,
        file_path: Option<String>,
        file_bytes: &'unit [u8],
        _config: IrBuildOption,
    ) -> Self {
        Self {
            arena,
            file_path,
            file_bytes,
            _language: PhantomData,
        }
    }

    /// Build HIR nodes from a parse tree root.
    fn build(self, root: &dyn ParseNode) -> HirNode<'unit> {
        self.build_node(root, None)
    }

    /// Recursively build a single HIR node and all descendants, allocating directly into arena.
    fn build_node(&self, node: &dyn ParseNode, parent: Option<HirId>) -> HirNode<'unit> {
        let id = next_hir_id();
        let kind_id = node.kind_id();
        let kind = Language::hir_kind(kind_id);
        let children = self.collect_children(node, id);
        let child_ids: Vec<HirId> = children.iter().map(|n| n.id()).collect();
        let base = self.make_base(id, parent, node, kind, child_ids);

        match kind {
            HirKind::File => {
                let path = self.file_path.clone().unwrap_or_default();
                let hir_file = HirFile::new(base, path);
                let allocated = self.arena.alloc(hir_file);
                HirNode::File(allocated)
            }
            HirKind::Text => {
                let text = self.get_text(&base);
                let hir_text = HirText::new(base, text);
                let allocated = self.arena.alloc(hir_text);
                HirNode::Text(allocated)
            }
            HirKind::Internal => {
                let hir_internal = HirInternal::new(base);
                let allocated = self.arena.alloc(hir_internal);
                HirNode::Internal(allocated)
            }
            HirKind::Scope => {
                // Find the first identifier child
                let ident = children.iter().find_map(|child| {
                    if let HirNode::Ident(ident_node) = child {
                        Some(*ident_node)
                    } else {
                        None
                    }
                });
                let hir_scope = HirScope::new(base, ident);
                let allocated = self.arena.alloc(hir_scope);
                HirNode::Scope(allocated)
            }
            HirKind::Identifier => {
                let text = self.get_text(&base);
                let hir_ident = HirIdent::new(base, text);
                let allocated = self.arena.alloc(hir_ident);
                HirNode::Ident(allocated)
            }
            _other => panic!("unsupported HIR kind for node {}", node.debug_info()),
        }
    }

    /// Collect all valid child nodes from a parse node.
    fn collect_children(&self, node: &dyn ParseNode, parent_id: HirId) -> Vec<HirNode<'unit>> {
        let mut child_nodes = Vec::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.is_error() || child.is_extra() || child.is_missing() || !child.is_named() {
                    continue;
                }

                let child_kind = Language::hir_kind(child.kind_id());
                if child_kind == HirKind::Text {
                    continue;
                }

                let child_node = self.build_node(child.as_ref(), Some(parent_id));
                child_nodes.push(child_node);
            }
        }
        child_nodes
    }

    /// Construct the base metadata for a HIR node.
    fn make_base(
        &self,
        id: HirId,
        parent: Option<HirId>,
        node: &dyn ParseNode,
        kind: HirKind,
        children: Vec<HirId>,
    ) -> HirBase {
        let kind_id = node.kind_id();
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let field_id = node.field_id().unwrap_or(u16::MAX);
        HirBase {
            id,
            parent,
            kind_id,
            start_byte,
            end_byte,
            kind,
            field_id,
            children,
        }
    }

    /// Extract text content from source for a text-type node.
    ///
    /// Handles both valid UTF-8 and lossy conversions gracefully.
    fn get_text(&self, base: &HirBase) -> String {
        let start = base.start_byte;
        let end = base.end_byte;
        if end > start && end <= self.file_bytes.len() {
            match std::str::from_utf8(&self.file_bytes[start..end]) {
                Ok(text) => text.to_owned(),
                Err(_) => String::from_utf8_lossy(&self.file_bytes[start..end]).into_owned(),
            }
        } else {
            String::new()
        }
    }
}

/// Build IR for a single file with language-specific handling.
fn build_llmcc_ir_inner<'unit, L: LanguageTrait>(
    file_path: Option<String>,
    file_bytes: &'unit [u8],
    parse_tree: &'unit dyn ParseTree,
    unit_arena: &'unit Arena<'unit>,
    config: IrBuildOption,
) -> Result<HirId, DynError> {
    let root = parse_tree
        .root_node()
        .ok_or_else(|| "ParseTree does not provide a root node".to_string())?;

    let builder = HirBuilder::<L>::new(unit_arena, file_path, file_bytes, config);
    let root = builder.build(root.as_ref());
    Ok(root.id())
}

struct BuildResult {
    /// Index of this file in the compile context
    index: usize,
    /// HirId of the file's root node
    file_root_id: HirId,
}

/// Build IR for all files in the compile context.
pub fn build_llmcc_ir<'tcx, L: LanguageTrait>(
    cc: &'tcx CompileCtxt<'tcx>,
    config: IrBuildOption,
) -> Result<(), DynError> {
    let build_one = |index: usize| -> Result<BuildResult, DynError> {
        let file_path = cc.file_path(index).map(|p| p.to_string());
        let file_bytes = cc.files[index].content();

        let parse_tree = cc
            .get_parse_tree(index)
            .ok_or_else(|| format!("No parse tree for unit {}", index))?;

        let file_root_id =
            build_llmcc_ir_inner::<L>(file_path, file_bytes, parse_tree, &cc.arena, config)?;

        Ok(BuildResult {
            index,
            file_root_id,
        })
    };

    let results: Vec<Result<BuildResult, DynError>> = if config.sequential {
        (0..cc.files.len()).map(build_one).collect()
    } else {
        (0..cc.files.len()).into_par_iter().map(build_one).collect()
    };

    // Collect and sort results
    let mut results: Vec<BuildResult> = results.into_iter().collect::<Result<Vec<_>, _>>()?;
    results.sort_by_key(|result| result.index);

    // Register all file start IDs
    for BuildResult {
        index,
        file_root_id,
    } in results
    {
        cc.set_file_root_id(index, file_root_id);
    }

    // Sequential phase: Build hir_map from all allocated nodes
    build_hir_map(cc)?;

    Ok(())
}

/// Rebuild the hir_map from all allocated HirNodes in the arena.
fn build_hir_map<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> Result<(), DynError> {
    let mut hir_map = cc.hir_map.write();

    for hir_root in cc.arena.hir_root() {
        let node = HirNode::Root(hir_root);
        let parented = ParentedNode::new(node);
        hir_map.insert(hir_root.base.id, parented);
    }

    for hir_text in cc.arena.hir_text() {
        let node = HirNode::Text(hir_text);
        let parented = ParentedNode::new(node);
        hir_map.insert(hir_text.base.id, parented);
    }

    for hir_internal in cc.arena.hir_internal() {
        let node = HirNode::Internal(hir_internal);
        let parented = ParentedNode::new(node);
        hir_map.insert(hir_internal.base.id, parented);
    }

    for hir_scope in cc.arena.hir_scope() {
        let node = HirNode::Scope(hir_scope);
        let parented = ParentedNode::new(node);
        hir_map.insert(hir_scope.base.id, parented);
    }

    for hir_file in cc.arena.hir_file() {
        let node = HirNode::File(hir_file);
        let parented = ParentedNode::new(node);
        hir_map.insert(hir_file.base.id, parented);
    }

    for hir_ident in cc.arena.hir_ident() {
        let node = HirNode::Ident(hir_ident);
        let parented = ParentedNode::new(node);
        hir_map.insert(hir_ident.base.id, parented);
    }

    Ok(())
}
