use parking_lot::RwLock;
use rayon::prelude::*;
use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::time::Instant;
use tree_sitter::Node;
use uuid::Uuid;

use crate::block::{BasicBlock, BlockArena, BlockId};
use crate::block_rel::{BlockIndexMaps, BlockRelationMap};
use crate::file::File;
use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirBase, HirId, HirIdent, HirKind, HirNode};
use crate::lang_def::{LanguageTrait, ParseTree};
use crate::scope::Scope;
use crate::symbol::{ScopeId, SymId, Symbol};

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
        self.cc
            .hir_map
            .read()
            .get(&id)
            .map(|parented| parented.node)
    }

    /// Get a HIR node by ID, panicking if not found
    pub fn hir_node(self, id: HirId) -> HirNode<'tcx> {
        self.opt_hir_node(id)
            .unwrap_or_else(|| panic!("hir node not found {}", id))
    }

    /// Get a HIR node by ID, returning None if not found
    pub fn opt_bb(self, id: BlockId) -> Option<BasicBlock<'tcx>> {
        self.cc
            .block_map
            .read()
            .get(&id)
            .map(|parented| parented.block.clone())
    }

    /// Get a HIR node by ID, panicking if not found
    pub fn bb(self, id: BlockId) -> BasicBlock<'tcx> {
        self.opt_bb(id)
            .unwrap_or_else(|| panic!("basic block not found: {}", id))
    }

    /// Get the parent of a HIR node
    pub fn parent_node(self, id: HirId) -> Option<HirId> {
        self.cc
            .hir_map
            .read()
            .get(&id)
            .and_then(|parented| parented.parent())
    }

    /// Get an existing scope or None if it doesn't exist.
    /// Uses O(1) direct indexing. ScopeIds start from 1.
    pub fn opt_get_scope(self, scope_id: ScopeId) -> Option<&'tcx Scope<'tcx>> {
        let idx = scope_id.0.checked_sub(1)?;
        self.cc.scope_map.read().get(idx).copied()
    }

    /// Get symbol by SymId using O(1) direct indexing.
    /// SymIds start from 1, so index = sym_id.0 - 1
    pub fn opt_get_symbol(self, sym_id: SymId) -> Option<&'tcx Symbol> {
        let idx = sym_id.0.checked_sub(1)?;
        self.cc.symbol_map.read().get(idx).copied()
    }

    /// Get an existing scope or panics if it doesn't exist
    pub fn get_scope(self, scope_id: ScopeId) -> &'tcx Scope<'tcx> {
        self.opt_get_scope(scope_id)
            .expect("ScopeId not mapped to Scope in CompileCtxt")
    }

    pub fn add_unresolved_symbol(&self, symbol: &'tcx Symbol) {
        self.cc.unresolve_symbols.write().push(symbol);
    }

    pub fn insert_block(&self, id: BlockId, block: BasicBlock<'tcx>, parent: BlockId) {
        let parented = ParentedBlock::new(parent, block.clone());
        self.cc.block_map.write().insert(id, parented);

        // Register the block in the index maps
        let block_kind = block.kind();
        let block_name = block
            .base()
            .and_then(|base| base.opt_get_name())
            .map(|s| s.to_string());

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

#[derive(Debug, Clone)]
pub struct ParentedBlock<'tcx> {
    pub parent: BlockId,
    pub block: BasicBlock<'tcx>,
}

impl<'tcx> ParentedBlock<'tcx> {
    pub fn new(parent: BlockId, block: BasicBlock<'tcx>) -> Self {
        Self { parent, block }
    }

    /// Get a reference to the wrapped node
    pub fn block(&self) -> &BasicBlock<'tcx> {
        &self.block
    }

    /// Get the parent ID
    pub fn parent(&self) -> BlockId {
        self.parent
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

    // HirId -> ParentedNode
    pub hir_map: RwLock<HashMap<HirId, ParentedNode<'tcx>>>,
    // ScopeId -> Scope (sorted Vec for O(1) indexed access, index = scope_id.0 - 1)
    pub scope_map: RwLock<Vec<&'tcx Scope<'tcx>>>,
    // HirId -> ScopeId
    pub owner_to_scope_id: RwLock<HashMap<HirId, ScopeId>>,
    // SymId -> &Symbol (sorted Vec for O(1) indexed access, index = sym_id.0 - 1)
    pub symbol_map: RwLock<Vec<&'tcx Symbol>>,

    pub block_arena: BlockArena<'tcx>,
    // BlockId -> ParentedBlock
    pub block_map: RwLock<HashMap<BlockId, ParentedBlock<'tcx>>>,
    pub unresolve_symbols: RwLock<Vec<&'tcx Symbol>>,
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
            hir_map: RwLock::new(HashMap::new()),
            scope_map: RwLock::new(Vec::new()),
            owner_to_scope_id: RwLock::new(HashMap::new()),
            symbol_map: RwLock::new(Vec::new()),
            block_arena: BlockArena::default(),
            block_map: RwLock::new(HashMap::new()),
            unresolve_symbols: RwLock::new(Vec::new()),
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
            hir_map: RwLock::new(HashMap::new()),
            scope_map: RwLock::new(Vec::new()),
            owner_to_scope_id: RwLock::new(HashMap::new()),
            symbol_map: RwLock::new(Vec::new()),
            block_arena: BlockArena::default(),
            block_map: RwLock::new(HashMap::new()),
            unresolve_symbols: RwLock::new(Vec::new()),
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
        let scope = self.arena.alloc(Scope::new(owner));
        // Scope is in arena; scope_map will be built via build_lookup_maps_from_arena()
        self.owner_to_scope_id.write().insert(owner, scope.id());
        scope
    }

    pub fn create_globals(&'tcx self) -> &'tcx Scope<'tcx> {
        self.create_unit_globals(Self::GLOBAL_SCOPE_OWNER)
    }

    /// Get scope by ScopeId using O(1) direct indexing.
    /// ScopeIds start from 1, so index = scope_id.0 - 1
    pub fn get_scope(&'tcx self, scope_id: ScopeId) -> &'tcx Scope<'tcx> {
        let idx = scope_id.0.checked_sub(1).expect("Invalid ScopeId 0");
        self.scope_map
            .read()
            .get(idx)
            .copied()
            .expect("ScopeId not mapped to Scope in CompileCtxt")
    }

    /// Get symbol by SymId using O(1) direct indexing.
    /// SymIds start from 1, so index = sym_id.0 - 1
    /// Requires `build_symbol_map_from_arena()` to have been called first.
    pub fn opt_get_symbol(&'tcx self, sym_id: SymId) -> Option<&'tcx Symbol> {
        let idx = sym_id.0.checked_sub(1)?;
        self.symbol_map.read().get(idx).copied()
    }

    pub fn get_symbol(&'tcx self, sym_id: SymId) -> &'tcx Symbol {
        self.opt_get_symbol(sym_id)
            .expect("SymId not mapped to Symbol in CompileCtxt")
    }

    /// Find the primary symbol associated with a block ID
    pub fn find_symbol_by_block_id(&'tcx self, block_id: BlockId) -> Option<&'tcx Symbol> {
        self.symbol_map
            .read()
            .iter()
            .find(|symbol| symbol.block_id() == Some(block_id))
            .copied()
    }

    /// Access the arena for allocations
    pub fn arena(&'tcx self) -> &'tcx Arena<'tcx> {
        &self.arena
    }

    pub fn build_lookup_maps_from_arena(&'tcx self) {
        // Build symbol_map: sorted Vec for O(1) lookup by sym_id.0 - 1
        let mut symbols = self.arena.symbol();
        symbols.sort_unstable_by_key(|s| s.id().0);
        *self.symbol_map.write() = symbols;

        // Build scope_map: sorted Vec for O(1) lookup by scope_id.0 - 1
        let mut scopes = self.arena.scope();
        scopes.sort_unstable_by_key(|s| s.id().0);
        *self.scope_map.write() = scopes;
    }

    /// Allocate a new HIR identifier node with the given ID, name and symbol
    pub fn alloc_hir_ident(
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
        self.arena.alloc(Scope::new(owner))
    }

    /// Merge the second scope into the first.
    pub fn merge_two_scopes(&'tcx self, first: &'tcx Scope<'tcx>, second: &'tcx Scope<'tcx>) {
        // Merge symbols from second into first
        first.merge_with(second, self.arena());
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

    /// Clear all maps (useful for testing)
    #[cfg(test)]
    pub fn clear(&self) {
        self.hir_map.write().clear();
        self.scope_map.write().clear();
        self.owner_to_scope_id.write().clear();
    }
}
