use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use tree_sitter::Tree;

use crate::block::{Arena as BlockArena, BasicBlock, BlockId, BlockRelation};
use crate::block_rel::{BlockRelationMap, RelationBuilder};
use crate::file::File;
use crate::ir::{Arena, HirId, HirKind, HirNode};
use crate::lang_def::LanguageTrait;
use crate::symbol::{Scope, ScopeStack, SymId, Symbol};

#[derive(Debug, Copy, Clone)]
pub struct Context<'tcx> {
    pub gcx: &'tcx GlobalCtxt<'tcx>,
}

impl<'tcx> Context<'tcx> {
    /// Get a HIR node by ID, returning None if not found
    pub fn opt_hir_node(self, id: HirId) -> Option<HirNode<'tcx>> {
        self.gcx
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
        self.gcx
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
        self.gcx
            .hir_map
            .borrow()
            .get(&id)
            .and_then(|parented| parented.parent())
    }

    /// Get a symbol from the uses map
    pub fn opt_uses(self, id: HirId) -> Option<&'tcx Symbol<'tcx>> {
        self.gcx.uses_map.borrow().get(&id).copied()
    }

    /// Get a symbol from the defs map
    pub fn opt_defs(self, id: HirId) -> Option<&'tcx Symbol<'tcx>> {
        self.gcx.defs_map.borrow().get(&id).copied()
    }

    /// Get a symbol from the defs map
    pub fn defs(self, id: HirId) -> &'tcx Symbol<'tcx> {
        self.gcx
            .defs_map
            .borrow()
            .get(&id)
            .copied()
            .unwrap_or_else(|| panic!("no defs: {}", id))
    }

    /// Get an existing scope or None if it doesn't exist
    pub fn opt_scope(self, owner: HirId) -> Option<&'tcx Scope<'tcx>> {
        self.gcx.scope_map.borrow().get(&owner).copied()
    }

    /// Create a new symbol in the arena
    pub fn new_symbol(self, owner: HirId, name: String) -> &'tcx Symbol<'tcx> {
        self.gcx.arena.alloc(Symbol::new(owner, name))
    }

    /// Find an existing scope or create a new one
    pub fn alloc_scope(self, owner: HirId) -> &'tcx Scope<'tcx> {
        // Check if scope already exists
        if let Some(existing_scope) = self.opt_scope(owner) {
            return existing_scope;
        }

        // Create new scope
        let scope = self.gcx.arena.alloc(Scope::new(owner));
        self.gcx.scope_map.borrow_mut().insert(owner, scope);
        scope
    }

    /// Add a HIR node to the map
    pub fn insert_hir_node(self, id: HirId, node: HirNode<'tcx>) {
        let parented = ParentedNode::new(node);
        self.gcx.hir_map.borrow_mut().insert(id, parented);
    }

    /// Add a symbol definition
    pub fn insert_def(self, id: HirId, symbol: &'tcx Symbol<'tcx>) {
        self.gcx.defs_map.borrow_mut().insert(id, symbol);
    }

    /// Add a symbol use
    pub fn insert_use(self, id: HirId, symbol: &'tcx Symbol<'tcx>) {
        self.gcx.uses_map.borrow_mut().insert(id, symbol);
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

impl<'tcx> Deref for Context<'tcx> {
    type Target = GlobalCtxt<'tcx>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.gcx
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
pub struct GlobalCtxt<'tcx> {
    pub arena: Arena<'tcx>,
    pub file: File,
    pub tree: Option<Tree>,

    // HirId -> ParentedNode
    pub hir_map: RefCell<HashMap<HirId, ParentedNode<'tcx>>>,
    // HirId -> &Symbol (definitions)
    pub defs_map: RefCell<HashMap<HirId, &'tcx Symbol<'tcx>>>,
    // HirId -> &Symbol (uses/references)
    pub uses_map: RefCell<HashMap<HirId, &'tcx Symbol<'tcx>>>,
    // HirId -> &Scope (scopes owned by this HIR node)
    pub scope_map: RefCell<HashMap<HirId, &'tcx Scope<'tcx>>>,

    pub block_arena: BlockArena<'tcx>,
    // BlockId -> ParentedBlock
    pub bb_map: RefCell<HashMap<BlockId, ParentedBlock<'tcx>>>,
    // BlockId -> RelatedBlock
    pub related_map: BlockRelationMap,
}

impl<'tcx> GlobalCtxt<'tcx> {
    /// Create a new GlobalCtxt from source code
    pub fn from_source<L: LanguageTrait>(source: &[u8]) -> Self {
        Self {
            arena: Arena::default(),
            file: File::new_source(source.to_vec()),
            tree: L::parse(source),
            ..Default::default()
        }
    }

    /// Create a new GlobalCtxt from file
    pub fn from_file<L: LanguageTrait>(file: String) -> std::io::Result<Self> {
        let file = File::new_file(file)?;
        let tree = L::parse(file.content());
        Ok(Self {
            arena: Arena::default(),
            file,
            tree,
            ..Default::default()
        })
    }

    pub fn tree(&'tcx self) -> Tree {
        self.tree.as_ref().unwrap().clone()
    }

    /// Create a context that references this GlobalCtxt
    pub fn create_context(&'tcx self) -> Context<'tcx> {
        Context { gcx: self }
    }

    /// Get statistics about the maps
    pub fn stats(&self) -> GlobalCtxtStats {
        GlobalCtxtStats {
            hir_nodes: self.hir_map.borrow().len(),
            definitions: self.defs_map.borrow().len(),
            uses: self.uses_map.borrow().len(),
            scopes: self.scope_map.borrow().len(),
        }
    }

    /// Clear all maps (useful for testing)
    #[cfg(test)]
    pub fn clear(&self) {
        self.hir_map.borrow_mut().clear();
        self.defs_map.borrow_mut().clear();
        self.uses_map.borrow_mut().clear();
        self.scope_map.borrow_mut().clear();
    }
}

/// Statistics about GlobalCtxt contents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalCtxtStats {
    pub hir_nodes: usize,
    pub definitions: usize,
    pub uses: usize,
    pub scopes: usize,
}

impl std::fmt::Display for GlobalCtxtStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GlobalCtxt Stats: {} HIR nodes, {} definitions, {} uses, {} scopes",
            self.hir_nodes, self.definitions, self.uses, self.scopes
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct BlockStats {
    pub total: usize,
    pub roots: usize,
    pub functions: usize,
    pub classes: usize,
    pub impls: usize,
    pub undefined: usize,
}

impl std::fmt::Display for BlockStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Block Stats: {} total ({} roots, {} functions, {} classes, {} impls, {} undefined)",
            self.total, self.roots, self.functions, self.classes, self.impls, self.undefined
        )
    }
}
