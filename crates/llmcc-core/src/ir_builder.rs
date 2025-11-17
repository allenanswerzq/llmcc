//! IR Builder: Transform parse trees into High-level Intermediate Representation (HIR).
//!
//! Uses per-unit arenas for parallel building, then merges results into global context.
//! This avoids locks during parallel builds and ensures deterministic ID allocation.
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::DynError;
use crate::context::CompileCtxt;
use crate::ir::{
    Arena, HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirScope, HirText,
};
use crate::lang_def::{LanguageTrait, ParseNode, ParseTree};

/// Global atomic counter for HIR ID allocation (used during parallel builds).
static HIR_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Configuration for IR building behavior.
///
/// This is currently a zero-sized marker type but is provided for future extensibility.
/// Future versions may include options like:
/// - Error recovery strategies
/// - Symbol table generation
/// - Scope depth limits
/// - Performance tuning parameters
#[derive(Debug, Clone, Copy, Default)]
pub struct IrBuildOption;

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
    ///
    /// # Arguments
    ///
    /// - `arena`: Per-unit arena for allocating all HIR nodes
    /// - `file_path`: Optional path to the file being parsed (stored in File node)
    /// - `file_bytes`: Source file content for text extraction
    /// - `_config`: Build configuration (reserved for future use)
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

    /// Reserve a new globally-unique HIR ID.
    ///
    /// Uses atomic increment with [`Ordering::SeqCst`] to ensure globally
    /// consistent ID assignment across all threads.
    fn reserve_hir_id(&self) -> HirId {
        let id = HIR_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        HirId(id)
    }

    /// Build HIR nodes from a parse tree root.
    ///
    /// Recursively walks the tree, assigning HIR IDs and allocating nodes directly
    /// into the per-unit arena. Returns the root HirId.
    ///
    /// # Returns
    ///
    /// Root HirId for this file
    fn build(self, root: &dyn ParseNode) -> HirId {
        self.build_node(root, None)
    }

    /// Recursively build a single HIR node and all descendants, allocating directly into arena.
    ///
    /// # Arguments
    ///
    /// - `node`: The parse tree node to convert
    /// - `parent`: Optional parent HirId (None for root)
    ///
    /// # Returns
    ///
    /// The HirNode allocated in the arena
    fn build_node(&self, node: &dyn ParseNode, parent: Option<HirId>) -> HirId {
        let id = self.reserve_hir_id();
        let kind_id = node.kind_id();
        let kind = Language::hir_kind(kind_id);
        let child_ids = self.collect_children(node, id);
        let base = self.make_base(id, parent, node, kind, child_ids);

        match kind {
            HirKind::File => {
                let path = self.file_path.clone().unwrap_or_default();
                let hir_file = HirFile::new(base, path);
                let _allocated = self.arena.alloc(hir_file);
            }
            HirKind::Text => {
                let text = self.extract_text(&base);
                let hir_text = HirText::new(base, text);
                let _allocated = self.arena.alloc(hir_text);
            }
            HirKind::Internal => {
                let hir_internal = HirInternal::new(base);
                let _allocated = self.arena.alloc(hir_internal);
            }
            HirKind::Scope => {
                let ident = self.extract_scope_ident(&base, node);
                let hir_scope = HirScope::new(base, ident);
                let _allocated = self.arena.alloc(hir_scope);
            }
            HirKind::Identifier => {
                let text = self.extract_text(&base);
                let hir_ident = HirIdent::new(base, text);
                let _allocated = self.arena.alloc(hir_ident);
            }
            _other => panic!("unsupported HIR kind for node {}", node.debug_info()),
        };

        id
    }

    /// Collect all valid child nodes from a parse node.
    ///
    /// Filters out error nodes, extra nodes (whitespace/comments), missing nodes,
    /// and unnamed nodes. Text nodes are skipped because they're handled separately
    /// via text extraction.
    fn collect_children(&self, node: &dyn ParseNode, parent_id: HirId) -> Vec<HirId> {
        let mut child_ids = Vec::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.is_error() || child.is_extra() || child.is_missing() || !child.is_named() {
                    continue;
                }

                let child_kind = Language::hir_kind(child.kind_id());
                if child_kind == HirKind::Text {
                    continue;
                }

                let child_id = self.build_node(child.as_ref(), Some(parent_id));
                child_ids.push(child_id);
            }
        }
        child_ids
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
    fn extract_text(&self, base: &HirBase) -> String {
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

    /// Extract the identifier from a scope node if present.
    ///
    /// Attempts to find the "name" child field and allocate an identifier in the arena.
    fn extract_scope_ident(
        &self,
        base: &HirBase,
        node: &dyn ParseNode,
    ) -> Option<&'unit HirIdent<'unit>> {
        let name_node = node.child_by_field_name("name")?;

        let id = self.reserve_hir_id();
        let kind_id = name_node.kind_id();
        let start_byte = name_node.start_byte();
        let end_byte = name_node.end_byte();
        let ident_base = HirBase {
            id,
            parent: Some(base.id),
            kind_id,
            start_byte,
            end_byte,
            kind: HirKind::Identifier,
            field_id: u16::MAX,
            children: Vec::new(),
        };

        let text = self.extract_text(&ident_base);
        let ident = HirIdent::new(ident_base, text);
        Some(self.arena.alloc(ident))
    }
}

/// Build IR for a single file with language-specific handling.
///
/// This internal function performs the actual IR building for one compilation unit
/// using the global shared arena. It is called from [`build_llmcc_ir`] in parallel
/// across multiple files. All HirNodes are allocated directly into the shared arena
/// (which is thread-safe via typed_arena's internal synchronization).
///
/// # Type Parameters
///
/// - `L`: Language implementation providing token ID to HIR kind mapping
///
/// # Arguments
///
/// - `file_path`: Optional path to the file
/// - `file_bytes`: Source file content
/// - `parse_tree`: Generic parse tree from the parser
/// - `unit_arena`: Shared arena for allocating HIR nodes (thread-safe)
/// - `config`: Build configuration
///
/// # Returns
///
/// Root HirId for this file
///
/// # Errors
///
/// Returns an error if the parse tree does not provide a root node.
///
/// # Thread Safety
///
/// This function is thread-safe for parallel execution. All arena allocations
/// are coordinated via typed_arena's internal synchronization, and HirIds are
/// allocated via atomic counter to ensure uniqueness.
fn build_llmcc_ir_inner<'arena, L: LanguageTrait>(
    file_path: Option<String>,
    file_bytes: &'arena [u8],
    parse_tree: &'arena dyn ParseTree,
    unit_arena: &'arena Arena<'arena>,
    config: IrBuildOption,
) -> Result<HirId, DynError> {
    let root = parse_tree
        .root_node()
        .ok_or_else(|| "ParseTree does not provide a root node".to_string())?;

    let builder = HirBuilder::<L>::new(unit_arena, file_path, file_bytes, config);
    let root_id = builder.build(root.as_ref());
    Ok(root_id)
}

/// Result container for parallel file IR building.
///
/// Holds only the file index and root HirId; all HIR nodes were allocated
/// into the shared global arena during parallel builds.
struct BuildResult {
    /// Index of this file in the compile context
    index: usize,
    /// HirId of the file's root node
    file_start_id: HirId,
}

/// Build IR for all files in the compile context.
///
/// Performs parallel multi-file IR building using Rayon for efficient compilation.
/// All files share the global arena (which is thread-safe via typed_arena), and
/// HirIds are allocated via atomic counter to ensure uniqueness.
pub fn build_llmcc_ir<'tcx, L: LanguageTrait>(
    cc: &'tcx CompileCtxt<'tcx>,
    config: IrBuildOption,
) -> Result<(), DynError> {
    // Collect all build results in parallel
    let results: Vec<Result<BuildResult, DynError>> = (0..cc.files.len())
        .into_par_iter()
        .map(|index| {
            let file_path = cc.file_path(index).map(|p| p.to_string());
            let file_bytes = cc.files[index].content();

            let parse_tree = cc
                .get_parse_tree(index)
                .ok_or_else(|| format!("No parse tree for unit {}", index))?;

            let file_start_id =
                build_llmcc_ir_inner::<L>(file_path, file_bytes, parse_tree, &cc.arena, config)?;

            Ok(BuildResult {
                index,
                file_start_id,
            })
        })
        .collect();

    // Collect and sort results
    let mut file_results: Vec<BuildResult> = results.into_iter().collect::<Result<Vec<_>, _>>()?;
    file_results.sort_by_key(|result| result.index);

    // Register all file start IDs
    for BuildResult {
        index,
        file_start_id,
    } in file_results
    {
        cc.set_file_start(index, file_start_id);
    }

    // Sequential phase: Build hir_map from all allocated nodes
    // We need to walk through each HirNode variant and extract parent relationships
    build_hir_map(cc)?;

    Ok(())
}

/// Rebuild the hir_map from all allocated HirNodes in the arena.
///
/// This function scans all allocated HIR nodes and constructs parent-child
/// relationships, populating the global hir_map used for lookups.
///
/// This is called after all parallel builds complete to ensure all parent
fn build_hir_map<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> Result<(), DynError> {
    let mut hir_map = cc.hir_map.write();

    // Iterate over all vec-backed HIR types and insert them into hir_map.
    // Since all HIR node types are now vec-backed, we can iterate them directly.

    for hir_root in cc.arena.hir_root() {
        let node = HirNode::Root(hir_root);
        let parented = crate::context::ParentedNode::new(node);
        hir_map.insert(hir_root.base.id, parented);
    }

    for hir_text in cc.arena.hir_text() {
        let node = HirNode::Text(hir_text);
        let parented = crate::context::ParentedNode::new(node);
        hir_map.insert(hir_text.base.id, parented);
    }

    for hir_internal in cc.arena.hir_internal() {
        let node = HirNode::Internal(hir_internal);
        let parented = crate::context::ParentedNode::new(node);
        hir_map.insert(hir_internal.base.id, parented);
    }

    for hir_scope in cc.arena.hir_scope() {
        let node = HirNode::Scope(hir_scope);
        let parented = crate::context::ParentedNode::new(node);
        hir_map.insert(hir_scope.base.id, parented);
    }

    for hir_file in cc.arena.hir_file() {
        let node = HirNode::File(hir_file);
        let parented = crate::context::ParentedNode::new(node);
        hir_map.insert(hir_file.base.id, parented);
    }

    for hir_ident in cc.arena.hir_ident() {
        let node = HirNode::Ident(hir_ident);
        let parented = crate::context::ParentedNode::new(node);
        hir_map.insert(hir_ident.base.id, parented);
    }

    Ok(())
}
