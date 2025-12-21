use parking_lot::RwLock;
use rayon::prelude::*;
use std::cmp::Ordering as CmpOrdering;
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::time::Instant;
use tree_sitter::Node;
use uuid::Uuid;

use crate::block::{BasicBlock, BlockArena, BlockId, reset_block_id_counter};
use crate::block_rel::{BlockIndexMaps, BlockRelationMap};
use crate::file::File;
use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirBase, HirId, HirIdent, HirKind, HirNode};
use crate::ir_builder::reset_hir_id_counter;
use crate::lang_def::{LanguageTrait, ParseTree};
use crate::scope::Scope;
use crate::symbol::{ScopeId, SymId, Symbol, reset_scope_id_counter, reset_symbol_id_counter};

#[derive(Debug, Copy, Clone)]
pub struct CompileUnit<'tcx> {
    pub cc: &'tcx CompileCtxt<'tcx>,
    pub index: usize,
}

impl<'tcx> CompileUnit<'tcx> {
    pub fn file(&self) -> &'tcx File {
        &self.cc.files[self.index]
    }

    /// Get the generic parse tree for this compilation unit
    pub fn parse_tree(&self) -> Option<&dyn ParseTree> {
        self.cc
            .parse_trees
            .get(self.index)
            .and_then(|t| t.as_deref())
    }

    /// Access the shared string interner.
    pub fn interner(&self) -> &InternPool {
        &self.cc.interner
    }

    /// Intern a string and return its symbol.
    pub fn intern_str<S>(&self, value: S) -> InternedStr
    where
        S: AsRef<str>,
    {
        self.cc.interner.intern(value)
    }

    /// Resolve an interned symbol into an owned string.
    pub fn resolve_interned_owned(&self, symbol: InternedStr) -> Option<String> {
        self.cc.interner.resolve_owned(symbol)
    }

    /// Resolve an interned symbol to string reference with "<unnamed>" as default.
    /// Useful for display purposes where you need a string that lives for 'static or is owned.
    pub fn resolve_name_or(&self, symbol: InternedStr, default: &str) -> String {
        self.resolve_interned_owned(symbol)
            .unwrap_or_else(|| default.to_string())
    }

    /// Resolve an interned symbol to string, using "<unnamed>" as default if not found.
    pub fn resolve_name(&self, symbol: InternedStr) -> String {
        self.resolve_name_or(symbol, "<unnamed>")
    }

    pub fn file_root_id(&self) -> Option<HirId> {
        self.cc.file_root_id(self.index)
    }

    pub fn file_path(&self) -> Option<&str> {
        self.cc.file_path(self.index)
    }

    /// Reserve a new block ID
    pub fn reserve_block_id(&self) -> BlockId {
        BlockId::allocate()
    }

    /// Get text from the file between start and end byte positions
    pub fn get_text(&self, start: usize, end: usize) -> String {
        self.file().get_text(start, end)
    }

    /// Convenience: extract text for a Tree-sitter node.
    pub fn ts_text(&self, node: Node<'tcx>) -> String {
        self.get_text(node.start_byte(), node.end_byte())
    }

    /// Convenience: extract text for a HIR node.
    pub fn hir_text(&self, node: &HirNode<'tcx>) -> String {
        self.get_text(node.start_byte(), node.end_byte())
    }

    /// Get a HIR node by ID, returning None if not found
    pub fn opt_hir_node(self, id: HirId) -> Option<HirNode<'tcx>> {
        self.cc.get_hir_node(id)
    }

    /// Get a HIR node by ID, panicking if not found
    pub fn hir_node(self, id: HirId) -> HirNode<'tcx> {
        self.opt_hir_node(id)
            .unwrap_or_else(|| panic!("hir node not found {}", id))
    }

    /// Get a HIR node by ID, returning None if not found
    pub fn opt_bb(self, id: BlockId) -> Option<BasicBlock<'tcx>> {
        // Direct indexing into block arena Vec using BlockId (offset by 1 since BlockId starts at 1)
        let index = (id.0 as usize).saturating_sub(1);
        self.cc.block_arena.bb().get(index).map(|bb| (*bb).clone())
    }

    /// Get a HIR node by ID, panicking if not found
    pub fn bb(self, id: BlockId) -> BasicBlock<'tcx> {
        self.opt_bb(id)
            .unwrap_or_else(|| panic!("basic block not found: {}", id))
    }

    /// Get the parent of a HIR node
    pub fn parent_node(self, id: HirId) -> Option<HirId> {
        self.opt_hir_node(id).and_then(|node| node.parent())
    }

    /// Get an existing scope or None if it doesn't exist
    pub fn opt_get_scope(self, scope_id: ScopeId) -> Option<&'tcx Scope<'tcx>> {
        self.cc.opt_get_scope(scope_id)
    }

    /// Get a symbol by ID, delegating to CompileCtxt
    pub fn opt_get_symbol(self, owner: SymId) -> Option<&'tcx Symbol> {
        self.cc.opt_get_symbol(owner)
    }

    /// Get an existing scope or panics if it doesn't exist
    pub fn get_scope(self, scope_id: ScopeId) -> &'tcx Scope<'tcx> {
        self.opt_get_scope(scope_id)
            .expect("ScopeId not mapped to Scope in CompileCtxt")
    }

    pub fn insert_block(&self, id: BlockId, block: BasicBlock<'tcx>, _parent: BlockId) {
        // Get block info before allocation
        let block_kind = block.kind();
        let block_name = block
            .base()
            .and_then(|base| base.opt_get_name())
            .map(|s| s.to_string());

        // Allocate block into the Arena Vec using BlockId as index
        self.cc.block_arena.alloc(block);

        // Register the block in the index maps
        self.cc
            .block_indexes
            .write()
            .insert_block(id, block_name, block_kind, self.index);
    }
}

impl<'tcx> Deref for CompileUnit<'tcx> {
    type Target = CompileCtxt<'tcx>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.cc
    }
}

#[derive(Debug, Clone)]
pub struct ParentedNode<'tcx> {
    pub node: HirNode<'tcx>,
}

impl<'tcx> ParentedNode<'tcx> {
    pub fn new(node: HirNode<'tcx>) -> Self {
        Self { node }
    }

    /// Get a reference to the wrapped node
    pub fn node(&self) -> &HirNode<'tcx> {
        &self.node
    }

    /// Get the parent ID
    pub fn parent(&self) -> Option<HirId> {
        self.node.parent()
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileParseMetric {
    pub path: String,
    pub seconds: f64,
}

#[derive(Debug, Clone, Default)]
pub struct BuildMetrics {
    pub file_read_seconds: f64,
    pub parse_wall_seconds: f64,
    pub parse_cpu_seconds: f64,
    pub parse_avg_seconds: f64,
    pub parse_file_count: usize,
    pub parse_slowest: Vec<FileParseMetric>,
}

#[derive(Default)]
pub struct CompileCtxt<'tcx> {
    pub arena: Arena<'tcx>,
    pub interner: InternPool,
    pub files: Vec<File>,
    /// Generic parse trees from language-specific parsers
    pub parse_trees: Vec<Option<Box<dyn ParseTree>>>,
    pub hir_root_ids: RwLock<Vec<Option<HirId>>>,

    pub block_arena: BlockArena<'tcx>,
    pub related_map: BlockRelationMap,

    /// Index maps for efficient block lookups by name, kind, unit, and id
    pub block_indexes: RwLock<BlockIndexMaps>,

    /// Metrics collected while building the compilation context
    pub build_metrics: BuildMetrics,
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
    /// Create a new CompileCtxt from source code
    pub fn from_sources<L: LanguageTrait>(sources: &[Vec<u8>]) -> Self {
        // Write sources to a unique temporary directory using UUID
        let temp_dir = std::env::temp_dir()
            .join("llmcc")
            .join(Uuid::new_v4().to_string());
        let _ = fs::create_dir_all(&temp_dir);

        let paths: Vec<String> = sources
            .iter()
            .enumerate()
            .map(|(index, src)| {
                let path = temp_dir.join(format!("source_{}.rs", index));
                if let Ok(mut file) = fs::File::create(&path) {
                    let _ = file.write_all(src);
                }
                path.to_string_lossy().to_string()
            })
            .collect();

        // Use from_files to parse and build context
        Self::from_files::<L>(&paths).unwrap_or_else(|_| {
            // Fallback: create empty context if temp file creation fails
            Self::default()
        })
    }

    /// Create a new CompileCtxt from files
    pub fn from_files<L: LanguageTrait>(paths: &[String]) -> std::io::Result<Self> {
        reset_hir_id_counter();
        reset_symbol_id_counter();
        reset_scope_id_counter();
        reset_block_id_counter();

        let read_start = Instant::now();

        let mut files_with_index: Vec<(usize, File)> = paths
            .par_iter()
            .enumerate()
            .map(|(index, path)| -> std::io::Result<(usize, File)> {
                let file = File::new_file(path.clone())?;
                Ok((index, file))
            })
            .collect::<std::io::Result<Vec<_>>>()?;

        files_with_index.sort_by_key(|(index, _)| *index);
        let files: Vec<File> = files_with_index.into_iter().map(|(_, file)| file).collect();

        let file_read_seconds = read_start.elapsed().as_secs_f64();

        let (parse_trees, mut metrics) = Self::parse_files_with_metrics::<L>(&files);
        metrics.file_read_seconds = file_read_seconds;

        let count = files.len();
        Ok(Self {
            arena: Arena::default(),
            interner: InternPool::default(),
            files,
            parse_trees,
            hir_root_ids: RwLock::new(vec![None; count]),
            block_arena: BlockArena::default(),
            related_map: BlockRelationMap::default(),
            block_indexes: RwLock::new(BlockIndexMaps::new()),
            build_metrics: metrics,
        })
    }

    /// Create a new CompileCtxt from files with separate physical and logical paths.
    /// Physical paths are used to read files from disk; logical paths are stored for display.
    /// Each element is (physical_path, logical_path).
    pub fn from_files_with_logical<L: LanguageTrait>(
        paths: &[(String, String)],
    ) -> std::io::Result<Self> {
        reset_hir_id_counter();
        reset_symbol_id_counter();
        reset_scope_id_counter();
        reset_block_id_counter();

        let read_start = Instant::now();

        let mut files_with_index: Vec<(usize, File)> = paths
            .par_iter()
            .enumerate()
            .map(
                |(index, (physical, logical))| -> std::io::Result<(usize, File)> {
                    let file = File::new_file_with_logical(physical, logical.clone())?;
                    Ok((index, file))
                },
            )
            .collect::<std::io::Result<Vec<_>>>()?;

        files_with_index.sort_by_key(|(index, _)| *index);
        let files: Vec<File> = files_with_index.into_iter().map(|(_, file)| file).collect();

        let file_read_seconds = read_start.elapsed().as_secs_f64();

        let (parse_trees, mut metrics) = Self::parse_files_with_metrics::<L>(&files);
        metrics.file_read_seconds = file_read_seconds;

        let count = files.len();
        Ok(Self {
            arena: Arena::default(),
            interner: InternPool::default(),
            files,
            parse_trees,
            hir_root_ids: RwLock::new(vec![None; count]),
            block_arena: BlockArena::default(),
            related_map: BlockRelationMap::default(),
            block_indexes: RwLock::new(BlockIndexMaps::new()),
            build_metrics: metrics,
        })
    }

    fn parse_files_with_metrics<L: LanguageTrait>(
        files: &[File],
    ) -> (Vec<Option<Box<dyn ParseTree>>>, BuildMetrics) {
        struct ParseRecord {
            tree: Option<Box<dyn ParseTree>>,
            elapsed: f64,
            path: Option<String>,
        }

        let parse_wall_start = Instant::now();
        let records: Vec<ParseRecord> = files
            .par_iter()
            .map(|file| {
                let path = file.path().map(|p| p.to_string());
                let per_file_start = Instant::now();
                let tree = L::parse(file.content());
                let elapsed = per_file_start.elapsed().as_secs_f64();
                ParseRecord {
                    tree,
                    elapsed,
                    path,
                }
            })
            .collect();
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

        (trees, metrics)
    }

    /// Sentinel owner id reserved for the global scope so that file-level scopes
    /// (whose HIR id often defaults to 0) do not reuse the same `Scope` instance.
    pub const GLOBAL_SCOPE_OWNER: HirId = HirId(usize::MAX);

    /// Create a context that references this CompileCtxt for a specific file index
    pub fn compile_unit(&'tcx self, index: usize) -> CompileUnit<'tcx> {
        CompileUnit { cc: self, index }
    }

    pub fn create_unit_globals(&'tcx self, owner: HirId) -> &'tcx Scope<'tcx> {
        // Scope already in Arena
        self.arena
            .alloc(Scope::new_with(owner, None, Some(&self.interner)))
    }

    pub fn create_globals(&'tcx self) -> &'tcx Scope<'tcx> {
        self.create_unit_globals(Self::GLOBAL_SCOPE_OWNER)
    }

    pub fn get_scope(&'tcx self, scope_id: ScopeId) -> &'tcx Scope<'tcx> {
        let index = (scope_id.0).saturating_sub(1);
        self.arena
            .scope()
            .get(index)
            .copied()
            .expect("ScopeId not mapped to Scope in CompileCtxt")
    }

    pub fn opt_get_scope(&'tcx self, scope_id: ScopeId) -> Option<&'tcx Scope<'tcx>> {
        // Direct lookup from Arena using offset, following redirects if scope was merged
        let index = scope_id.0;
        self.arena.scope().get(index).and_then(|scope| {
            if let Some(target_id) = scope.get_redirect() {
                // Follow redirect chain
                self.opt_get_scope(target_id)
            } else {
                Some(scope)
            }
        })
    }

    pub fn opt_get_symbol(&'tcx self, owner: SymId) -> Option<&'tcx Symbol> {
        self.arena.symbol().get(owner.0).copied()
    }

    pub fn get_symbol(&'tcx self, owner: SymId) -> &'tcx Symbol {
        self.opt_get_symbol(owner)
            .expect("SymId not mapped to Symbol in CompileCtxt")
    }

    /// Find the primary symbol associated with a block ID
    pub fn find_symbol_by_block_id(&'tcx self, block_id: BlockId) -> Option<&'tcx Symbol> {
        self.arena
            .symbol()
            .iter()
            .find(|symbol| symbol.block_id() == Some(block_id))
            .copied()
    }

    /// Access the arena for allocations
    pub fn arena(&'tcx self) -> &'tcx Arena<'tcx> {
        &self.arena
    }

    /// Allocate a new file identifier node with the given ID, name and symbol
    pub fn alloc_file_ident(
        &'tcx self,
        id: HirId,
        name: &str,
        symbol: &'tcx Symbol,
    ) -> &'tcx HirIdent<'tcx> {
        let base = HirBase {
            id,
            parent: None,
            kind_id: 0,
            start_byte: 0,
            end_byte: 0,
            kind: HirKind::Identifier,
            field_id: u16::MAX,
            children: Vec::new(),
        };
        let ident = self.arena.alloc(HirIdent::new(base, name.to_string()));
        ident.set_symbol(symbol);
        ident
    }

    pub fn alloc_scope(&'tcx self, owner: HirId) -> &'tcx Scope<'tcx> {
        self.arena
            .alloc(Scope::new_with(owner, None, Some(&self.interner)))
    }

    /// Merge the second scope into the first.
    ///
    /// This combines all symbols from the second scope into the first scope.
    /// Any future lookup of second's scope ID will redirect to first.
    pub fn merge_two_scopes(&'tcx self, first: &'tcx Scope<'tcx>, second: &'tcx Scope<'tcx>) {
        // Merge symbols from second into first
        first.merge_with(second, self.arena());
        // Redirect second's scope ID to first's scope ID so lookups redirect
        second.set_redirect(first.id());
    }

    pub fn set_file_root_id(&self, index: usize, start: HirId) {
        let mut starts = self.hir_root_ids.write();
        if index < starts.len() && starts[index].is_none() {
            starts[index] = Some(start);
        }
    }

    pub fn file_root_id(&self, index: usize) -> Option<HirId> {
        self.hir_root_ids.read().get(index).and_then(|opt| *opt)
    }

    pub fn file_path(&self, index: usize) -> Option<&str> {
        self.files.get(index).and_then(|file| file.path())
    }

    /// Get the generic parse tree for a specific file
    pub fn get_parse_tree(&self, index: usize) -> Option<&dyn ParseTree> {
        self.parse_trees.get(index).and_then(|t| t.as_deref())
    }

    /// Get all file paths from the compilation context
    pub fn get_files(&self) -> Vec<String> {
        self.files
            .iter()
            .filter_map(|f| f.path().map(|p| p.to_string()))
            .collect()
    }

    // ========== HIR Map APIs ==========

    /// Get a HIR node by ID from the Arena (O(1) lookup using ID as index)
    /// HIR IDs correspond directly to positions in the Arena vector
    pub fn get_hir_node(&self, id: HirId) -> Option<HirNode<'tcx>> {
        self.arena.hir_node().get(id.0).map(|node_ref| **node_ref)
    }

    /// Check if a HIR node exists in the Arena (O(1) check using ID as index)
    pub fn hir_node_exists(&self, id: HirId) -> bool {
        let hir_nodes = self.arena.hir_node();
        id.0 < hir_nodes.len()
    }

    /// Get the total count of HIR nodes in the Arena
    pub fn hir_node_count(&self) -> usize {
        self.arena.hir_node().len()
    }

    /// Get all HIR node IDs from the Arena
    pub fn all_hir_node_ids(&self) -> Vec<HirId> {
        self.arena.hir_node().iter().map(|node| node.id()).collect()
    }

    // ========== Block Indexes APIs ==========

    /// Get all blocks by name
    pub fn find_blocks_by_name(
        &self,
        name: &str,
    ) -> Vec<(usize, crate::block::BlockKind, BlockId)> {
        self.block_indexes.read().find_by_name(name)
    }

    /// Get all blocks by kind
    pub fn find_blocks_by_kind(
        &self,
        kind: crate::block::BlockKind,
    ) -> Vec<(usize, Option<String>, BlockId)> {
        self.block_indexes.read().find_by_kind(kind)
    }

    /// Get blocks in a specific unit
    pub fn find_blocks_in_unit(
        &self,
        unit_index: usize,
    ) -> Vec<(Option<String>, crate::block::BlockKind, BlockId)> {
        self.block_indexes.read().find_by_unit(unit_index)
    }

    /// Get blocks of a specific kind in a specific unit
    pub fn find_blocks_by_kind_in_unit(
        &self,
        kind: crate::block::BlockKind,
        unit_index: usize,
    ) -> Vec<BlockId> {
        self.block_indexes
            .read()
            .find_by_kind_and_unit(kind, unit_index)
    }

    /// Get block info by ID
    pub fn get_block_info(
        &self,
        block_id: BlockId,
    ) -> Option<(usize, Option<String>, crate::block::BlockKind)> {
        self.block_indexes.read().get_block_info(block_id)
    }

    /// Get all blocks with their metadata
    pub fn get_all_blocks(&self) -> Vec<(BlockId, usize, Option<String>, crate::block::BlockKind)> {
        self.block_indexes.read().iter_all_blocks()
    }

    // ========== Symbol Map APIs ==========

    /// Get all symbols from the symbol map
    pub fn get_all_symbols(&'tcx self) -> Vec<&'tcx Symbol> {
        self.arena.symbol().iter().copied().collect()
    }

    /// Get the count of registered symbols (excluding unresolved)
    pub fn symbol_count(&self) -> usize {
        self.arena.symbol().iter().count()
    }

    /// Iterate over all symbols and their IDs (excluding unresolved)
    pub fn for_each_symbol<F>(&self, mut f: F)
    where
        F: FnMut(SymId, &'tcx Symbol),
    {
        for symbol in self.arena.symbol().iter() {
            f(symbol.id(), symbol);
        }
    }
}
