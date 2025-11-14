use parking_lot::{Mutex, RwLock};
use rayon::prelude::*;
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BTreeMap, HashMap};
use std::ops::Deref;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;
use tree_sitter::Node;

use crate::block::{Arena as BlockArena, BasicBlock, BlockId, BlockKind};
use crate::block_rel::BlockRelationMap;
use crate::file::File;
use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirId, HirNode};
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
    pub fn parse_tree(&self) -> Option<&Box<dyn ParseTree>> {
        self.cc.parse_trees.get(self.index).and_then(|t| t.as_ref())
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

    pub fn reserve_hir_id(&self) -> HirId {
        self.cc.reserve_hir_id()
    }

    pub fn reserve_block_id(&self) -> BlockId {
        self.cc.reserve_block_id()
    }

    pub fn register_file_start(&self) -> HirId {
        let start = self.cc.current_hir_id();
        self.cc.set_file_start(self.index, start);
        start
    }

    pub fn file_start_hir_id(&self) -> Option<HirId> {
        self.cc.file_start(self.index)
    }

    pub fn file_path(&self) -> Option<&str> {
        self.cc.file_path(self.index)
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

    /// Get the next HIR id that will be assigned (useful for diagnostics).
    pub fn hir_next(&self) -> HirId {
        self.cc.current_hir_id()
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

    /// Get an existing scope or None if it doesn't exist
    pub fn opt_get_scope(self, owner: HirId) -> Option<&'tcx Scope<'tcx>> {
        // Direct lookup from cache
        self.cc.scope_cache.read().get(&owner).copied()
    }

    pub fn opt_get_symbol(self, owner: SymId) -> Option<&'tcx Symbol> {
        self.cc.symbol_map.read().get(&owner).copied()
    }

    /// Get an existing scope or panics if it doesn't exist
    pub fn get_scope(self, owner: HirId) -> &'tcx Scope<'tcx> {
        self.cc
            .scope_cache
            .read()
            .get(&owner)
            .copied()
            .expect("HirId not mapped to Scope in CompileCtxt")
    }

    /// Find an existing scope or create a new one
    pub fn alloc_scope(self, owner: HirId) -> &'tcx Scope<'tcx> {
        self.cc.alloc_scope(owner)
    }

    /// Add a HIR node to the map
    pub fn insert_hir_node(self, id: HirId, node: HirNode<'tcx>) {
        let parented = ParentedNode::new(node);
        self.cc.hir_map.write().insert(id, parented);
    }

    /// Get all child nodes of a given parent
    pub fn children_of(self, parent: HirId) -> Vec<(HirId, HirNode<'tcx>)> {
        let Some(parent_node) = self.opt_hir_node(parent) else {
            return Vec::new();
        };
        parent_node
            .children()
            .iter()
            .map(|&child_id| (child_id, self.hir_node(child_id)))
            .collect()
    }

    /// Walk up the parent chain to find an ancestor of a specific type
    pub fn find_ancestor<F>(self, mut current: HirId, predicate: F) -> Option<HirId>
    where
        F: Fn(&HirNode<'tcx>) -> bool,
    {
        while let Some(parent_id) = self.parent_node(current) {
            if let Some(parent_node) = self.opt_hir_node(parent_id) {
                if predicate(&parent_node) {
                    return Some(parent_id);
                }
                current = parent_id;
            } else {
                break;
            }
        }
        None
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

/// BlockIndexMaps provides efficient lookup of blocks by various indices.
///
/// Best practices for usage:
/// - block_name_index: Use when you want to find blocks by name (multiple blocks can share the same name)
/// - unit_index_index: Use when you want all blocks in a specific unit
/// - block_kind_index: Use when you want all blocks of a specific kind (e.g., all functions)
/// - block_id_index: Use for O(1) lookup of block metadata by BlockId
///
/// Important: The "name" field is optional since Root blocks and some other blocks may not have names.
///
/// Rationale for data structure choices:
/// - BTreeMap is used for name and unit indexes for better iteration and range queries
/// - HashMap is used for kind index since BlockKind doesn't implement Ord
/// - HashMap is used for block_id_index (direct lookup by BlockId) for O(1) access
/// - Vec is used for values to handle multiple blocks with the same index (same name/kind/unit)
#[derive(Debug, Default, Clone)]
pub struct BlockIndexMaps {
    /// block_name -> Vec<(unit_index, block_kind, block_id)>
    /// Multiple blocks can share the same name across units or within the same unit
    pub block_name_index: BTreeMap<String, Vec<(usize, BlockKind, BlockId)>>,

    /// unit_index -> Vec<(block_name, block_kind, block_id)>
    /// Allows retrieval of all blocks in a specific compilation unit
    pub unit_index_map: BTreeMap<usize, Vec<(Option<String>, BlockKind, BlockId)>>,

    /// block_kind -> Vec<(unit_index, block_name, block_id)>
    /// Allows retrieval of all blocks of a specific kind across all units
    pub block_kind_index: HashMap<BlockKind, Vec<(usize, Option<String>, BlockId)>>,

    /// block_id -> (unit_index, block_name, block_kind)
    /// Direct O(1) lookup of block metadata by ID
    pub block_id_index: HashMap<BlockId, (usize, Option<String>, BlockKind)>,
}

impl BlockIndexMaps {
    /// Create a new empty BlockIndexMaps
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new block in all indexes
    ///
    /// # Arguments
    /// - `block_id`: The unique block identifier
    /// - `block_name`: Optional name of the block (None for unnamed blocks)
    /// - `block_kind`: The kind of block (Func, Class, Stmt, etc.)
    /// - `unit_index`: The compilation unit index this block belongs to
    pub fn insert_block(
        &mut self,
        block_id: BlockId,
        block_name: Option<String>,
        block_kind: BlockKind,
        unit_index: usize,
    ) {
        // Insert into block_id_index for O(1) lookups
        self.block_id_index
            .insert(block_id, (unit_index, block_name.clone(), block_kind));

        // Insert into block_name_index (if name exists)
        if let Some(ref name) = block_name {
            self.block_name_index
                .entry(name.clone())
                .or_default()
                .push((unit_index, block_kind, block_id));
        }

        // Insert into unit_index_map
        self.unit_index_map.entry(unit_index).or_default().push((
            block_name.clone(),
            block_kind,
            block_id,
        ));

        // Insert into block_kind_index
        self.block_kind_index
            .entry(block_kind)
            .or_default()
            .push((unit_index, block_name, block_id));
    }

    /// Find all blocks with a given name (may return multiple blocks)
    ///
    /// Returns a vector of (unit_index, block_kind, block_id) tuples
    pub fn find_by_name(&self, name: &str) -> Vec<(usize, BlockKind, BlockId)> {
        self.block_name_index.get(name).cloned().unwrap_or_default()
    }

    /// Find all blocks in a specific unit
    ///
    /// Returns a vector of (block_name, block_kind, block_id) tuples
    pub fn find_by_unit(&self, unit_index: usize) -> Vec<(Option<String>, BlockKind, BlockId)> {
        self.unit_index_map
            .get(&unit_index)
            .cloned()
            .unwrap_or_default()
    }

    /// Find all blocks of a specific kind across all units
    ///
    /// Returns a vector of (unit_index, block_name, block_id) tuples
    pub fn find_by_kind(&self, block_kind: BlockKind) -> Vec<(usize, Option<String>, BlockId)> {
        self.block_kind_index
            .get(&block_kind)
            .cloned()
            .unwrap_or_default()
    }

    /// Find all blocks of a specific kind in a specific unit
    ///
    /// Returns a vector of block_ids
    pub fn find_by_kind_and_unit(&self, block_kind: BlockKind, unit_index: usize) -> Vec<BlockId> {
        let by_kind = self.find_by_kind(block_kind);
        by_kind
            .into_iter()
            .filter(|(unit, _, _)| *unit == unit_index)
            .map(|(_, _, block_id)| block_id)
            .collect()
    }

    /// Look up block metadata by BlockId for O(1) access
    ///
    /// Returns (unit_index, block_name, block_kind) if found
    pub fn get_block_info(&self, block_id: BlockId) -> Option<(usize, Option<String>, BlockKind)> {
        self.block_id_index.get(&block_id).cloned()
    }

    /// Get total number of blocks indexed
    pub fn block_count(&self) -> usize {
        self.block_id_index.len()
    }

    /// Get the number of unique block names
    pub fn unique_names_count(&self) -> usize {
        self.block_name_index.len()
    }

    /// Check if a block with the given ID exists
    pub fn contains_block(&self, block_id: BlockId) -> bool {
        self.block_id_index.contains_key(&block_id)
    }

    /// Clear all indexes
    pub fn clear(&mut self) {
        self.block_name_index.clear();
        self.unit_index_map.clear();
        self.block_kind_index.clear();
        self.block_id_index.clear();
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
    pub hir_next_id: AtomicU32,
    pub hir_start_ids: RwLock<Vec<Option<HirId>>>,

    // HirId -> ParentedNode
    pub hir_map: RwLock<HashMap<HirId, ParentedNode<'tcx>>>,
    // ScopeId -> Scope (used internally for allocation)
    pub scope_map: RwLock<HashMap<ScopeId, &'tcx Scope<'tcx>>>,
    // HirId -> Scope (direct single-step lookup)
    pub scope_cache: RwLock<HashMap<HirId, &'tcx Scope<'tcx>>>,
    // SymId -> &Symbol
    pub symbol_map: RwLock<HashMap<SymId, &'tcx Symbol>>,

    pub block_arena: Mutex<BlockArena<'tcx>>,
    pub block_next_id: AtomicU32,
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
            .field("hir_next_id", &self.hir_next_id)
            .field("build_metrics", &self.build_metrics)
            .finish()
    }
}

impl<'tcx> CompileCtxt<'tcx> {
    /// Create a new CompileCtxt from source code
    pub fn from_sources<L: LanguageTrait>(sources: &[Vec<u8>]) -> Self {
        let files: Vec<File> = sources
            .iter()
            .enumerate()
            .map(|(index, src)| {
                let path = format!("virtual://unit_{index}.rs");
                File::new_virtual(path, src.clone())
            })
            .collect();
        let (parse_trees, mut metrics) = Self::parse_files_with_metrics::<L>(&files);
        metrics.file_read_seconds = 0.0;
        let count = files.len();
        Self {
            arena: Arena::default(),
            interner: InternPool::default(),
            files,
            parse_trees,
            hir_next_id: AtomicU32::new(0),
            hir_start_ids: RwLock::new(vec![None; count]),
            hir_map: RwLock::new(HashMap::new()),
            scope_map: RwLock::new(HashMap::new()),
            scope_cache: RwLock::new(HashMap::new()),
            symbol_map: RwLock::new(HashMap::new()),
            block_arena: Mutex::new(BlockArena::default()),
            block_next_id: AtomicU32::new(1),
            block_map: RwLock::new(HashMap::new()),
            unresolve_symbols: RwLock::new(Vec::new()),
            related_map: BlockRelationMap::default(),
            block_indexes: RwLock::new(BlockIndexMaps::new()),
            build_metrics: metrics,
        }
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
            hir_next_id: AtomicU32::new(0),
            hir_start_ids: RwLock::new(vec![None; count]),
            hir_map: RwLock::new(HashMap::new()),
            scope_map: RwLock::new(HashMap::new()),
            scope_cache: RwLock::new(HashMap::new()),
            symbol_map: RwLock::new(HashMap::new()),
            block_arena: Mutex::new(BlockArena::default()),
            block_next_id: AtomicU32::new(1),
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
    pub const GLOBAL_SCOPE_OWNER: HirId = HirId(u32::MAX);

    /// Create a context that references this CompileCtxt for a specific file index
    pub fn compile_unit(&'tcx self, index: usize) -> CompileUnit<'tcx> {
        CompileUnit { cc: self, index }
    }

    pub fn create_unit_globals(&'tcx self, owner: HirId) -> &'tcx Scope<'tcx> {
        let scope = self.alloc_scope(owner);
        // self.symbol_map
        //     .write()
        //     .insert(SymId::GLOBAL_SCOPE, scope.as_symbol());
        self.scope_cache.write().insert(owner, scope);
        self.scope_map.write().insert(scope.id(), scope);
        scope
    }

    pub fn create_globals(&'tcx self) -> &'tcx Scope<'tcx> {
        self.create_unit_globals(Self::GLOBAL_SCOPE_OWNER)
    }

    pub fn get_scope(&'tcx self, owner: HirId) -> &'tcx Scope<'tcx> {
        self.scope_cache
            .read()
            .get(&owner)
            .copied()
            .expect("HirId not mapped to Scope in CompileCtxt")
    }

    pub fn opt_get_symbol(&'tcx self, owner: SymId) -> Option<&'tcx Symbol> {
        self.symbol_map.read().get(&owner).cloned()
    }

    /// Find the primary symbol associated with a block ID
    pub fn find_symbol_by_block_id(&'tcx self, block_id: BlockId) -> Option<&'tcx Symbol> {
        self.symbol_map
            .read()
            .values()
            .find(|symbol| symbol.block_id() == Some(block_id))
            .copied()
    }

    /// Access the arena for allocations
    pub fn arena(&'tcx self) -> &'tcx Arena<'tcx> {
        &self.arena
    }

    pub fn alloc_scope(&'tcx self, owner: HirId) -> &'tcx Scope<'tcx> {
        // Check cache first
        if let Some(existing) = self.scope_cache.read().get(&owner) {
            return existing;
        }

        // Allocate new scope
        let scope_id = ScopeId(self.scope_map.read().len() as u32);
        let scope = self.arena.alloc(Scope::new(owner));

        // Update both maps
        self.scope_map.write().insert(scope_id, scope);
        self.scope_cache.write().insert(owner, scope);

        scope
    }

    /// Merge the second scope into the first.
    ///
    /// This combines all symbols from the second scope into the first scope,
    /// and updates both the scope and symbol maps to reference the merged result.
    ///
    /// # Arguments
    /// * `first` - The target scope to merge into
    /// * `second` - The source scope to merge from
    ///
    /// # Side Effects
    /// - All symbols from `second` are merged into `first`
    /// - The symbol_map is updated to point all merged symbols to the symbol_map
    /// - The scope_map is updated to redirect second's scope ID to first
    /// - The scope_cache is updated to redirect second's owner to first
    pub fn merge_two_scopes<'src>(&'tcx self, first: &'tcx Scope<'tcx>, second: &'src Scope<'src>) {
        // Merge symbols from second into first
        first.merge_with(second, self.arena());

        // Update all symbol map entries for symbols now in first scope
        // We need to register the merged symbols in the global symbol_map
        first.for_each_symbol(|sym| {
            self.symbol_map.write().insert(sym.id(), sym);
        });

        // Remap scope references from second to first
        // If any HIR node was mapped to second's scope, it should now map to first
        self.scope_map.write().insert(second.id(), first);
        self.scope_cache.write().insert(second.owner(), first);
    }

    /// Allocate a new scope based on an existing one, cloning its contents.
    ///
    /// The existing ones are from the other arena, and we want to allocate in
    /// this arena.
    pub fn alloc_scope_with<'src>(&'tcx self, existing: &Scope<'src>) -> &'tcx Scope<'tcx> {
        let owner = existing.owner();
        // Check cache first
        if let Some(existing) = self.scope_cache.read().get(&owner) {
            return existing;
        }

        // Allocate new scope from existing
        let scope_id = existing.id();
        let scope = Scope::new_from(existing, self.arena());
        let scope = self.arena.alloc(scope);

        // Update both maps
        self.scope_map.write().insert(scope_id, scope);
        self.scope_cache.write().insert(owner, scope);
        scope.for_each_symbol(|sym| {
            self.symbol_map.write().insert(sym.id(), sym);
        });

        scope
    }

    pub fn reserve_hir_id(&self) -> HirId {
        let id = self.hir_next_id.fetch_add(1, Ordering::Relaxed);
        HirId(id)
    }

    pub fn reserve_block_id(&self) -> BlockId {
        let id = self.block_next_id.fetch_add(1, Ordering::Relaxed);
        BlockId::new(id)
    }

    pub fn current_hir_id(&self) -> HirId {
        HirId(self.hir_next_id.load(Ordering::Relaxed))
    }

    pub fn set_file_start(&self, index: usize, start: HirId) {
        let mut starts = self.hir_start_ids.write();
        if index < starts.len() && starts[index].is_none() {
            starts[index] = Some(start);
        }
    }

    pub fn file_start(&self, index: usize) -> Option<HirId> {
        self.hir_start_ids.read().get(index).and_then(|opt| *opt)
    }

    pub fn file_path(&self, index: usize) -> Option<&str> {
        self.files.get(index).and_then(|file| file.path())
    }

    /// Get the generic parse tree for a specific file
    pub fn get_parse_tree(&self, index: usize) -> Option<&Box<dyn ParseTree>> {
        self.parse_trees.get(index).and_then(|t| t.as_ref())
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
        self.scope_cache.write().clear();
    }
}
