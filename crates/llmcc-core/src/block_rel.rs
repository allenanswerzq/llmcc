use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::block::{BlockId, BlockRelation};

/// Manages relationships between blocks in a clean, type-safe way
#[derive(Debug, Clone, Default)]
pub struct BlockRelationMap {
    /// BlockId -> (Relation -> Vec<BlockId>)
    relations: RefCell<HashMap<BlockId, HashMap<BlockRelation, Vec<BlockId>>>>,
}

impl BlockRelationMap {
    /// Add a relationship between two blocks
    pub fn add_relation_impl(&self, from: BlockId, relation: BlockRelation, to: BlockId) {
        let mut relations = self.relations.borrow_mut();
        relations
            .entry(from)
            .or_default()
            .entry(relation)
            .or_default()
            .push(to);
    }

    /// Add multiple relationships of the same type from one block
    pub fn add_relation_impls(&self, from: BlockId, relation: BlockRelation, targets: &[BlockId]) {
        let mut relations = self.relations.borrow_mut();
        let block_relations = relations.entry(from).or_default();
        let relation_vec = block_relations.entry(relation).or_default();
        relation_vec.extend_from_slice(targets);
    }

    /// Remove a specific relationship
    pub fn remove_relation_impl(
        &self,
        from: BlockId,
        relation: BlockRelation,
        to: BlockId,
    ) -> bool {
        let mut relations = self.relations.borrow_mut();
        if let Some(block_relations) = relations.get_mut(&from) {
            if let Some(targets) = block_relations.get_mut(&relation) {
                if let Some(pos) = targets.iter().position(|&x| x == to) {
                    targets.remove(pos);
                    // Clean up empty vectors
                    if targets.is_empty() {
                        block_relations.remove(&relation);
                        // Clean up empty maps
                        if block_relations.is_empty() {
                            relations.remove(&from);
                        }
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Remove all relationships of a specific type from a block
    pub fn remove_all_relations(&self, from: BlockId, relation: BlockRelation) -> Vec<BlockId> {
        let mut relations = self.relations.borrow_mut();
        if let Some(block_relations) = relations.get_mut(&from) {
            if let Some(targets) = block_relations.remove(&relation) {
                // Clean up empty maps
                if block_relations.is_empty() {
                    relations.remove(&from);
                }
                return targets;
            }
        }
        Vec::new()
    }

    /// Remove all relationships for a block (useful when deleting a block)
    pub fn remove_block_relations(&self, block_id: BlockId) {
        let mut relations = self.relations.borrow_mut();
        relations.remove(&block_id);
    }

    /// Get all blocks related to a given block with a specific relationship
    pub fn get_related(&self, from: BlockId, relation: BlockRelation) -> Vec<BlockId> {
        self.relations
            .borrow()
            .get(&from)
            .and_then(|block_relations| block_relations.get(&relation))
            .cloned()
            .unwrap_or_default()
    }

    /// Get all relationships for a specific block
    pub fn get_all_relations(&self, from: BlockId) -> HashMap<BlockRelation, Vec<BlockId>> {
        self.relations
            .borrow()
            .get(&from)
            .cloned()
            .unwrap_or_default()
    }

    /// Check if a specific relationship exists
    pub fn has_relation(&self, from: BlockId, relation: BlockRelation, to: BlockId) -> bool {
        self.relations
            .borrow()
            .get(&from)
            .and_then(|block_relations| block_relations.get(&relation))
            .map(|targets| targets.contains(&to))
            .unwrap_or(false)
    }

    /// Add a relation if it doesn't already exist (optimized: single borrow)
    pub fn add_relation_if_not_exists(&self, from: BlockId, relation: BlockRelation, to: BlockId) {
        let mut relations = self.relations.borrow_mut();
        let block_relations = relations.entry(from).or_default();
        let targets = block_relations.entry(relation).or_default();
        if !targets.contains(&to) {
            targets.push(to);
        }
    }

    /// Add bidirectional relation if it doesn't already exist (optimized: single borrow)
    pub fn add_bidirectional_if_not_exists(&self, caller: BlockId, callee: BlockId) {
        let mut relations = self.relations.borrow_mut();

        // Add caller -> callee (DependsOn)
        let caller_relations = relations.entry(caller).or_default();
        let caller_targets = caller_relations
            .entry(BlockRelation::DependsOn)
            .or_default();
        if !caller_targets.contains(&callee) {
            caller_targets.push(callee);
        }

        // Add callee -> caller (DependedBy)
        let callee_relations = relations.entry(callee).or_default();
        let callee_targets = callee_relations
            .entry(BlockRelation::DependedBy)
            .or_default();
        if !callee_targets.contains(&caller) {
            callee_targets.push(caller);
        }
    }

    /// Check if any relationship of a type exists
    pub fn has_relation_type(&self, from: BlockId, relation: BlockRelation) -> bool {
        self.relations
            .borrow()
            .get(&from)
            .and_then(|block_relations| block_relations.get(&relation))
            .map(|targets| !targets.is_empty())
            .unwrap_or(false)
    }

    /// Get all blocks that have any relationships
    pub fn get_connected_blocks(&self) -> Vec<BlockId> {
        self.relations.borrow().keys().copied().collect()
    }

    /// Get all blocks related to a given block (regardless of relationship type)
    pub fn get_all_related_blocks(&self, from: BlockId) -> HashSet<BlockId> {
        let mut result = HashSet::new();
        if let Some(block_relations) = self.relations.borrow().get(&from) {
            for targets in block_relations.values() {
                result.extend(targets.iter().copied());
            }
        }
        result
    }

    /// Find all blocks that point to a given block with a specific relationship
    pub fn find_reverse_relations(&self, to: BlockId, relation: BlockRelation) -> Vec<BlockId> {
        let mut result = Vec::new();
        let relations = self.relations.borrow();

        for (&from_block, block_relations) in relations.iter() {
            if let Some(targets) = block_relations.get(&relation) {
                if targets.contains(&to) {
                    result.push(from_block);
                }
            }
        }
        result
    }

    /// Get statistics about relationships
    pub fn stats(&self) -> RelationStats {
        let relations = self.relations.borrow();
        let mut total_relations = 0;
        let mut by_relation: HashMap<BlockRelation, usize> = HashMap::new();

        for block_relations in relations.values() {
            for (&relation, targets) in block_relations.iter() {
                by_relation
                    .entry(relation)
                    .and_modify(|count| *count += targets.len())
                    .or_insert_with(|| targets.len());
                total_relations += targets.len();
            }
        }

        RelationStats {
            total_blocks: relations.len(),
            total_relations,
            by_relation,
        }
    }

    /// Clear all relationships
    pub fn clear(&self) {
        self.relations.borrow_mut().clear();
    }

    /// Check if the map is empty
    pub fn is_empty(&self) -> bool {
        self.relations.borrow().is_empty()
    }

    /// Get the number of blocks with relationships
    pub fn len(&self) -> usize {
        self.relations.borrow().len()
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
            .add_relation_impl(self.from, BlockRelation::DependsOn, to);
        self
    }

    /// Add a "called by" relationship
    pub fn called_by(self, to: BlockId) -> Self {
        self.map
            .add_relation_impl(self.from, BlockRelation::DependedBy, to);
        self
    }

    /// Add a "contains" relationship
    pub fn contains(self, to: BlockId) -> Self {
        self.map
            .add_relation_impl(self.from, BlockRelation::Unknown, to);
        self
    }

    /// Add a "contained by" relationship
    pub fn contained_by(self, to: BlockId) -> Self {
        self.map
            .add_relation_impl(self.from, BlockRelation::Unknown, to);
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
            writeln!(f, "    {}: {}", relation, count)?;
        }
        Ok(())
    }
}

// Convenience functions for common relationship patterns
impl BlockRelationMap {
    /// Create a bidirectional call relationship
    pub fn add_relation(&self, caller: BlockId, callee: BlockId) {
        self.add_relation_impl(caller, BlockRelation::DependsOn, callee);
        self.add_relation_impl(callee, BlockRelation::DependedBy, caller);
    }

    /// Remove a bidirectional call relationship
    pub fn remove_relation(&self, caller: BlockId, callee: BlockId) {
        self.remove_relation_impl(caller, BlockRelation::DependsOn, callee);
        self.remove_relation_impl(callee, BlockRelation::DependedBy, caller);
    }

    pub fn get_depended(&self, block: BlockId) -> Vec<BlockId> {
        self.get_related(block, BlockRelation::DependedBy)
    }

    pub fn get_depends(&self, block: BlockId) -> Vec<BlockId> {
        self.get_related(block, BlockRelation::DependsOn)
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
