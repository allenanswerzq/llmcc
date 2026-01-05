use dashmap::DashMap;
use std::collections::{HashMap, HashSet};

use crate::block::{BlockId, BlockKind, BlockRelation};

/// Manages relationships between blocks in a clean, type-safe way
#[derive(Debug, Default, Clone)]
pub struct BlockRelationMap {
    /// BlockId -> (Relation -> Vec<BlockId>)
    relations: DashMap<BlockId, HashMap<BlockRelation, Vec<BlockId>>>,
}

impl BlockRelationMap {
    /// Add a relationship between two blocks
    pub fn add_relation_impl(&self, from: BlockId, relation: BlockRelation, to: BlockId) {
        self.relations
            .entry(from)
            .or_default()
            .entry(relation)
            .or_default()
            .push(to);
    }

    /// Add multiple relationships of the same type from one block
    pub fn add_relation_impls(&self, from: BlockId, relation: BlockRelation, targets: &[BlockId]) {
        let mut entry = self.relations.entry(from).or_default();
        let relation_vec = entry.entry(relation).or_default();
        relation_vec.extend_from_slice(targets);
    }

    /// Remove a specific relationship
    pub fn remove_relation_impl(
        &self,
        from: BlockId,
        relation: BlockRelation,
        to: BlockId,
    ) -> bool {
        let mut removed = false;
        if let Some(mut block_relations) = self.relations.get_mut(&from) {
            if let Some(targets) = block_relations.get_mut(&relation)
                && let Some(pos) = targets.iter().position(|&x| x == to)
            {
                targets.remove(pos);
                removed = true;
                // Clean up empty vectors
                if targets.is_empty() {
                    block_relations.remove(&relation);
                }
            }
            // Clean up empty maps
            if block_relations.is_empty() {
                drop(block_relations);
                self.relations.remove(&from);
            }
        }
        removed
    }

    /// Remove all relationships of a specific type from a block
    pub fn remove_all_relations(&self, from: BlockId, relation: BlockRelation) -> Vec<BlockId> {
        let mut result = Vec::new();
        if let Some(mut block_relations) = self.relations.get_mut(&from) {
            if let Some(targets) = block_relations.remove(&relation) {
                result = targets;
            }
            // Clean up empty maps
            if block_relations.is_empty() {
                drop(block_relations);
                self.relations.remove(&from);
            }
        }
        result
    }

    /// Remove all relationships for a block (useful when deleting a block)
    pub fn remove_block_relations(&self, block_id: BlockId) {
        self.relations.remove(&block_id);
    }

    /// Get all blocks related to a given block with a specific relationship
    pub fn get_related(&self, from: BlockId, relation: BlockRelation) -> Vec<BlockId> {
        self.relations
            .get(&from)
            .and_then(|block_relations| block_relations.get(&relation).cloned())
            .unwrap_or_default()
    }

    /// Get all relationships for a specific block
    pub fn get_all_relations(&self, from: BlockId) -> HashMap<BlockRelation, Vec<BlockId>> {
        self.relations
            .get(&from)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Check if a specific relationship exists
    pub fn has_relation(&self, from: BlockId, relation: BlockRelation, to: BlockId) -> bool {
        self.relations
            .get(&from)
            .and_then(|block_relations| block_relations.get(&relation).map(|t| t.contains(&to)))
            .unwrap_or(false)
    }

    /// Add a relation if it doesn't already exist (optimized: single borrow)
    pub fn add_relation_if_not_exists(&self, from: BlockId, relation: BlockRelation, to: BlockId) {
        let mut entry = self.relations.entry(from).or_default();
        let targets = entry.entry(relation).or_default();
        if !targets.contains(&to) {
            targets.push(to);
        }
    }

    /// Add bidirectional relation if it doesn't already exist (optimized: single borrow)
    pub fn add_bidirectional_if_not_exists(&self, caller: BlockId, callee: BlockId) {
        // Add caller -> callee (Calls)
        {
            let mut caller_entry = self.relations.entry(caller).or_default();
            let caller_targets = caller_entry.entry(BlockRelation::Calls).or_default();
            if !caller_targets.contains(&callee) {
                caller_targets.push(callee);
            }
        }

        // Add callee -> caller (CalledBy)
        {
            let mut callee_entry = self.relations.entry(callee).or_default();
            let callee_targets = callee_entry.entry(BlockRelation::CalledBy).or_default();
            if !callee_targets.contains(&caller) {
                callee_targets.push(caller);
            }
        }
    }

    /// Check if any relationship of a type exists
    pub fn has_relation_type(&self, from: BlockId, relation: BlockRelation) -> bool {
        self.relations
            .get(&from)
            .and_then(|block_relations| block_relations.get(&relation).map(|t| !t.is_empty()))
            .unwrap_or(false)
    }

    /// Get all blocks that have any relationships
    pub fn get_connected_blocks(&self) -> Vec<BlockId> {
        self.relations.iter().map(|r| *r.key()).collect()
    }

    /// Get all blocks related to a given block (regardless of relationship type)
    pub fn get_all_related_blocks(&self, from: BlockId) -> HashSet<BlockId> {
        let mut result = HashSet::new();
        if let Some(block_relations) = self.relations.get(&from) {
            for targets in block_relations.values() {
                result.extend(targets.iter().copied());
            }
        }
        result
    }

    /// Find all blocks that point to a given block with a specific relationship
    pub fn find_reverse_relations(&self, to: BlockId, relation: BlockRelation) -> Vec<BlockId> {
        let mut result = Vec::new();
        for entry in self.relations.iter() {
            let from_block = *entry.key();
            let block_relations = entry.value();
            if let Some(targets) = block_relations.get(&relation)
                && targets.contains(&to)
            {
                result.push(from_block);
            }
        }
        result
    }

    /// Get statistics about relationships
    pub fn stats(&self) -> RelationStats {
        let mut total_relations = 0;
        let mut by_relation: HashMap<BlockRelation, usize> = HashMap::new();

        for entry in self.relations.iter() {
            let block_relations = entry.value();
            for (&relation, targets) in block_relations.iter() {
                by_relation
                    .entry(relation)
                    .and_modify(|count| *count += targets.len())
                    .or_insert_with(|| targets.len());
                total_relations += targets.len();
            }
        }

        RelationStats {
            total_blocks: self.relations.len(),
            total_relations,
            by_relation,
        }
    }

    /// Clear all relationships
    pub fn clear(&self) {
        self.relations.clear();
    }

    /// Check if the map is empty
    pub fn is_empty(&self) -> bool {
        self.relations.is_empty()
    }

    /// Get the number of blocks with relationships
    pub fn len(&self) -> usize {
        self.relations.len()
    }
}

/// Helper struct for building relationships fluently
pub struct RelationBuilder<'a> {
    map: &'a BlockRelationMap,
    from: BlockId,
}

impl<'a> RelationBuilder<'a> {
    fn new(map: &'a BlockRelationMap, from: BlockId) -> Self {
        Self { map, from }
    }

    /// Add a "calls" relationship
    pub fn calls(self, to: BlockId) -> Self {
        self.map
            .add_relation_impl(self.from, BlockRelation::Calls, to);
        self
    }

    /// Add a "called by" relationship
    pub fn called_by(self, to: BlockId) -> Self {
        self.map
            .add_relation_impl(self.from, BlockRelation::CalledBy, to);
        self
    }

    /// Add a "contains" relationship
    pub fn contains(self, to: BlockId) -> Self {
        self.map
            .add_relation_impl(self.from, BlockRelation::Contains, to);
        self
    }

    /// Add a "contained by" relationship
    pub fn contained_by(self, to: BlockId) -> Self {
        self.map
            .add_relation_impl(self.from, BlockRelation::ContainedBy, to);
        self
    }

    /// Add a custom relationship
    pub fn relation(self, relation: BlockRelation, to: BlockId) -> Self {
        self.map.add_relation_impl(self.from, relation, to);
        self
    }

    /// Add multiple relationships of the same type
    pub fn relations(self, relation: BlockRelation, targets: &[BlockId]) -> Self {
        self.map.add_relation_impls(self.from, relation, targets);
        self
    }
}

impl BlockRelationMap {
    /// Create a fluent builder for adding relationships from a block
    pub fn from_block(&self, from: BlockId) -> RelationBuilder<'_> {
        RelationBuilder::new(self, from)
    }
}

/// Statistics about relationships in the map
#[derive(Debug, Default, Clone)]
pub struct RelationStats {
    pub total_blocks: usize,
    pub total_relations: usize,
    pub by_relation: HashMap<BlockRelation, usize>,
}

impl std::fmt::Display for RelationStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Relation Stats:")?;
        writeln!(f, "  Total blocks with relations: {}", self.total_blocks)?;
        writeln!(f, "  Total relationships: {}", self.total_relations)?;
        writeln!(f, "  By type:")?;
        for (&relation, &count) in &self.by_relation {
            writeln!(f, "    {relation}: {count}")?;
        }
        Ok(())
    }
}

// Convenience functions for common relationship patterns
impl BlockRelationMap {
    /// Create a bidirectional call relationship
    pub fn add_call_relation(&self, caller: BlockId, callee: BlockId) {
        self.add_relation_impl(caller, BlockRelation::Calls, callee);
        self.add_relation_impl(callee, BlockRelation::CalledBy, caller);
    }

    /// Remove a bidirectional call relationship
    pub fn remove_call_relation(&self, caller: BlockId, callee: BlockId) {
        self.remove_relation_impl(caller, BlockRelation::Calls, callee);
        self.remove_relation_impl(callee, BlockRelation::CalledBy, caller);
    }

    pub fn get_callers(&self, block: BlockId) -> Vec<BlockId> {
        self.get_related(block, BlockRelation::CalledBy)
    }

    pub fn get_callees(&self, block: BlockId) -> Vec<BlockId> {
        self.get_related(block, BlockRelation::Calls)
    }

    /// Get all children of a block
    pub fn get_children(&self, block: BlockId) -> Vec<BlockId> {
        self.get_related(block, BlockRelation::Unknown)
    }

    /// Get the parent of a block (assumes single parent)
    pub fn get_parent(&self, block: BlockId) -> Option<BlockId> {
        self.find_reverse_relations(block, BlockRelation::Unknown)
            .into_iter()
            .next()
    }

    /// Get all ancestors of a block (walking up the containment hierarchy)
    pub fn get_ancestors(&self, mut block: BlockId) -> Vec<BlockId> {
        let mut ancestors = Vec::new();
        let mut visited = HashSet::new();

        while let Some(parent) = self.get_parent(block) {
            if visited.contains(&parent) {
                // Cycle detection
                break;
            }
            visited.insert(parent);
            ancestors.push(parent);
            block = parent;
        }

        ancestors
    }

    /// Get all descendants of a block (walking down the containment hierarchy)
    pub fn get_descendants(&self, block: BlockId) -> Vec<BlockId> {
        let mut descendants = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = vec![block];

        while let Some(current) = queue.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            let children = self.get_children(current);
            descendants.extend(&children);
            queue.extend(children);
        }

        descendants
    }
}

/// BlockIndexMaps provides efficient lookup of blocks by various indices.
///
/// Best practices for usage:
/// - block_name_index: Use when you want to find blocks by name (multiple blocks can share the same name)
/// - unit_index_map: Use when you want all blocks in a specific unit
/// - block_kind_index: Use when you want all blocks of a specific kind (e.g., all functions)
/// - block_id_index: Use for O(1) lookup of block metadata by BlockId
///
/// Important: The "name" field is optional since Root blocks and some other blocks may not have names.
///
/// Rationale for data structure choices:
/// - DashMap is used for all indexes to allow concurrent access during parallel graph building
/// - Vec is used for values to handle multiple blocks with the same index (same name/kind/unit)
pub struct BlockIndexMaps {
    /// block_name -> Vec<(unit_index, block_kind, block_id)>
    /// Multiple blocks can share the same name across units or within the same unit
    block_name_index: DashMap<String, Vec<(usize, BlockKind, BlockId)>>,

    /// unit_index -> Vec<(block_name, block_kind, block_id)>
    /// Allows retrieval of all blocks in a specific compilation unit
    unit_index_map: DashMap<usize, Vec<(Option<String>, BlockKind, BlockId)>>,

    /// block_kind -> Vec<(unit_index, block_name, block_id)>
    /// Allows retrieval of all blocks of a specific kind across all units
    block_kind_index: DashMap<BlockKind, Vec<(usize, Option<String>, BlockId)>>,

    /// block_id -> (unit_index, block_name, block_kind)
    /// Direct O(1) lookup of block metadata by ID
    block_id_index: DashMap<BlockId, (usize, Option<String>, BlockKind)>,
}

impl Default for BlockIndexMaps {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockIndexMaps {
    /// Create a new empty BlockIndexMaps
    pub fn new() -> Self {
        Self {
            block_name_index: DashMap::new(),
            unit_index_map: DashMap::new(),
            block_kind_index: DashMap::new(),
            block_id_index: DashMap::new(),
        }
    }

    /// Register a new block in all indexes
    ///
    /// # Arguments
    /// - `block_id`: The unique block identifier
    /// - `block_name`: Optional name of the block (None for unnamed blocks)
    /// - `block_kind`: The kind of block (Func, Class, Stmt, etc.)
    /// - `unit_index`: The compilation unit index this block belongs to
    pub fn insert_block(
        &self,
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
        self.block_name_index
            .get(name)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Find all blocks in a specific unit
    ///
    /// Returns a vector of (block_name, block_kind, block_id) tuples
    pub fn find_by_unit(&self, unit_index: usize) -> Vec<(Option<String>, BlockKind, BlockId)> {
        self.unit_index_map
            .get(&unit_index)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Find all blocks of a specific kind across all units
    ///
    /// Returns a vector of (unit_index, block_name, block_id) tuples
    pub fn find_by_kind(&self, block_kind: BlockKind) -> Vec<(usize, Option<String>, BlockId)> {
        self.block_kind_index
            .get(&block_kind)
            .map(|v| v.clone())
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
        self.block_id_index.get(&block_id).map(|v| v.clone())
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

    /// Get an iterator over all blocks with their metadata
    /// Returns (block_id, unit_index, block_name, block_kind)
    pub fn iter_all_blocks(&self) -> Vec<(BlockId, usize, Option<String>, BlockKind)> {
        self.block_id_index
            .iter()
            .map(|entry| {
                let block_id = *entry.key();
                let (unit_index, name, kind) = entry.value();
                (block_id, *unit_index, name.clone(), *kind)
            })
            .collect()
    }

    /// Clear all indexes
    pub fn clear(&self) {
        self.block_name_index.clear();
        self.unit_index_map.clear();
        self.block_kind_index.clear();
        self.block_id_index.clear();
    }
}
