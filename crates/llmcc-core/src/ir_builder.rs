//! Build language-neutral HIR from parse trees.
//!
//! Each file is built into the shared arena independently, then its root HIR id
//! is registered on the compilation context.
use smallvec::SmallVec;
use std::time::Instant;

use rayon::prelude::*;

use crate::context::{CompileCtxt, CompileUnit};
use crate::id::next_hir_id;
use crate::ir::{
    Arena, HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirScope, HirText,
};
use crate::lang_def::{HirBuildAction, Language, NO_FIELD_ID, ParseChild, ParseNode};
use crate::{Error, ErrorKind, Result};

type HirChildIds = SmallVec<[HirId; 4]>;
type HirChildren<'unit> = SmallVec<[HirNode<'unit>; 8]>;

/// Options for building HIR from parse trees.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HirBuildOptions {
    sequential: bool,
}

impl HirBuildOptions {
    /// Create options that build files in parallel.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create options that build files sequentially.
    pub fn sequential() -> Self {
        Self { sequential: true }
    }

    /// Choose whether files are built sequentially.
    pub fn with_sequential(mut self, sequential: bool) -> Self {
        self.sequential = sequential;
        self
    }

    /// Return true when files should be built one at a time.
    pub fn is_sequential(self) -> bool {
        self.sequential
    }
}

/// Timings collected while building HIR for a compilation context.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HirBuildMetrics {
    /// Wall time spent building files.
    pub build_wall_ms: f64,
    /// Sum of per-file build CPU time across worker threads.
    pub build_cpu_ms: f64,
    /// Time spent publishing built root ids back to the compilation context.
    pub publish_ms: f64,
    /// End-to-end HIR build time.
    pub total_ms: f64,
}

/// Builds the HIR tree for one source file.
struct FileHirBuilder<'unit> {
    arena: &'unit Arena<'unit>,
    file_path: String,
    file_bytes: &'unit [u8],
}

impl<'unit> FileHirBuilder<'unit> {
    fn new(arena: &'unit Arena<'unit>, file_path: String, file_bytes: &'unit [u8]) -> Self {
        Self {
            arena,
            file_path,
            file_bytes,
        }
    }

    fn build<L: Language>(self, root: &dyn ParseNode) -> Result<HirNode<'unit>> {
        self.build_node::<L>(root, None, NO_FIELD_ID)
    }

    fn build_node<L: Language>(
        &self,
        node: &dyn ParseNode,
        parent_id: Option<HirId>,
        field_id: u16,
    ) -> Result<HirNode<'unit>> {
        let id = next_hir_id();
        let kind_id = node.kind_id();
        let kind = L::hir_kind(kind_id);

        let children = if kind.is_leaf() {
            HirChildren::new()
        } else {
            self.build_children::<L>(node, id)?
        };
        let child_ids: HirChildIds = children.iter().map(|node| node.id()).collect();
        let base = self.make_base(id, parent_id, node, kind, child_ids, field_id);

        let hir_node = match kind {
            HirKind::File => {
                let hir_file = HirFile::new(base, self.file_path.clone());
                let allocated = self.arena.alloc(hir_file);
                HirNode::File(allocated)
            }
            HirKind::Text | HirKind::Comment => {
                let text = self.source_text(&base)?;
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
                let ident = children.iter().find_map(|child| match child {
                    HirNode::Ident(ident_node) => Some(*ident_node),
                    _ => None,
                });
                let hir_scope = HirScope::new(base, ident);
                let allocated = self.arena.alloc(hir_scope);
                HirNode::Scope(allocated)
            }
            HirKind::Identifier => {
                let text = self.source_text(&base)?;
                let hir_ident = HirIdent::new(base, text);
                let allocated = self.arena.alloc(hir_ident);
                HirNode::Ident(allocated)
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::InvariantViolation,
                    "language mapped parse node to an unsupported HIR kind",
                )
                .with_operation("build_hir_node")
                .with_context("hir_kind", kind.to_string())
                .with_context("parse_node", node.debug_label()));
            }
        };

        // Allocate the HirNode wrapper with its ID for O(1) lookup
        Ok(*self.arena.alloc_with_id(id.0, hir_node))
    }

    fn build_children<L: Language>(
        &self,
        node: &dyn ParseNode,
        parent_id: HirId,
    ) -> Result<HirChildren<'unit>> {
        let mut children = HirChildren::new();
        let mut skip_next = false;

        for ParseChild {
            node: child,
            field_id,
        } in node.children_with_fields()
        {
            if skip_next {
                skip_next = false;
                continue;
            }

            match L::hir_build_action(child.as_ref(), self.file_bytes) {
                HirBuildAction::Build => {}
                HirBuildAction::Skip => continue,
                HirBuildAction::SkipNextSibling => {
                    skip_next = true;
                    continue;
                }
            }

            let child_node = self.build_node::<L>(child.as_ref(), Some(parent_id), field_id)?;
            children.push(child_node);
        }
        Ok(children)
    }

    fn make_base(
        &self,
        id: HirId,
        parent_id: Option<HirId>,
        node: &dyn ParseNode,
        kind: HirKind,
        children: HirChildIds,
        field_id: u16,
    ) -> HirBase {
        let kind_id = node.kind_id();
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_line = node.start_line();

        HirBase {
            id,
            parent: parent_id,
            kind_id,
            start_byte,
            end_byte,
            start_line,
            kind,
            field_id,
            children,
        }
    }

    fn source_text(&self, base: &HirBase) -> Result<&'unit str> {
        let start = base.start_byte;
        let end = base.end_byte;

        if start > end || end > self.file_bytes.len() {
            return Err(Error::new(
                ErrorKind::InvariantViolation,
                "parse node byte range is outside the source file",
            )
            .with_operation("source_text")
            .with_context("path", self.file_path.clone())
            .with_context("range", format!("{start}..{end}"))
            .with_context("source_len", self.file_bytes.len().to_string()));
        }

        if start == end {
            return Ok("");
        }

        let bytes = &self.file_bytes[start..end];
        match std::str::from_utf8(bytes) {
            Ok(text) => Ok(self.arena.alloc_str(text)),
            Err(_) => {
                let lossy = String::from_utf8_lossy(bytes);
                Ok(self.arena.alloc_str(&lossy))
            }
        }
    }
}

/// Build HIR for a single source file and return its root node id.
pub fn build_file_hir<'unit, L: Language>(unit: CompileUnit<'unit>) -> Result<HirId> {
    let root = unit.parse_tree()?.root();
    let path = unit.file_path().unwrap_or("<memory>").to_string();
    let bytes = unit.file().content();

    let builder = FileHirBuilder::new(&unit.cc.arena, path, bytes);
    let root = builder.build::<L>(root.as_ref())?;
    Ok(root.id())
}

#[derive(Debug, Clone, Copy)]
struct BuiltFile {
    /// Index of this file in the compile context
    index: usize,
    /// HirId of the file's root node
    file_root_id: HirId,
    build_ns: u64,
}

/// Build HIR for all files in the compile context.
pub fn build_hir<'tcx, L: Language>(
    cc: &'tcx CompileCtxt<'tcx>,
    options: HirBuildOptions,
) -> Result<HirBuildMetrics> {
    let total_start = Instant::now();

    let parallel_start = Instant::now();
    let results: Vec<Result<BuiltFile>> = if options.is_sequential() {
        (0..cc.files.len())
            .map(|index| build_unit_hir::<L>(cc, index))
            .collect()
    } else {
        (0..cc.files.len())
            .into_par_iter()
            .map(|index| build_unit_hir::<L>(cc, index))
            .collect()
    };
    let parallel_time = parallel_start.elapsed();

    let publish_start = Instant::now();
    // Collect results before publishing roots to the shared context.
    let results: Vec<BuiltFile> = results.into_iter().collect::<Result<Vec<_>>>()?;
    let build_cpu_ns = results.iter().map(|unit| unit.build_ns).sum::<u64>();

    for BuiltFile {
        index,
        file_root_id,
        build_ns: _,
    } in results
    {
        cc.set_file_root_id(index, file_root_id);
    }

    let publish_time = publish_start.elapsed();

    let metrics = HirBuildMetrics {
        build_wall_ms: parallel_time.as_secs_f64() * 1000.0,
        build_cpu_ms: build_cpu_ns as f64 / 1_000_000.0,
        publish_ms: publish_time.as_secs_f64() * 1000.0,
        total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
    };

    tracing::info!(
        build_wall_ms = metrics.build_wall_ms,
        build_cpu_ms = metrics.build_cpu_ms,
        publish_ms = metrics.publish_ms,
        total_ms = metrics.total_ms,
        "HIR build complete"
    );

    Ok(metrics)
}

fn build_unit_hir<'tcx, L: Language>(
    cc: &'tcx CompileCtxt<'tcx>,
    index: usize,
) -> Result<BuiltFile> {
    let build_start = Instant::now();

    let file_root_id = build_file_hir::<L>(cc.compile_unit(index))?;

    let build_ns = build_start.elapsed().as_nanos() as u64;

    Ok(BuiltFile {
        index,
        file_root_id,
        build_ns,
    })
}
