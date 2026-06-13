//! Directed block relations and block metadata indexes.

use dashmap::DashMap;
use std::collections::{HashMap, HashSet};

use crate::block::{BlockId, BlockKind, BlockRelation};

/// Concurrent map of directed, de-duplicated relations between blocks.
#[derive(Debug, Default, Clone)]
pub struct BlockRelationMap {
    /// Source block -> relation kind -> target blocks.
    relations: DashMap<BlockId, HashMap<BlockRelation, HashSet<BlockId>>>,
}

/// Outgoing targets for one relation type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRelationEntry {
    pub relation: BlockRelation,
    pub targets: Vec<BlockId>,
}

impl BlockRelationEntry {
    fn new(relation: BlockRelation, targets: impl IntoIterator<Item = BlockId>) -> Self {
        Self {
            relation,
            targets: sorted_block_ids(targets),
        }
    }
}

fn sorted_block_ids(ids: impl IntoIterator<Item = BlockId>) -> Vec<BlockId> {
    let mut ids: Vec<_> = ids.into_iter().collect();
    ids.sort();
    ids
}

impl BlockRelationMap {
    /// Insert a directed relation. Returns `true` when it was newly inserted.
    pub fn insert(&self, from: BlockId, relation: BlockRelation, to: BlockId) -> bool {
        let mut entry = self.relations.entry(from).or_default();
        let targets = entry.entry(relation).or_default();
        targets.insert(to)
    }

    /// Insert a relation and its inverse, when the relation has one.
    pub fn insert_pair(&self, from: BlockId, relation: BlockRelation, to: BlockId) -> bool {
        let inserted = self.insert(from, relation, to);
        if let Some(inverse) = relation.inverse() {
            self.insert(to, inverse, from);
        }
        inserted
    }

    /// Insert multiple directed relations of the same type.
    pub fn extend(&self, from: BlockId, relation: BlockRelation, targets: &[BlockId]) -> usize {
        targets
            .iter()
            .filter(|&&target| self.insert(from, relation, target))
            .count()
    }

    /// Remove one directed relation.
    pub fn remove(&self, from: BlockId, relation: BlockRelation, to: BlockId) -> bool {
        let mut removed = false;
        if let Some(mut block_relations) = self.relations.get_mut(&from) {
            if let Some(targets) = block_relations.get_mut(&relation)
                && targets.remove(&to)
            {
                removed = true;
                // Clean up empty target sets
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

    /// Remove all outgoing relations of one type from a block.
    pub fn remove_relation_kind(&self, from: BlockId, relation: BlockRelation) -> Vec<BlockId> {
        let mut result = Vec::new();
        if let Some(mut block_relations) = self.relations.get_mut(&from) {
            if let Some(targets) = block_relations.remove(&relation) {
                result = sorted_block_ids(targets);
            }
            // Clean up empty maps
            if block_relations.is_empty() {
                drop(block_relations);
                self.relations.remove(&from);
            }
        }
        result
    }

    /// Remove all outgoing relations for a block.
    pub fn remove_block(&self, block_id: BlockId) {
        self.relations.remove(&block_id);
    }

    /// Return outgoing targets for one relation type.
    pub fn related(&self, from: BlockId, relation: BlockRelation) -> Vec<BlockId> {
        self.relations
            .get(&from)
            .and_then(|block_relations| {
                block_relations
                    .get(&relation)
                    .map(|targets| sorted_block_ids(targets.iter().copied()))
            })
            .unwrap_or_default()
    }

    /// Return all outgoing relations from a block.
    pub fn relations_from(&self, from: BlockId) -> Vec<BlockRelationEntry> {
        let mut entries: Vec<_> = self
            .relations
            .get(&from)
            .map(|relations| {
                relations
                    .iter()
                    .map(|(&relation, targets)| {
                        BlockRelationEntry::new(relation, targets.iter().copied())
                    })
                    .collect()
            })
            .unwrap_or_default();
        entries.sort_by_key(|entry: &BlockRelationEntry| entry.relation as usize);
        entries
    }

    /// Return whether a specific relation exists.
    pub fn contains(&self, from: BlockId, relation: BlockRelation, to: BlockId) -> bool {
        self.relations
            .get(&from)
            .and_then(|block_relations| block_relations.get(&relation).map(|t| t.contains(&to)))
            .unwrap_or(false)
    }

    /// Return whether any outgoing relation of this type exists.
    pub fn contains_relation(&self, from: BlockId, relation: BlockRelation) -> bool {
        self.relations
            .get(&from)
            .and_then(|block_relations| block_relations.get(&relation).map(|t| !t.is_empty()))
            .unwrap_or(false)
    }

    /// Return all blocks that have outgoing relations.
    pub fn blocks(&self) -> Vec<BlockId> {
        sorted_block_ids(self.relations.iter().map(|r| *r.key()))
    }

    /// Return all outgoing targets regardless of relation type.
    pub fn related_blocks(&self, from: BlockId) -> Vec<BlockId> {
        let mut result = HashSet::new();
        if let Some(block_relations) = self.relations.get(&from) {
            for targets in block_relations.values() {
                result.extend(targets.iter().copied());
            }
        }
        sorted_block_ids(result)
    }

    /// Return all sources that point to `to` with `relation`.
    pub fn reverse_related(&self, to: BlockId, relation: BlockRelation) -> Vec<BlockId> {
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
        sorted_block_ids(result)
    }

    /// Return aggregate relation statistics.
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

    /// Remove all relations.
    pub fn clear(&self) {
        self.relations.clear();
    }

    /// Return whether the map has no relations.
    pub fn is_empty(&self) -> bool {
        self.relations.is_empty()
    }

    /// Return the number of blocks with outgoing relations.
    pub fn len(&self) -> usize {
        self.relations.len()
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
        let mut by_relation: Vec<_> = self.by_relation.iter().collect();
        by_relation.sort_by_key(|(relation, _)| relation.to_string());
        for (&relation, &count) in by_relation {
            writeln!(f, "    {relation}: {count}")?;
        }
        Ok(())
    }
}

impl BlockRelationMap {
    /// Insert bidirectional call/called-by relations.
    pub fn insert_call(&self, caller: BlockId, callee: BlockId) {
        self.insert_pair(caller, BlockRelation::Calls, callee);
    }

    /// Remove bidirectional call/called-by relations.
    pub fn remove_call(&self, caller: BlockId, callee: BlockId) {
        self.remove(caller, BlockRelation::Calls, callee);
        self.remove(callee, BlockRelation::CalledBy, caller);
    }
}

/// Metadata stored for each indexed block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockIndexEntry {
    pub block_id: BlockId,
    pub unit_index: usize,
    pub name: Option<String>,
    pub kind: BlockKind,
}

impl BlockIndexEntry {
    pub fn new(
        block_id: BlockId,
        unit_index: usize,
        name: Option<String>,
        kind: BlockKind,
    ) -> Self {
        Self {
            block_id,
            unit_index,
            name,
            kind,
        }
    }

    fn sort_key(&self) -> (usize, u32) {
        (self.unit_index, self.block_id.as_u32())
    }
}

fn sorted_entries(mut entries: Vec<BlockIndexEntry>) -> Vec<BlockIndexEntry> {
    entries.sort_by_key(BlockIndexEntry::sort_key);
    entries
}

/// Concurrent indexes for block metadata.
///
/// Names are optional because root and synthetic blocks may not have a stable
/// source name. Query methods return deterministic vectors sorted by unit and
/// block id.
pub struct BlockIndexMaps {
    /// block_name -> entries
    /// Multiple blocks can share the same name across units or within the same unit
    block_name_index: DashMap<String, Vec<BlockIndexEntry>>,

    /// unit_index -> entries
    /// Allows retrieval of all blocks in a specific compilation unit
    unit_index_map: DashMap<usize, Vec<BlockIndexEntry>>,

    /// block_kind -> entries
    /// Allows retrieval of all blocks of a specific kind across all units
    block_kind_index: DashMap<BlockKind, Vec<BlockIndexEntry>>,

    /// block_id -> entry
    /// Direct O(1) lookup of block metadata by ID
    block_id_index: DashMap<BlockId, BlockIndexEntry>,
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

    /// Register a block in all indexes.
    pub fn insert_block(
        &self,
        block_id: BlockId,
        block_name: Option<String>,
        block_kind: BlockKind,
        unit_index: usize,
    ) {
        let entry = BlockIndexEntry::new(block_id, unit_index, block_name, block_kind);

        self.block_id_index.insert(block_id, entry.clone());

        // Insert into block_name_index (if name exists)
        if let Some(ref name) = entry.name {
            self.block_name_index
                .entry(name.clone())
                .or_default()
                .push(entry.clone());
        }

        // Insert into unit_index_map
        self.unit_index_map
            .entry(unit_index)
            .or_default()
            .push(entry.clone());

        // Insert into block_kind_index
        self.block_kind_index
            .entry(block_kind)
            .or_default()
            .push(entry);
    }

    /// Return blocks with the given name.
    pub fn by_name(&self, name: &str) -> Vec<BlockIndexEntry> {
        sorted_entries(
            self.block_name_index
                .get(name)
                .map(|v| v.clone())
                .unwrap_or_default(),
        )
    }

    /// Return blocks in one compilation unit.
    pub fn by_unit(&self, unit_index: usize) -> Vec<BlockIndexEntry> {
        sorted_entries(
            self.unit_index_map
                .get(&unit_index)
                .map(|v| v.clone())
                .unwrap_or_default(),
        )
    }

    /// Return blocks with the given kind.
    pub fn by_kind(&self, block_kind: BlockKind) -> Vec<BlockIndexEntry> {
        sorted_entries(
            self.block_kind_index
                .get(&block_kind)
                .map(|v| v.clone())
                .unwrap_or_default(),
        )
    }

    /// Return block ids for one kind in one compilation unit.
    pub fn by_kind_in_unit(&self, block_kind: BlockKind, unit_index: usize) -> Vec<BlockId> {
        let by_kind = self.by_kind(block_kind);
        by_kind
            .into_iter()
            .filter(|entry| entry.unit_index == unit_index)
            .map(|entry| entry.block_id)
            .collect()
    }

    /// Return metadata for one block id.
    pub fn block_info(&self, block_id: BlockId) -> Option<BlockIndexEntry> {
        self.block_id_index.get(&block_id).map(|v| v.clone())
    }

    /// Return the total number of indexed blocks.
    pub fn block_count(&self) -> usize {
        self.block_id_index.len()
    }

    /// Return the number of unique indexed block names.
    pub fn unique_names_count(&self) -> usize {
        self.block_name_index.len()
    }

    /// Return whether a block id is indexed.
    pub fn contains_block(&self, block_id: BlockId) -> bool {
        self.block_id_index.contains_key(&block_id)
    }

    /// Return all blocks with their indexed metadata.
    pub fn blocks(&self) -> Vec<BlockIndexEntry> {
        sorted_entries(
            self.block_id_index
                .iter()
                .map(|entry| entry.value().clone())
                .collect(),
        )
    }

    /// Clear all indexes.
    pub fn clear(&self) {
        self.block_name_index.clear();
        self.unit_index_map.clear();
        self.block_kind_index.clear();
        self.block_id_index.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_insert_is_unique_and_removal_cleans_empty_entries() {
        let relations = BlockRelationMap::default();
        let caller = BlockId::new(1);
        let callee = BlockId::new(2);

        assert!(relations.insert(caller, BlockRelation::Calls, callee));
        assert!(!relations.insert(caller, BlockRelation::Calls, callee));
        assert_eq!(
            relations.related(caller, BlockRelation::Calls),
            vec![callee]
        );
        assert!(relations.contains(caller, BlockRelation::Calls, callee));

        assert!(relations.remove(caller, BlockRelation::Calls, callee));
        assert!(!relations.remove(caller, BlockRelation::Calls, callee));
        assert!(relations.is_empty());
    }

    #[test]
    fn relation_extend_counts_new_targets_only() {
        let relations = BlockRelationMap::default();
        let from = BlockId::new(1);
        let target_a = BlockId::new(2);
        let target_b = BlockId::new(3);

        assert_eq!(
            relations.extend(from, BlockRelation::Uses, &[target_a, target_a, target_b]),
            2
        );
        assert_eq!(
            relations.related(from, BlockRelation::Uses),
            vec![target_a, target_b]
        );
    }

    #[test]
    fn relation_queries_are_sorted() {
        let relations = BlockRelationMap::default();
        let from = BlockId::new(1);
        let target_a = BlockId::new(2);
        let target_b = BlockId::new(3);

        relations.insert(from, BlockRelation::Uses, target_b);
        relations.insert(from, BlockRelation::Calls, target_a);
        relations.insert(from, BlockRelation::Uses, target_a);

        assert_eq!(
            relations.related(from, BlockRelation::Uses),
            vec![target_a, target_b]
        );
        assert_eq!(
            relations.relations_from(from),
            vec![
                BlockRelationEntry::new(BlockRelation::Calls, [target_a]),
                BlockRelationEntry::new(BlockRelation::Uses, [target_a, target_b]),
            ]
        );
    }

    #[test]
    fn block_indexes_query_registered_blocks() {
        let indexes = BlockIndexMaps::new();
        let func = BlockId::new(1);
        let class = BlockId::new(2);

        indexes.insert_block(func, Some("run".to_string()), BlockKind::Func, 0);
        indexes.insert_block(class, Some("User".to_string()), BlockKind::Class, 1);

        assert_eq!(
            indexes.by_name("run"),
            vec![BlockIndexEntry::new(
                func,
                0,
                Some("run".to_string()),
                BlockKind::Func
            )]
        );
        assert_eq!(indexes.by_kind_in_unit(BlockKind::Class, 1), vec![class]);
        assert_eq!(
            indexes.block_info(class),
            Some(BlockIndexEntry::new(
                class,
                1,
                Some("User".to_string()),
                BlockKind::Class
            ))
        );
        assert!(indexes.contains_block(func));
        assert_eq!(indexes.block_count(), 2);
    }
}
