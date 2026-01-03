//! IR Builder: Transform parse trees into High-level Intermediate Representation (HIR).
//!
//! Uses per-unit arenas for parallel building, then merges results into global context.
//! This avoids locks during parallel builds and ensures deterministic ID allocation.
use smallvec::SmallVec;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use rayon::prelude::*;

use crate::DynError;
use crate::context::CompileCtxt;
use crate::ir::{
    Arena, HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirScope, HirText,
};
use crate::lang_def::{LanguageTrait, ParseNode, ParseTree};

/// Global atomic counter for HIR ID allocation (used during parallel builds).
static HIR_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

// Timing counters for IR building
static IR_BUILD_CPU_TIME_NS: AtomicU64 = AtomicU64::new(0);

pub fn reset_ir_build_counters() {
    IR_BUILD_CPU_TIME_NS.store(0, Ordering::Relaxed);
}

pub fn get_ir_build_cpu_time_ms() -> f64 {
    IR_BUILD_CPU_TIME_NS.load(Ordering::Relaxed) as f64 / 1_000_000.0
}

/// Reserve a new globally-unique HIR ID.
#[inline]
pub fn next_hir_id() -> HirId {
    let id = HIR_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    HirId(id)
}

/// Reset the global HIR ID counter to 0 (for testing isolation)
pub fn reset_hir_id_counter() {
    HIR_ID_COUNTER.store(0, Ordering::Relaxed);
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
        let child_ids: SmallVec<[HirId; 4]> = children.iter().map(|n| n.id()).collect();
        let base = self.make_base(id, parent, node, kind, child_ids);

        let hir_node = match kind {
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
                let ident = children
                    .iter()
                    .map(|child| {
                        if let HirNode::Ident(ident_node) = child {
                            *ident_node
                        } else {
                            let text = self.get_text(&base);
                            tracing::trace!("scope crate non-identifier ident '{}'", text);
                            let hir_ident = HirIdent::new(base.clone(), text);
                            self.arena.alloc(hir_ident)
                        }
                    })
                    .next();
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
        };

        // Allocate the HirNode wrapper with its ID for O(1) lookup
        *self.arena.alloc_with_id(id.0, hir_node)
    }

    /// Collect all valid child nodes from a parse node.
    /// Filters out test code (items with #[test] or #[cfg(test)] attributes).
    fn collect_children(&self, node: &dyn ParseNode, parent_id: HirId) -> SmallVec<[HirNode<'unit>; 8]> {
        let mut child_nodes = SmallVec::new();
        let mut skip_next = false;

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                // Check if this is a test attribute that should cause the next item to be skipped
                if Language::is_test_attribute(child.as_ref(), self.file_bytes) {
                    skip_next = true;
                    // Still add the attribute node itself (it will be orphaned but harmless)
                    // Actually, skip the attribute too for cleaner HIR
                    continue;
                }

                // Skip items that follow test attributes
                if skip_next {
                    skip_next = false;
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
        children: SmallVec<[HirId; 4]>,
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
    /// Allocates the string in the arena to avoid heap allocation.
    fn get_text(&self, base: &HirBase) -> &'unit str {
        let start = base.start_byte;
        let end = base.end_byte;
        if end > start && end <= self.file_bytes.len() {
            match std::str::from_utf8(&self.file_bytes[start..end]) {
                Ok(text) => self.arena.alloc_str(text),
                Err(_) => {
                    let lossy = String::from_utf8_lossy(&self.file_bytes[start..end]);
                    self.arena.alloc_str(&lossy)
                }
            }
        } else {
            ""
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
    let total_start = Instant::now();
    reset_ir_build_counters();

    let build_one = |index: usize| -> Result<BuildResult, DynError> {
        let build_start = Instant::now();

        let file_path = cc.file_path(index).map(|p| p.to_string());
        let file_bytes = cc.files[index].content();

        let parse_tree = cc
            .get_parse_tree(index)
            .ok_or_else(|| format!("No parse tree for unit {}", index))?;

        let file_root_id =
            build_llmcc_ir_inner::<L>(file_path, file_bytes, parse_tree, &cc.arena, config)?;

        IR_BUILD_CPU_TIME_NS.fetch_add(build_start.elapsed().as_nanos() as u64, Ordering::Relaxed);

        Ok(BuildResult {
            index,
            file_root_id,
        })
    };

    let parallel_start = Instant::now();
    let results: Vec<Result<BuildResult, DynError>> = if config.sequential {
        (0..cc.files.len()).map(build_one).collect()
    } else {
        (0..cc.files.len()).into_par_iter().map(build_one).collect()
    };
    let parallel_time = parallel_start.elapsed();

    let collect_start = Instant::now();
    // Collect results (no sorting needed - DashMap provides O(1) lookup by ID)
    let results: Vec<BuildResult> = results.into_iter().collect::<Result<Vec<_>, _>>()?;

    // Register all file start IDs
    for BuildResult {
        index,
        file_root_id,
    } in results
    {
        cc.set_file_root_id(index, file_root_id);
    }

    // No sort needed: DashMap already provides O(1) lookup by ID
    let collect_time = collect_start.elapsed();

    let total_time = total_start.elapsed();
    let build_cpu_ms = get_ir_build_cpu_time_ms();

    tracing::info!(
        "ir_build breakdown: parallel={:.2}ms (build_cpu={:.2}ms), collect={:.2}ms, total={:.2}ms",
        parallel_time.as_secs_f64() * 1000.0,
        build_cpu_ms,
        collect_time.as_secs_f64() * 1000.0,
        total_time.as_secs_f64() * 1000.0,
    );

    Ok(())
}
