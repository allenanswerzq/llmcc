//! IR Builder: Transform parse trees into High-level Intermediate Representation (HIR).
//!
//! This module handles the conversion from parser-agnostic ParseTree representations
//! into a unified HIR (High-level Intermediate Representation) that can be used for
//! program analysis, transformation, and code generation.
//!
//! # Architecture
//!
//! The IR building process is organized into three layers:
//!
//! 1. **Parse Tree Traversal** ([`HirBuilder`]): Recursively walks the parse tree,
//!    assigning HIR IDs and collecting node metadata.
//! 2. **Node Specification** ([`HirNodeSpec`]): Language-agnostic intermediate form
//!    that captures node structure without arena allocation.
//! 3. **Arena Allocation** ([`HirNodeSpec::into_parented_node`]): Final allocation
//!    in the compile context's arena for memory efficiency.
//!
//! # Parallelization
//!
//! File-level IR building is parallelized via Rayon to efficiently handle
//! multi-file compilation units. Per-file building is inherently sequential
//! (tree traversal + arena allocation).
//!
//! # Design Decisions
//!
//! - **Parser Abstraction**: Uses [`ParseNode`] and [`ParseTree`] traits to support
//!   multiple parser backends (tree-sitter, custom parsers, etc.)
//! - **Global ID Counter**: [`HIR_ID_COUNTER`] ensures unique IDs across all files
//!   and threads (atomic with SeqCst ordering for thread safety).
//! - **Deferred Allocation**: Nodes are collected as specs first, then allocated
//!   into the arena only after all files complete processing (ensures consistent
//!   allocation order despite parallelization).
//! - **Text Extraction**: Lazy extraction of node text from source bytes to minimize
//!   memory overhead for internal nodes.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};

use rayon::prelude::*;

use crate::DynError;
use crate::context::{CompileCtxt, ParentedNode};
use crate::ir::{
    Arena, HirBase, HirFile, HirId, HirIdent, HirInternal, HirKind, HirNode, HirScope, HirText,
};
use crate::lang_def::{LanguageTrait, ParseNode, ParseTree};

/// Global atomic counter for HIR ID allocation.
///
/// This counter is shared across all files and threads, ensuring unique
/// HirId values throughout the entire compilation unit set.
/// Uses [`Ordering::SeqCst`] for strict thread-safety guarantees.
static HIR_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Configuration for IR building behavior.
///
/// This is currently a zero-sized marker type but is provided for future extensibility.
/// Future versions may include options like:
/// - Error recovery strategies
/// - Symbol table generation
/// - Scope depth limits
/// - Performance tuning parameters
#[derive(Debug, Clone, Copy, Default)]
pub struct IrBuildConfig;

/// Specification for a single HIR node before arena allocation.
///
/// This intermediate form separates node discovery from arena allocation,
/// allowing all nodes to be collected and deduplicated before final allocation.
#[derive(Clone)]
struct HirNodeSpec {
    /// Base metadata common to all HIR nodes
    base: HirBase,
    /// Variant-specific data for different node types
    variant: HirNodeVariantSpec,
}

/// Variant-specific data for different HIR node types.
///
/// Represents the different HIR node kinds and their associated data.
#[derive(Clone)]
enum HirNodeVariantSpec {
    /// File node: root of each compilation unit
    File { file_path: String },
    /// Text/leaf node: represents identifiers, keywords, literals
    Text { text: String },
    /// Internal node: structural node without specific semantics
    Internal,
    /// Scope node: represents a binding scope (function, class, module, etc.)
    Scope { ident: Option<HirScopeIdentSpec> },
    /// Identifier node: represents a reference to an identifier
    Ident { name: String },
}

/// Specification for an identifier within a scope node.
///
/// Deferred allocation of scope identifiers until arena is available.
#[derive(Clone)]
struct HirScopeIdentSpec {
    /// Base metadata for the identifier
    base: HirBase,
    /// The actual identifier text
    name: String,
}

/// IR builder that transforms parse trees into HIR node specifications.
///
/// Performs a recursive depth-first traversal of the parse tree, assigning HIR IDs,
/// extracting text and metadata, and building the initial node specifications.
/// The result is then allocated into the arena.
///
/// # Generics
///
/// - `Language`: Implements [`LanguageTrait`] to provide language-specific mappings
///   (token IDs to HIR kinds, field ID resolution, etc.)
struct HirBuilder<'a, Language> {
    /// Accumulated node specifications, keyed by HirId
    node_specs: HashMap<HirId, HirNodeSpec>,
    /// Optional file path for the File node
    file_path: Option<String>,
    /// Source file content bytes for text extraction
    file_bytes: &'a [u8],
    /// Language-specific handler (used via PhantomData for compile-time only)
    _language: PhantomData<Language>,
}

impl<'a, Language: LanguageTrait> HirBuilder<'a, Language> {
    /// Create a new HIR builder for a single file.
    ///
    /// # Arguments
    ///
    /// - `file_path`: Optional path to the file being parsed (stored in File node)
    /// - `file_bytes`: Source file content for text extraction
    /// - `_config`: Build configuration (reserved for future use)
    fn new(file_path: Option<String>, file_bytes: &'a [u8], _config: IrBuildConfig) -> Self {
        Self {
            node_specs: HashMap::new(),
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
    /// Recursively walks the tree, assigning HIR IDs and collecting node specifications.
    ///
    /// # Returns
    ///
    /// Tuple of (root_hir_id, all_node_specs) for all discovered nodes.
    fn build(mut self, root: &dyn ParseNode) -> (HirId, HashMap<HirId, HirNodeSpec>) {
        let file_start_id = self.build_node(root, None);
        (file_start_id, self.node_specs)
    }

    /// Recursively build a single HIR node and all descendants.
    ///
    /// # Arguments
    ///
    /// - `node`: The parse tree node to convert
    /// - `parent`: Optional parent HirId (None for root)
    ///
    /// # Returns
    ///
    /// The HirId assigned to this node
    fn build_node(&mut self, node: &dyn ParseNode, parent: Option<HirId>) -> HirId {
        let id = self.reserve_hir_id();
        let kind_id = node.kind_id();
        let kind = Language::hir_kind(kind_id);
        let child_ids = self.collect_children(node, id);
        let base = self.make_base(id, parent, node, kind, child_ids);

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
                let ident = self.extract_scope_ident(&base, node);
                HirNodeVariantSpec::Scope { ident }
            }
            HirKind::Identifier => {
                let text = self.extract_text(&base);
                HirNodeVariantSpec::Ident { name: text }
            }
            _other => panic!("unsupported HIR kind for node {}", node.debug_info()),
        };

        self.node_specs
            .insert(id, HirNodeSpec { base, variant });
        id
    }

    /// Collect all valid child nodes from a parse node.
    ///
    /// Filters out error nodes, extra nodes (whitespace/comments), missing nodes,
    /// and unnamed nodes. Text nodes are skipped because they're handled separately
    /// via text extraction.
    fn collect_children(&mut self, node: &dyn ParseNode, parent_id: HirId) -> Vec<HirId> {
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
        let field_id = Self::field_id_of().unwrap_or(u16::MAX);
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
    /// Attempts to find the "name" child field and create an identifier specification.
    fn extract_scope_ident(
        &self,
        base: &HirBase,
        node: &dyn ParseNode,
    ) -> Option<HirScopeIdentSpec> {
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
        Some(HirScopeIdentSpec {
            base: ident_base,
            name: text,
        })
    }

    /// Get the field ID for a node.
    ///
    /// Currently returns `None` because the ParseNode trait doesn't provide
    /// direct field ID access. This could be extended in the future if needed
    /// by adding a method to the ParseNode trait.
    #[allow(clippy::unused_self)]
    fn field_id_of() -> Option<u16> {
        None
    }
}

impl HirNodeSpec {
    /// Convert a node specification into an allocated HIR node in the arena.
    ///
    /// Allocates the appropriate HIR node type based on the variant, storing
    /// the result in the provided arena and wrapping it in a ParentedNode.
    ///
    /// # Arguments
    ///
    /// - `arena`: The arena to allocate nodes into (from the compile context)
    fn into_parented_node<'hir>(self, arena: &'hir Arena<'hir>) -> ParentedNode<'hir> {
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

/// Build IR for a single file with language-specific handling.
///
/// This internal function performs the actual IR building for one compilation unit.
/// It is called from [`build_llmcc_ir`] in parallel across multiple files.
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
/// - `config`: Build configuration
///
/// # Returns
///
/// Tuple of (root_hir_id, all_node_specs) for this file
///
/// # Errors
///
/// Returns an error if the parse tree does not provide a root node.
fn build_llmcc_ir_inner<'a, L: LanguageTrait>(
    file_path: Option<String>,
    file_bytes: &'a [u8],
    parse_tree: &'a dyn ParseTree,
    config: IrBuildConfig,
) -> Result<(HirId, HashMap<HirId, HirNodeSpec>), DynError> {
    let root = parse_tree
        .root_node()
        .ok_or_else(|| "ParseTree does not provide a root node".to_string())?;

    let builder = HirBuilder::<L>::new(file_path, file_bytes, config);
    let result = builder.build(root.as_ref());
    Ok(result)
}

/// Result container for parallel file IR building.
///
/// Holds the output from building IR for a single file, including its index
/// for later reassembly in the correct order.
struct FileIrBuildResult {
    /// Index of this file in the compile context
    index: usize,
    /// HirId of the file's root node
    file_start_id: HirId,
    /// All node specifications for this file
    node_specs: HashMap<HirId, HirNodeSpec>,
}

/// Build IR for all files in the compile context.
///
/// Performs parallel multi-file IR building using Rayon for efficient compilation
/// of large projects. Files are processed independently, then results are collected
/// and stored in order.
///
/// # Type Parameters
///
/// - `L`: Language implementation (must be `Send + Sync` for parallel processing)
///
/// # Algorithm
///
/// 1. **Parallel Phase**: Each file's IR is built independently in a thread pool
/// 2. **Collection Phase**: Results are collected from the par_iter into a Vec
/// 3. **Sorting Phase**: Results are sorted by index to maintain deterministic order
/// 4. **Allocation Phase**: Sequential phase allocates nodes to arena and updates context
///
/// This approach ensures:
/// - Maximum parallelism for independent file parsing
/// - Deterministic ID allocation (global counter maintained)
/// - Consistent arena allocation order despite parallel processing
///
/// # Errors
///
/// Returns an error if any file fails to build (propagated via `?` in par_iter).
pub fn build_llmcc_ir<'a, L: LanguageTrait>(
    cc: &'a CompileCtxt<'a>,
    config: IrBuildConfig,
) -> Result<(), DynError> {
    let results: Vec<Result<FileIrBuildResult, DynError>> = (0..cc.files.len())
        .into_par_iter()
        .map(|index| {
            let unit = cc.compile_unit(index);
            let file_path = unit.file_path().map(|p| p.to_string());
            let file_bytes = unit.file().content();

            let parse_tree = cc
                .get_parse_tree(index)
                .ok_or_else(|| format!("No parse tree for unit {}", index))?;

            build_llmcc_ir_inner::<L>(file_path, file_bytes, parse_tree.as_ref(), config).map(
                |(file_start_id, node_specs)| FileIrBuildResult {
                    index,
                    file_start_id,
                    node_specs,
                },
            )
        })
        .collect();

    let mut results: Vec<FileIrBuildResult> = results.into_iter().collect::<Result<Vec<_>, _>>()?;

    // Sort by index to maintain deterministic order despite parallel processing
    results.sort_by_key(|result| result.index);

    // Sequential allocation into arena
    for result in results {
        let FileIrBuildResult {
            index,
            file_start_id,
            node_specs,
        } = result;

        {
            let mut hir_map = cc.hir_map.write();
            for (id, spec) in node_specs {
                let parented_node = spec.into_parented_node(&cc.arena);
                hir_map.insert(id, parented_node);
            }
        }

        cc.set_file_start(index, file_start_id);
    }

    Ok(())
}
