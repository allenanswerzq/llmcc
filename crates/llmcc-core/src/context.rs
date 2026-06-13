//! Compilation context and unit management.

use parking_lot::RwLock;
use rayon::prelude::*;
use smallvec::SmallVec;
use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tree_sitter::Node;

use crate::block::{
    ArenaInsertWithId as BlockArenaInsertWithId, BasicBlock, BlockArena, BlockId,
    reset_block_id_counter,
};
use crate::block_rel::{BlockIndexEntry, BlockIndexMaps, BlockRelationMap};
use crate::file::File;
use crate::id::reset_hir_id_counter;
use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirBase, HirId, HirIdent, HirKind, HirNode};
use crate::lang_def::{Language, ParseTree};
use crate::meta::{UnitMeta, UnitMetaIndex};
use crate::scope::Scope;
use crate::symbol::{ScopeId, SymId, Symbol, reset_scope_id_counter, reset_symbol_id_counter};
use crate::{Error, ErrorKind, Result};

/// Controls how source files are ordered after parallel reading.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FileOrder {
    /// Preserve the original input order (deterministic, good for tests).
    #[default]
    Original,
    /// Sort by file size descending (better parallel load balancing for large projects).
    BySizeDescending,
}

/// File-scoped view into a [`CompileCtxt`].
///
/// A compile unit is cheap to copy and carries the file index needed by language
/// collectors, binders, graph builders, and renderers.
#[derive(Debug, Copy, Clone)]
pub struct CompileUnit<'tcx> {
    cc: &'tcx CompileCtxt<'tcx>,
    index: usize,
}

impl<'tcx> CompileUnit<'tcx> {
    /// Return the parent compilation context.
    pub fn context(&self) -> &'tcx CompileCtxt<'tcx> {
        self.cc
    }

    /// Return this unit's zero-based file index in the context.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Return this unit's source file.
    pub fn file(&self) -> &'tcx File {
        &self.cc.files[self.index]
    }

    /// Return this unit's parse tree, if it has been loaded.
    pub fn try_parse_tree(&self) -> Option<&dyn ParseTree> {
        self.cc.try_parse_tree(self.index)
    }

    /// Return this unit's parse tree or an invariant error when it is missing.
    pub fn parse_tree(&self) -> Result<&dyn ParseTree> {
        self.cc.parse_tree(self.index)
    }

    /// Return the shared string interner.
    pub fn interner(&self) -> &InternPool {
        self.cc.interner()
    }

    /// Intern a string and return its symbol.
    pub fn intern_str<S>(&self, value: S) -> InternedStr
    where
        S: AsRef<str>,
    {
        self.cc.interner().intern(value)
    }

    /// Resolve an interned symbol into an owned string.
    pub fn resolve_interned_owned(&self, symbol: InternedStr) -> Option<String> {
        self.cc.interner().resolve_owned(symbol)
    }

    /// Resolve an interned symbol to an owned string, using `default` when missing.
    pub fn resolve_name_or(&self, symbol: InternedStr, default: &str) -> String {
        self.resolve_interned_owned(symbol)
            .unwrap_or_else(|| default.to_string())
    }

    /// Resolve an interned symbol to an owned string, using `<unnamed>` when missing.
    pub fn resolve_name(&self, symbol: InternedStr) -> String {
        self.resolve_name_or(symbol, "<unnamed>")
    }

    /// Return this unit's HIR root id, if HIR has been built.
    pub fn try_file_root_id(&self) -> Option<HirId> {
        self.cc.try_file_root_id(self.index)
    }

    /// Return this unit's HIR root id or an invariant error when it is missing.
    pub fn file_root_id(&self) -> Result<HirId> {
        self.cc.file_root_id(self.index)
    }

    /// Return this unit's file path, if the unit is file-backed.
    pub fn file_path(&self) -> Option<&str> {
        self.cc.file_path(self.index)
    }

    /// Return this unit's project/package/module/file metadata.
    pub fn unit_meta(&self) -> &UnitMeta {
        self.cc
            .unit_meta(self.index)
            .expect("CompileUnit index not mapped to UnitMeta")
    }

    /// Reserve a new block id.
    pub fn reserve_block_id(&self) -> BlockId {
        BlockId::allocate()
    }

    /// Allocate a value in the block arena using a block id.
    pub(crate) fn alloc_block<T>(&self, id: BlockId, block: T) -> &'tcx T
    where
        T: BlockArenaInsertWithId<'tcx>,
    {
        self.cc.block_arena.alloc_with_id(id.0 as usize, block)
    }

    /// Return source text between byte offsets.
    pub fn source_text(&self, start: usize, end: usize) -> String {
        self.file().get_text(start, end)
    }

    /// Convenience: extract text for a Tree-sitter node.
    pub fn ts_text(&self, node: Node<'tcx>) -> String {
        self.source_text(node.start_byte(), node.end_byte())
    }

    /// Convenience: extract text for a HIR node.
    pub fn hir_text(&self, node: &HirNode<'tcx>) -> String {
        self.source_text(node.start_byte(), node.end_byte())
    }

    /// Return a HIR node by id, if it exists.
    pub fn try_hir_node(self, id: HirId) -> Option<HirNode<'tcx>> {
        self.cc.try_hir_node(id)
    }

    /// Return a HIR node by id, panicking when it is missing.
    pub fn hir_node(self, id: HirId) -> HirNode<'tcx> {
        self.try_hir_node(id)
            .unwrap_or_else(|| panic!("hir node not found {id}"))
    }

    /// Return a basic block by id, if it exists.
    pub fn try_block(self, id: BlockId) -> Option<BasicBlock<'tcx>> {
        self.cc.try_block(id)
    }

    /// Return a basic block by id, panicking when it is missing.
    pub fn block(self, id: BlockId) -> BasicBlock<'tcx> {
        self.try_block(id)
            .unwrap_or_else(|| panic!("basic block not found: {id}"))
    }

    /// Return this unit's root block, if graph building has produced one.
    pub fn root_block(self) -> Option<BasicBlock<'tcx>> {
        let root_blocks = self
            .cc
            .find_blocks_by_kind_in_unit(crate::block::BlockKind::Root, self.index);
        root_blocks.first().and_then(|&id| self.try_block(id))
    }

    /// Return a HIR node's parent id, if the node exists and has a parent.
    pub fn parent_node(self, id: HirId) -> Option<HirId> {
        self.try_hir_node(id).and_then(|node| node.parent())
    }

    /// Return a scope by id, if it exists.
    pub fn try_scope(self, scope_id: ScopeId) -> Option<&'tcx Scope<'tcx>> {
        self.cc.try_scope(scope_id)
    }

    /// Return a symbol by id, if it exists.
    pub fn try_symbol(self, owner: SymId) -> Option<&'tcx Symbol> {
        self.cc.try_symbol(owner)
    }

    /// Return the symbol referenced by `symbol.type_of()`, if both links exist.
    pub fn try_type(self, symbol: &Symbol) -> Option<&'tcx Symbol> {
        symbol.type_of().and_then(|id| self.try_symbol(id))
    }

    /// Return the graph-display type symbol for an already-bound symbol.
    pub fn try_effective_type(self, symbol: Option<&'tcx Symbol>) -> Option<&'tcx Symbol> {
        let symbol = symbol?;
        if symbol.kind() == crate::symbol::SymKind::EnumVariant {
            return None;
        }

        let type_symbol = self.try_type(symbol).unwrap_or(symbol);
        if type_symbol.kind() == crate::symbol::SymKind::TypeParameter {
            return self.try_type(type_symbol).or(Some(type_symbol));
        }

        Some(type_symbol)
    }

    /// Return a scope by id, panicking when it is missing.
    pub fn scope(self, scope_id: ScopeId) -> &'tcx Scope<'tcx> {
        self.try_scope(scope_id)
            .expect("ScopeId not mapped to Scope in CompileCtxt")
    }

    /// Insert a block and update the block indexes for this unit.
    pub fn insert_block(&self, id: BlockId, block: BasicBlock<'tcx>) {
        let block_kind = block.kind();
        let block_name = block.try_name().map(|name| name.to_string());
        self.alloc_block(id, block);
        self.cc
            .block_indexes
            .insert_block(id, block_name, block_kind, self.index);
    }
}

/// Parse timing for a single source file.
#[derive(Debug, Clone, Default)]
pub struct FileParseMetric {
    pub path: String,
    pub seconds: f64,
}

/// Metrics collected while loading and parsing a compilation context.
#[derive(Debug, Clone, Default)]
pub struct BuildMetrics {
    pub file_read_seconds: f64,
    pub parse_wall_seconds: f64,
    pub parse_cpu_seconds: f64,
    pub parse_avg_seconds: f64,
    pub parse_file_count: usize,
    pub parse_slowest: Vec<FileParseMetric>,
}

struct LoadedFiles {
    files: Vec<File>,
    read_seconds: f64,
}

#[derive(Default)]
pub struct CompileCtxt<'tcx> {
    pub(crate) arena: Arena<'tcx>,
    pub(crate) interner: InternPool,
    pub(crate) files: Vec<File>,
    /// Per-file metadata: package/module/file names and roots.
    pub(crate) unit_metas: Vec<UnitMeta>,
    /// Generic parse trees from language-specific parsers
    pub(crate) parse_trees: Vec<Box<dyn ParseTree>>,
    pub(crate) hir_root_ids: RwLock<Vec<Option<HirId>>>,

    pub(crate) block_arena: BlockArena<'tcx>,
    pub(crate) related_map: BlockRelationMap,

    /// Index maps for efficient block lookups by name, kind, unit, and id
    /// Uses DashMap internally for concurrent access
    pub(crate) block_indexes: BlockIndexMaps,

    /// Metrics collected while building the compilation context
    pub(crate) build_metrics: BuildMetrics,
}

impl<'tcx> std::fmt::Debug for CompileCtxt<'tcx> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompileCtxt")
            .field("files", &self.files.len())
            .field("parse_trees", &self.parse_trees.len())
            .field("build_metrics", &self.build_metrics)
            .finish()
    }
}

impl<'tcx> CompileCtxt<'tcx> {
    /// Build a context from source file paths, preserving input order.
    pub fn from_files<L: Language>(paths: &[String]) -> Result<Self> {
        Self::from_files_with_order::<L>(paths, FileOrder::Original)
    }

    /// Build a context from source file paths with a selected read order.
    pub fn from_files_with_order<L: Language>(paths: &[String], order: FileOrder) -> Result<Self> {
        Self::reset_context_counters();
        let LoadedFiles {
            files,
            read_seconds,
        } = Self::read_files(paths, order)?;

        let (parse_trees, mut metrics) = Self::parse_files_with_metrics::<L>(&files)?;
        metrics.file_read_seconds = read_seconds;

        let unit_metas = Self::build_unit_metas::<L>(&files);
        Ok(Self::from_parts(files, unit_metas, parse_trees, metrics))
    }

    fn reset_context_counters() {
        reset_hir_id_counter();
        reset_symbol_id_counter();
        reset_scope_id_counter();
        reset_block_id_counter();
    }

    fn read_files(paths: &[String], order: FileOrder) -> Result<LoadedFiles> {
        let read_start = Instant::now();
        let mut indexed_files: Vec<(usize, File)> = paths
            .par_iter()
            .enumerate()
            .map(|(index, path)| -> Result<(usize, File)> {
                Ok((index, File::new_file(path.clone())?))
            })
            .collect::<Result<Vec<_>>>()?;

        Self::order_loaded_files(&mut indexed_files, order);
        let files = indexed_files.into_iter().map(|(_, file)| file).collect();

        Ok(LoadedFiles {
            files,
            read_seconds: read_start.elapsed().as_secs_f64(),
        })
    }

    fn order_loaded_files(indexed_files: &mut [(usize, File)], order: FileOrder) {
        match order {
            FileOrder::Original => indexed_files.sort_by_key(|(index, _)| *index),
            FileOrder::BySizeDescending => {
                indexed_files.sort_by_key(|(_, file)| std::cmp::Reverse(file.content().len()))
            }
        }
    }

    fn from_parts(
        files: Vec<File>,
        unit_metas: Vec<UnitMeta>,
        parse_trees: Vec<Box<dyn ParseTree>>,
        build_metrics: BuildMetrics,
    ) -> Self {
        let unit_count = files.len();
        debug_assert_eq!(unit_metas.len(), unit_count);
        debug_assert_eq!(parse_trees.len(), unit_count);

        Self {
            arena: Arena::default(),
            interner: InternPool::default(),
            files,
            unit_metas,
            parse_trees,
            hir_root_ids: RwLock::new(vec![None; unit_count]),
            block_arena: BlockArena::default(),
            related_map: BlockRelationMap::default(),
            block_indexes: BlockIndexMaps::new(),
            build_metrics,
        }
    }

    /// Build unit metadata for all files using UnitMetaIndex.
    fn build_unit_metas<L: Language>(files: &[File]) -> Vec<UnitMeta> {
        if files.is_empty() {
            return Vec::new();
        }

        let file_paths = Self::metadata_paths(files);
        if file_paths.is_empty() {
            return vec![UnitMeta::default(); files.len()];
        }

        let meta_index = UnitMetaIndex::from_language::<L>(&file_paths);
        let mut metas: Vec<UnitMeta> = files
            .iter()
            .map(|file| Self::metadata_for_file(&meta_index, file))
            .collect();

        Self::assign_crate_indexes(&mut metas);
        metas
    }

    fn metadata_paths(files: &[File]) -> Vec<PathBuf> {
        files
            .iter()
            .filter_map(|file| file.path().map(PathBuf::from))
            .collect()
    }

    fn metadata_for_file(meta_index: &UnitMetaIndex, file: &File) -> UnitMeta {
        file.path()
            .map(|path| meta_index.metadata_for(Path::new(path)))
            .unwrap_or_default()
    }

    fn assign_crate_indexes(metas: &mut [UnitMeta]) {
        let mut package_crates: HashMap<PathBuf, usize> = HashMap::new();
        let mut next_crate_index = 0usize;

        for meta in metas {
            if let Some(ref package_root) = meta.package_root {
                let crate_index =
                    *package_crates
                        .entry(package_root.clone())
                        .or_insert_with(|| {
                            let crate_index = next_crate_index;
                            next_crate_index += 1;
                            crate_index
                        });
                meta.crate_index = crate_index;
            }
        }
    }

    fn parse_files_with_metrics<L: Language>(
        files: &[File],
    ) -> Result<(Vec<Box<dyn ParseTree>>, BuildMetrics)> {
        struct ParseRecord {
            tree: Box<dyn ParseTree>,
            elapsed: f64,
            path: Option<String>,
        }

        let parse_wall_start = Instant::now();
        let records: Vec<ParseRecord> = files
            .par_iter()
            .map(|file| {
                let path = file.path().map(|p| p.to_string());
                let per_file_start = Instant::now();
                let tree = L::parse(file.content()).map_err(|error| {
                    error
                        .with_operation("parse_file")
                        .with_context("path", path.as_deref().unwrap_or("<memory>"))
                })?;
                let elapsed = per_file_start.elapsed().as_secs_f64();
                Ok(ParseRecord {
                    tree,
                    elapsed,
                    path,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let parse_wall_seconds = parse_wall_start.elapsed().as_secs_f64();

        let mut trees = Vec::with_capacity(records.len());
        let parse_file_count = records.len();
        let mut parse_cpu_seconds = 0.0;
        let mut slowest = Vec::with_capacity(records.len());

        for record in records {
            parse_cpu_seconds += record.elapsed;
            trees.push(record.tree);
            let path = record.path.unwrap_or_else(|| "<memory>".to_string());
            slowest.push(FileParseMetric {
                path,
                seconds: record.elapsed,
            });
        }

        slowest.sort_by(|a, b| {
            b.seconds
                .partial_cmp(&a.seconds)
                .unwrap_or(CmpOrdering::Equal)
        });
        slowest.truncate(5);

        let metrics = BuildMetrics {
            file_read_seconds: 0.0,
            parse_wall_seconds,
            parse_cpu_seconds,
            parse_avg_seconds: if parse_file_count == 0 {
                0.0
            } else {
                parse_cpu_seconds / parse_file_count as f64
            },
            parse_file_count,
            parse_slowest: slowest,
        };

        Ok((trees, metrics))
    }

    /// Sentinel owner id reserved for the global scope so that file-level scopes
    /// (whose HIR id often defaults to 0) do not reuse the same `Scope` instance.
    pub const GLOBAL_SCOPE_OWNER: HirId = HirId(usize::MAX);

    /// Return all loaded source files.
    pub fn files(&self) -> &[File] {
        &self.files
    }

    /// Return the source file for `index`, if it exists.
    pub fn file(&self, index: usize) -> Option<&File> {
        self.files.get(index)
    }

    /// Return the shared arena used for HIR, symbol, and scope allocation.
    pub fn arena(&'tcx self) -> &'tcx Arena<'tcx> {
        &self.arena
    }

    /// Return the shared string interner.
    pub fn interner(&self) -> &InternPool {
        &self.interner
    }

    /// Return parse/build metrics collected during context construction.
    pub fn build_metrics(&self) -> &BuildMetrics {
        &self.build_metrics
    }

    /// Return read-only metadata for all units.
    pub fn unit_metas(&self) -> &[UnitMeta] {
        &self.unit_metas
    }

    /// Return metadata for a single unit.
    pub fn unit_meta(&self, index: usize) -> Option<&UnitMeta> {
        self.unit_metas.get(index)
    }

    /// Return the block-relation map built for this context.
    pub fn block_relations(&self) -> &BlockRelationMap {
        &self.related_map
    }

    /// Return file paths for all file-backed units.
    pub fn file_paths(&self) -> Vec<String> {
        self.files
            .iter()
            .filter_map(|file| file.path().map(ToOwned::to_owned))
            .collect()
    }

    /// Return the number of compilation units in this context.
    pub fn unit_count(&self) -> usize {
        self.files.len()
    }

    /// Return a compile unit for `index`, if it exists.
    pub fn try_compile_unit(&'tcx self, index: usize) -> Option<CompileUnit<'tcx>> {
        (index < self.unit_count()).then_some(CompileUnit { cc: self, index })
    }

    /// Return a compile unit for `index`, panicking when the index is invalid.
    pub fn compile_unit(&'tcx self, index: usize) -> CompileUnit<'tcx> {
        if let Some(unit) = self.try_compile_unit(index) {
            return unit;
        }

        panic!(
            "compile unit index {index} out of bounds for {} files",
            self.unit_count()
        );
    }

    /// Allocate a global scope for a single unit owner.
    pub fn create_unit_globals(&'tcx self, owner: HirId) -> &'tcx Scope<'tcx> {
        let scope = Scope::new_with(owner, None, Some(&self.interner));
        let id = scope.id().0;
        self.arena.alloc_with_id(id, scope)
    }

    /// Allocate the process-wide global scope used during resolution.
    pub fn create_globals(&'tcx self) -> &'tcx Scope<'tcx> {
        let scope =
            Scope::new_with_shards(Self::GLOBAL_SCOPE_OWNER, None, Some(&self.interner), 256);
        let id = scope.id().0;
        self.arena.alloc_with_id(id, scope)
    }

    /// Return a scope by id, panicking when it is missing.
    pub fn scope(&'tcx self, scope_id: ScopeId) -> &'tcx Scope<'tcx> {
        self.arena
            .get_scope(scope_id.0)
            .expect("ScopeId not mapped to Scope in CompileCtxt")
    }

    /// Return a scope by id, following merge redirects when present.
    pub fn try_scope(&'tcx self, scope_id: ScopeId) -> Option<&'tcx Scope<'tcx>> {
        self.arena.get_scope(scope_id.0).and_then(|scope| {
            if let Some(target_id) = scope.try_redirect() {
                self.try_scope(target_id)
            } else {
                Some(scope)
            }
        })
    }

    /// Return a symbol by id, if it exists.
    pub fn try_symbol(&'tcx self, owner: SymId) -> Option<&'tcx Symbol> {
        self.arena.get_symbol(owner.0)
    }

    /// Return a symbol by id, panicking when it is missing.
    pub fn symbol(&'tcx self, owner: SymId) -> &'tcx Symbol {
        self.try_symbol(owner)
            .expect("SymId not mapped to Symbol in CompileCtxt")
    }

    /// Return the primary symbol associated with a block id.
    pub fn find_symbol_by_block_id(&'tcx self, block_id: BlockId) -> Option<&'tcx Symbol> {
        self.arena
            .iter_symbol()
            .find(|symbol| symbol.block_id() == Some(block_id))
    }

    /// Allocate a synthetic file identifier node with the given id, name, and symbol.
    pub fn alloc_file_ident(
        &'tcx self,
        id: HirId,
        name: &'tcx str,
        symbol: &'tcx Symbol,
    ) -> &'tcx HirIdent<'tcx> {
        let base = HirBase {
            id,
            parent: None,
            kind_id: 0,
            start_byte: 0,
            end_byte: 0,
            start_line: 0,
            kind: HirKind::Identifier,
            field_id: u16::MAX,
            children: SmallVec::new(),
        };
        let ident = self.arena.alloc(HirIdent::new(base, name));
        ident.set_symbol(symbol);
        ident
    }

    /// Allocate a scope owned by `owner`.
    pub fn alloc_scope(&'tcx self, owner: HirId) -> &'tcx Scope<'tcx> {
        let scope = Scope::new_with(owner, None, Some(&self.interner));
        let id = scope.id().0;
        self.arena.alloc_with_id(id, scope)
    }

    /// Merge `second` into `first` and redirect future lookups to `first`.
    pub fn merge_two_scopes(&'tcx self, first: &'tcx Scope<'tcx>, second: &'tcx Scope<'tcx>) {
        first.merge_with(second);
        second.set_redirect(first.id());
    }

    /// Publish a unit's HIR root id if it has not already been set.
    pub fn set_file_root_id(&self, index: usize, start: HirId) {
        let mut starts = self.hir_root_ids.write();
        if index < starts.len() && starts[index].is_none() {
            starts[index] = Some(start);
        }
    }

    /// Return a unit's HIR root id, if HIR has been built.
    pub fn try_file_root_id(&self, index: usize) -> Option<HirId> {
        self.hir_root_ids.read().get(index).and_then(|opt| *opt)
    }

    /// Return a unit's HIR root id or an invariant error when it is missing.
    pub fn file_root_id(&self, index: usize) -> Result<HirId> {
        self.try_file_root_id(index).ok_or_else(|| {
            Error::new(
                ErrorKind::InvariantViolation,
                "HIR root is not available for compilation unit",
            )
            .with_operation("file_root_id")
            .with_context("unit_index", index.to_string())
            .with_context("path", self.file_path(index).unwrap_or("<memory>"))
        })
    }

    /// Return a unit's file path, if the unit is file-backed.
    pub fn file_path(&self, index: usize) -> Option<&str> {
        self.files.get(index).and_then(|file| file.path())
    }

    /// Return a unit's parse tree, if it has been loaded.
    pub fn try_parse_tree(&self, index: usize) -> Option<&dyn ParseTree> {
        self.parse_trees.get(index).map(|tree| tree.as_ref())
    }

    /// Return a unit's parse tree or an invariant error when it is missing.
    pub fn parse_tree(&self, index: usize) -> Result<&dyn ParseTree> {
        self.try_parse_tree(index).ok_or_else(|| {
            Error::new(
                ErrorKind::InvariantViolation,
                "parse tree is not available for compilation unit",
            )
            .with_operation("parse_tree")
            .with_context("unit_index", index.to_string())
            .with_context("path", self.file_path(index).unwrap_or("<memory>"))
        })
    }

    /// Return a HIR node by id.
    pub fn try_hir_node(&'tcx self, id: HirId) -> Option<HirNode<'tcx>> {
        self.arena.get_hir_node(id.0).copied()
    }

    /// Return a HIR node by id, panicking when it is missing.
    pub fn hir_node(&'tcx self, id: HirId) -> HirNode<'tcx> {
        self.try_hir_node(id)
            .unwrap_or_else(|| panic!("hir node not found {id}"))
    }

    /// Return a basic block by id.
    pub fn try_block(&'tcx self, id: BlockId) -> Option<BasicBlock<'tcx>> {
        self.block_arena.get_bb(id.0 as usize).cloned()
    }

    /// Return a basic block by id, panicking when it is missing.
    pub fn block(&'tcx self, id: BlockId) -> BasicBlock<'tcx> {
        self.try_block(id)
            .unwrap_or_else(|| panic!("basic block not found: {id}"))
    }

    /// Return whether a HIR node id is registered.
    pub fn hir_node_exists(&self, id: HirId) -> bool {
        self.arena.hir_node.contains_key(&id.0)
    }

    /// Return the number of registered HIR nodes.
    pub fn hir_node_count(&self) -> usize {
        self.arena.len_hir_node()
    }

    /// Return all registered HIR node ids.
    pub fn all_hir_node_ids(&'tcx self) -> Vec<HirId> {
        self.arena.iter_hir_node().map(|node| node.id()).collect()
    }

    /// Return blocks indexed by display name.
    pub fn find_blocks_by_name(&self, name: &'tcx str) -> Vec<BlockIndexEntry> {
        self.block_indexes.by_name(name)
    }

    /// Return blocks indexed by kind.
    pub fn find_blocks_by_kind(&self, kind: crate::block::BlockKind) -> Vec<BlockIndexEntry> {
        self.block_indexes.by_kind(kind)
    }

    /// Return blocks in a specific unit.
    pub fn find_blocks_in_unit(&self, unit_index: usize) -> Vec<BlockIndexEntry> {
        self.block_indexes.by_unit(unit_index)
    }

    /// Return block ids for a specific kind in a specific unit.
    pub fn find_blocks_by_kind_in_unit(
        &self,
        kind: crate::block::BlockKind,
        unit_index: usize,
    ) -> Vec<BlockId> {
        self.block_indexes.by_kind_in_unit(kind, unit_index)
    }

    /// Return block metadata by id.
    pub fn block_info(&self, block_id: BlockId) -> Option<BlockIndexEntry> {
        self.block_indexes.block_info(block_id)
    }

    /// Return all blocks with their metadata.
    pub fn blocks(&self) -> Vec<BlockIndexEntry> {
        self.block_indexes.blocks()
    }

    /// Return all registered symbols.
    pub fn symbols(&'tcx self) -> Vec<&'tcx Symbol> {
        self.arena.iter_symbol().collect()
    }

    /// Return the number of registered symbols.
    pub fn symbol_count(&self) -> usize {
        self.arena.len_symbol()
    }

    /// Visit every registered symbol with its id.
    pub fn for_each_symbol<F>(&'tcx self, mut f: F)
    where
        F: FnMut(SymId, &'tcx Symbol),
    {
        for symbol in self.arena.iter_symbol() {
            f(symbol.id(), symbol);
        }
    }
}
