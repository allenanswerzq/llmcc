use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Deref;
use tree_sitter::Tree;

use crate::block::{Arena as BlockArena, BasicBlock, BlockId};
use crate::block_rel::BlockRelationMap;
use crate::file::File;
use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirId, HirNode};
use crate::lang_def::LanguageTrait;
use crate::symbol::{Scope, Symbol};

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
            .bb_map
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

    /// Get an existing scope or None if it doesn't exist
    pub fn get_scope(self, owner: HirId) -> &'tcx Scope<'tcx> {
        self.cc.scope_map.borrow().get(&owner).copied().unwrap()
    }

    /// Create a new symbol in the arena
    pub fn new_symbol(self, owner: HirId, name: String) -> &'tcx Symbol {
        let key = self.cc.interner.intern(&name);
        self.cc.arena.alloc(Symbol::new(owner, name, key))
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

    pub block_arena: BlockArena<'tcx>,
    pub block_next_id: Cell<u32>,
    // BlockId -> ParentedBlock
    pub bb_map: RefCell<HashMap<BlockId, ParentedBlock<'tcx>>>,
    // BlockId -> RelatedBlock
    pub related_map: BlockRelationMap,
}

impl<'tcx> CompileCtxt<'tcx> {
    /// Create a new CompileCtxt from source code
    pub fn from_sources<L: LanguageTrait>(sources: &[Vec<u8>]) -> Self {
        let files: Vec<File> = sources
            .iter()
            .map(|src| File::new_source(src.clone()))
            .collect();
        let trees = sources.iter().map(|src| L::parse(src)).collect();
        let count = files.len();
        Self {
            arena: Arena::default(),
            interner: InternPool::default(),
            files,
            trees,
            hir_next_id: Cell::new(1),
            hir_start_ids: RefCell::new(vec![None; count]),
            hir_map: RefCell::new(HashMap::new()),
            scope_map: RefCell::new(HashMap::new()),
            block_arena: BlockArena::default(),
            block_next_id: Cell::new(0),
            bb_map: RefCell::new(HashMap::new()),
            related_map: BlockRelationMap::default(),
        }
    }

    /// Create a new CompileCtxt from files
    pub fn from_files<L: LanguageTrait>(paths: &[String]) -> std::io::Result<Self> {
        let mut files = Vec::new();
        let mut trees = Vec::new();
        for path in paths {
            let file = File::new_file(path.clone())?;
            trees.push(L::parse(file.content()));
            files.push(file);
        }
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
            block_arena: BlockArena::default(),
            block_next_id: Cell::new(0),
            bb_map: RefCell::new(HashMap::new()),
            related_map: BlockRelationMap::default(),
        })
    }

    /// Create a context that references this CompileCtxt for a specific file index
    pub fn compile_unit(&'tcx self, index: usize) -> CompileUnit<'tcx> {
        CompileUnit { cc: self, index }
    }

    pub fn create_globals(&'tcx self) -> &'tcx Scope<'tcx> {
        self.alloc_scope(HirId(0))
    }

    pub fn create_graph(&'tcx self) -> ProjectGraph<'tcx> {
        ProjectGraph::new()
    }

    pub fn get_scope(&'tcx self, owner: HirId) -> &'tcx Scope<'tcx> {
        self.scope_map.borrow().get(&owner).unwrap()
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

    /// Clear all maps (useful for testing)
    #[cfg(test)]
    pub fn clear(&self) {
        self.hir_map.borrow_mut().clear();
        self.scope_map.borrow_mut().clear();
    }
}
