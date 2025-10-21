use rayon::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::ops::Deref;
use std::path::Path;
use tree_sitter::Tree;

use crate::block::{Arena as BlockArena, BasicBlock, BlockId, BlockKind};
use crate::block_rel::BlockRelationMap;
use crate::file::File;
use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirId, HirNode};
use crate::lang_def::LanguageTrait;
use crate::symbol::{Scope, SymId, Symbol};

#[derive(Debug, Copy, Clone)]
pub struct CompileUnit<'tcx> {
    pub cc: &'tcx CompileCtxt<'tcx>,
    pub index: usize,
}

impl<'tcx> CompileUnit<'tcx> {
    pub fn file(&self) -> &'tcx File {
        &self.cc.files[self.index]
    }

    pub fn tree(&self) -> &'tcx Tree {
        &self.cc.trees[self.index].as_ref().unwrap()
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

    /// Get a HIR node by ID, returning None if not found
    pub fn opt_hir_node(self, id: HirId) -> Option<HirNode<'tcx>> {
        self.cc
            .hir_map
            .borrow()
            .get(&id)
            .map(|parented| parented.node.clone())
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
            .borrow()
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
            .borrow()
            .get(&id)
            .and_then(|parented| parented.parent())
    }

    /// Get an existing scope or None if it doesn't exist
    pub fn opt_get_scope(self, owner: HirId) -> Option<&'tcx Scope<'tcx>> {
        self.cc.scope_map.borrow().get(&owner).copied()
    }

    pub fn opt_get_symbol(self, owner: SymId) -> Option<&'tcx Symbol> {
        self.cc.symbol_map.borrow().get(&owner).copied()
    }

    /// Get an existing scope or None if it doesn't exist
    pub fn get_scope(self, owner: HirId) -> &'tcx Scope<'tcx> {
        self.cc.scope_map.borrow().get(&owner).copied().unwrap()
    }

    /// Find an existing scope or create a new one
    pub fn alloc_scope(self, owner: HirId) -> &'tcx Scope<'tcx> {
        self.cc.alloc_scope(owner)
    }

    /// Add a HIR node to the map
    pub fn insert_hir_node(self, id: HirId, node: HirNode<'tcx>) {
        let parented = ParentedNode::new(node);
        self.cc.hir_map.borrow_mut().insert(id, parented);
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
        self.cc.unresolve_symbols.borrow_mut().push(symbol);
    }

    pub fn insert_block(&self, id: BlockId, block: BasicBlock<'tcx>, parent: BlockId) {
        let parented = ParentedBlock::new(parent, block.clone());
        self.cc.block_map.borrow_mut().insert(id, parented);

        // Register the block in the index maps
        let block_kind = block.kind();
        let block_name = block
            .base()
            .and_then(|base| base.opt_get_name())
            .map(|s| s.to_string());

        self.cc
            .block_indexes
            .borrow_mut()
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
                .or_insert_with(Vec::new)
                .push((unit_index, block_kind, block_id));
        }

        // Insert into unit_index_map
        self.unit_index_map
            .entry(unit_index)
            .or_insert_with(Vec::new)
            .push((block_name.clone(), block_kind, block_id));

        // Insert into block_kind_index
        self.block_kind_index
            .entry(block_kind)
            .or_insert_with(Vec::new)
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

#[derive(Debug, Default)]
pub struct CompileCtxt<'tcx> {
    pub arena: Arena<'tcx>,
    pub interner: InternPool,
    pub files: Vec<File>,
    pub trees: Vec<Option<Tree>>,
    pub hir_next_id: Cell<u32>,
    pub hir_start_ids: RefCell<Vec<Option<HirId>>>,

    // HirId -> ParentedNode
    pub hir_map: RefCell<HashMap<HirId, ParentedNode<'tcx>>>,
    // HirId -> &Scope (scopes owned by this HIR node)
    pub scope_map: RefCell<HashMap<HirId, &'tcx Scope<'tcx>>>,
    // SymId -> &Symbol
    pub symbol_map: RefCell<HashMap<SymId, &'tcx Symbol>>,

    pub block_arena: BlockArena<'tcx>,
    pub block_next_id: Cell<u32>,
    // BlockId -> ParentedBlock
    pub block_map: RefCell<HashMap<BlockId, ParentedBlock<'tcx>>>,
    pub unresolve_symbols: RefCell<Vec<&'tcx Symbol>>,
    pub related_map: BlockRelationMap,

    /// Index maps for efficient block lookups by name, kind, unit, and id
    pub block_indexes: RefCell<BlockIndexMaps>,
}

impl<'tcx> CompileCtxt<'tcx> {
    /// Create a new CompileCtxt from source code
    pub fn from_sources<L: LanguageTrait>(sources: &[Vec<u8>]) -> Self {
        let files: Vec<File> = sources
            .iter()
            .map(|src| File::new_source(src.clone()))
            .collect();
        let trees = sources.par_iter().map(|src| L::parse(src)).collect();
        let count = files.len();
        Self {
            arena: Arena::default(),
            interner: InternPool::default(),
            files,
            trees,
            hir_next_id: Cell::new(0),
            hir_start_ids: RefCell::new(vec![None; count]),
            hir_map: RefCell::new(HashMap::new()),
            scope_map: RefCell::new(HashMap::new()),
            symbol_map: RefCell::new(HashMap::new()),
            block_arena: BlockArena::default(),
            block_next_id: Cell::new(0),
            block_map: RefCell::new(HashMap::new()),
            unresolve_symbols: RefCell::new(Vec::new()),
            related_map: BlockRelationMap::default(),
            block_indexes: RefCell::new(BlockIndexMaps::new()),
        }
    }

    /// Create a new CompileCtxt from files
    pub fn from_files<L: LanguageTrait>(paths: &[String]) -> std::io::Result<Self> {
        let mut files = Vec::new();
        for path in paths {
            files.push(File::new_file(path.clone())?);
        }

        let trees: Vec<_> = files
            .par_iter()
            .map(|file| L::parse(file.content()))
            .collect();

        let count = files.len();
        Ok(Self {
            arena: Arena::default(),
            interner: InternPool::default(),
            files,
            trees,
            hir_next_id: Cell::new(0),
            hir_start_ids: RefCell::new(vec![None; count]),
            hir_map: RefCell::new(HashMap::new()),
            scope_map: RefCell::new(HashMap::new()),
            symbol_map: RefCell::new(HashMap::new()),
            block_arena: BlockArena::default(),
            block_next_id: Cell::new(0),
            block_map: RefCell::new(HashMap::new()),
            unresolve_symbols: RefCell::new(Vec::new()),
            related_map: BlockRelationMap::default(),
            block_indexes: RefCell::new(BlockIndexMaps::new()),
        })
    }

    /// Create a new CompileCtxt from a directory, recursively finding all *.rs files
    pub fn from_dir<P: AsRef<Path>, L: LanguageTrait>(dir: P) -> std::io::Result<Self> {
        let mut files = Vec::new();

        let walker = ignore::WalkBuilder::new(dir.as_ref())
            .standard_filters(true)
            .build();

        for entry in walker {
            let entry: ignore::DirEntry = entry.map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to walk directory: {}", e),
                )
            })?;
            let path = entry.path();

            if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                if let Ok(file) = File::new_file(path.to_string_lossy().to_string()) {
                    files.push(file);
                }
            }
        }

        let trees: Vec<_> = files
            .par_iter()
            .map(|file| L::parse(file.content()))
            .collect();

        let count = files.len();
        Ok(Self {
            arena: Arena::default(),
            interner: InternPool::default(),
            files,
            trees,
            hir_next_id: Cell::new(0),
            hir_start_ids: RefCell::new(vec![None; count]),
            hir_map: RefCell::new(HashMap::new()),
            scope_map: RefCell::new(HashMap::new()),
            symbol_map: RefCell::new(HashMap::new()),
            block_arena: BlockArena::default(),
            block_next_id: Cell::new(0),
            block_map: RefCell::new(HashMap::new()),
            unresolve_symbols: RefCell::new(Vec::new()),
            related_map: BlockRelationMap::default(),
            block_indexes: RefCell::new(BlockIndexMaps::new()),
        })
    }

    /// Create a context that references this CompileCtxt for a specific file index
    pub fn compile_unit(&'tcx self, index: usize) -> CompileUnit<'tcx> {
        CompileUnit { cc: self, index }
    }

    pub fn create_globals(&'tcx self) -> &'tcx Scope<'tcx> {
        self.alloc_scope(HirId(0))
    }

    pub fn get_scope(&'tcx self, owner: HirId) -> &'tcx Scope<'tcx> {
        self.scope_map.borrow().get(&owner).unwrap()
    }

    pub fn opt_get_symbol(&'tcx self, owner: SymId) -> Option<&'tcx Symbol> {
        self.symbol_map.borrow().get(&owner).cloned()
    }

    /// Find the primary symbol associated with a block ID
    pub fn find_symbol_by_block_id(&'tcx self, block_id: BlockId) -> Option<&'tcx Symbol> {
        self.symbol_map
            .borrow()
            .values()
            .find(|symbol| symbol.block_id() == Some(block_id))
            .copied()
    }

    pub fn alloc_scope(&'tcx self, owner: HirId) -> &'tcx Scope<'tcx> {
        if let Some(existing) = self.scope_map.borrow().get(&owner) {
            return existing;
        }

        let scope = self.arena.alloc(Scope::new(owner));
        self.scope_map.borrow_mut().insert(owner, scope);
        scope
    }

    pub fn reserve_hir_id(&self) -> HirId {
        let id = self.hir_next_id.get();
        self.hir_next_id.set(id + 1);
        HirId(id)
    }

    pub fn reserve_block_id(&self) -> BlockId {
        let id = self.block_next_id.get();
        self.block_next_id.set(id + 1);
        BlockId::new(id)
    }

    pub fn current_hir_id(&self) -> HirId {
        HirId(self.hir_next_id.get())
    }

    pub fn set_file_start(&self, index: usize, start: HirId) {
        let mut starts = self.hir_start_ids.borrow_mut();
        if index < starts.len() && starts[index].is_none() {
            starts[index] = Some(start);
        }
    }

    pub fn file_start(&self, index: usize) -> Option<HirId> {
        self.hir_start_ids.borrow().get(index).and_then(|opt| *opt)
    }

    pub fn file_path(&self, index: usize) -> Option<&str> {
        self.files.get(index).and_then(|file| file.path())
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
        self.hir_map.borrow_mut().clear();
        self.scope_map.borrow_mut().clear();
    }
}
