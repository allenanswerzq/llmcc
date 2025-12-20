use std::collections::HashSet;

use crate::block::{BlockId, BlockKind, BlockRelation};
use crate::block_rel::BlockRelationMap;
use crate::context::CompileCtxt;

#[derive(Debug, Clone)]
pub struct UnitGraph {
    /// Compile unit this graph belongs to
    unit_index: usize,
    /// Root block ID of this unit
    root: BlockId,
    /// Edge of this graph unit
    edges: BlockRelationMap,
}

impl UnitGraph {
    pub fn new(unit_index: usize, root: BlockId, edges: BlockRelationMap) -> Self {
        Self {
            unit_index,
            root,
            edges,
        }
    }

    pub fn unit_index(&self) -> usize {
        self.unit_index
    }

    pub fn root(&self) -> BlockId {
        self.root
    }

    pub fn edges(&self) -> &BlockRelationMap {
        &self.edges
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitNode {
    pub unit_index: usize,
    pub block_id: BlockId,
}

/// ProjectGraph represents a complete compilation project with all units and their inter-dependencies.
#[derive(Debug)]
pub struct ProjectGraph<'tcx> {
    /// Reference to the compilation context containing all symbols, HIR nodes, and blocks
    pub cc: &'tcx CompileCtxt<'tcx>,
    /// Per-unit graphs containing blocks and intra-unit relations
    units: Vec<UnitGraph>,
    /// Component grouping depth for graph visualization
    component_depth: usize,
}

impl<'tcx> ProjectGraph<'tcx> {
    pub fn new(cc: &'tcx CompileCtxt<'tcx>) -> Self {
        Self {
            cc,
            units: Vec::new(),
            component_depth: 2, // Default to top-level modules
        }
    }

    /// Set the component depth for graph visualization
    pub fn set_component_depth(&mut self, depth: usize) {
        self.component_depth = depth;
    }

    /// Get the component depth for graph visualization
    pub fn component_depth(&self) -> usize {
        self.component_depth
    }

    pub fn add_child(&mut self, graph: UnitGraph) {
        self.units.push(graph);
    }

    /// Add multiple unit graphs to the project graph.
    pub fn add_children(&mut self, graphs: Vec<UnitGraph>) {
        self.units.extend(graphs);
    }

    pub fn connect_blocks(&mut self) {
    }

    pub fn units(&self) -> &[UnitGraph] {
        &self.units
    }

    pub fn unit_graph(&self, unit_index: usize) -> Option<&UnitGraph> {
        self.units
            .iter()
            .find(|unit| unit.unit_index() == unit_index)
    }

    pub fn block_by_name(&self, name: &str) -> Option<UnitNode> {
        let matches = self.cc.find_blocks_by_name(name);

        matches.first().map(|(unit_index, _, block_id)| UnitNode {
            unit_index: *unit_index,
            block_id: *block_id,
        })
    }

    pub fn blocks_by_name(&self, name: &str) -> Vec<UnitNode> {
        let matches = self.cc.find_blocks_by_name(name);

        matches
            .into_iter()
            .map(|(unit_index, _, block_id)| UnitNode {
                unit_index,
                block_id,
            })
            .collect()
    }

    pub fn block_by_name_in(&self, unit_index: usize, name: &str) -> Option<UnitNode> {
        let matches = self.cc.find_blocks_by_name(name);

        matches
            .iter()
            .find(|(u, _, _)| *u == unit_index)
            .map(|(_, _, block_id)| UnitNode {
                unit_index,
                block_id: *block_id,
            })
    }

    pub fn blocks_by_kind(&self, block_kind: BlockKind) -> Vec<UnitNode> {
        let matches = self.cc.find_blocks_by_kind(block_kind);

        matches
            .into_iter()
            .map(|(unit_index, _, block_id)| UnitNode {
                unit_index,
                block_id,
            })
            .collect()
    }

    pub fn blocks_by_kind_in(&self, block_kind: BlockKind, unit_index: usize) -> Vec<UnitNode> {
        let block_ids = self.cc.find_blocks_by_kind_in_unit(block_kind, unit_index);

        block_ids
            .into_iter()
            .map(|block_id| UnitNode {
                unit_index,
                block_id,
            })
            .collect()
    }

    pub fn blocks_in(&self, unit_index: usize) -> Vec<UnitNode> {
        let matches = self.cc.find_blocks_in_unit(unit_index);

        matches
            .into_iter()
            .map(|(_, _, block_id)| UnitNode {
                unit_index,
                block_id,
            })
            .collect()
    }

    pub fn block_info(&self, block_id: BlockId) -> Option<(usize, Option<String>, BlockKind)> {
        self.cc.get_block_info(block_id)
    }

    pub fn find_related_blocks(
        &self,
        node: UnitNode,
        relations: Vec<BlockRelation>,
    ) -> Vec<UnitNode> {
        if node.unit_index >= self.units.len() {
            return Vec::new();
        }

        let unit = &self.units[node.unit_index];
        let mut result = Vec::new();

        for relation in relations {
            match relation {
                BlockRelation::Calls => {
                    let dependencies = unit.edges.get_related(node.block_id, BlockRelation::Calls);
                    for dep_block_id in dependencies {
                        let dep_unit_index = self
                            .cc
                            .get_block_info(dep_block_id)
                            .map(|(idx, _, _)| idx)
                            .unwrap_or(node.unit_index);
                        result.push(UnitNode {
                            unit_index: dep_unit_index,
                            block_id: dep_block_id,
                        });
                    }
                }
                BlockRelation::CalledBy => {
                    let mut seen = HashSet::new();

                    let dependents = unit
                        .edges
                        .get_related(node.block_id, BlockRelation::CalledBy);
                    if !dependents.is_empty() {
                        for dep_block_id in dependents {
                            if !seen.insert(dep_block_id) {
                                continue;
                            }
                            if let Some((dep_unit_idx, _, _)) = self.cc.get_block_info(dep_block_id)
                            {
                                result.push(UnitNode {
                                    unit_index: dep_unit_idx,
                                    block_id: dep_block_id,
                                });
                            } else {
                                result.push(UnitNode {
                                    unit_index: node.unit_index,
                                    block_id: dep_block_id,
                                });
                            }
                        }
                    }

                    let local_dependents = unit
                        .edges
                        .find_reverse_relations(node.block_id, BlockRelation::Calls);
                    for dep_block_id in local_dependents {
                        if !seen.insert(dep_block_id) {
                            continue;
                        }
                        result.push(UnitNode {
                            unit_index: node.unit_index,
                            block_id: dep_block_id,
                        });
                    }
                }
                BlockRelation::Unknown => {}
                // Handle other relations generically
                _ => {
                    let related = unit.edges.get_related(node.block_id, relation);
                    for related_block_id in related {
                        let related_unit_index = self
                            .cc
                            .get_block_info(related_block_id)
                            .map(|(idx, _, _)| idx)
                            .unwrap_or(node.unit_index);
                        result.push(UnitNode {
                            unit_index: related_unit_index,
                            block_id: related_block_id,
                        });
                    }
                }
            }
        }

        result
    }

    pub fn find_dpends_blocks_recursive(&self, node: UnitNode) -> HashSet<UnitNode> {
        let mut visited = HashSet::new();
        let mut stack = vec![node];
        let relations = vec![BlockRelation::Calls];

        while let Some(current) = stack.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            for related in self.find_related_blocks(current, relations.clone()) {
                if !visited.contains(&related) {
                    stack.push(related);
                }
            }
        }

        visited.remove(&node);
        visited
    }

    pub fn find_depended_blocks_recursive(&self, node: UnitNode) -> HashSet<UnitNode> {
        let mut visited = HashSet::new();
        let mut stack = vec![node];
        let relations = vec![BlockRelation::CalledBy];

        while let Some(current) = stack.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            for related in self.find_related_blocks(current, relations.clone()) {
                if !visited.contains(&related) {
                    stack.push(related);
                }
            }
        }

        visited.remove(&node);
        visited
    }

    pub fn traverse_bfs<F>(&self, start: UnitNode, mut callback: F)
    where
        F: FnMut(UnitNode),
    {
        let mut visited = HashSet::new();
        let mut queue = vec![start];
        let relations = vec![BlockRelation::Calls, BlockRelation::CalledBy];

        while !queue.is_empty() {
            let current = queue.remove(0);
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);
            callback(current);

            for related in self.find_related_blocks(current, relations.clone()) {
                if !visited.contains(&related) {
                    queue.push(related);
                }
            }
        }
    }

    pub fn traverse_dfs<F>(&self, start: UnitNode, mut callback: F)
    where
        F: FnMut(UnitNode),
    {
        let mut visited = HashSet::new();
        self.traverse_dfs_impl(start, &mut visited, &mut callback);
    }

    fn traverse_dfs_impl<F>(
        &self,
        node: UnitNode,
        visited: &mut HashSet<UnitNode>,
        callback: &mut F,
    ) where
        F: FnMut(UnitNode),
    {
        if visited.contains(&node) {
            return;
        }
        visited.insert(node);
        callback(node);

        let relations = vec![BlockRelation::Calls, BlockRelation::CalledBy];
        for related in self.find_related_blocks(node, relations) {
            if !visited.contains(&related) {
                self.traverse_dfs_impl(related, visited, callback);
            }
        }
    }

    pub fn get_block_depends(&self, node: UnitNode) -> HashSet<UnitNode> {
        if node.unit_index >= self.units.len() {
            return HashSet::new();
        }

        let unit = &self.units[node.unit_index];
        let mut result = HashSet::new();
        let mut visited = HashSet::new();
        let mut stack = vec![node.block_id];

        while let Some(current_block) = stack.pop() {
            if visited.contains(&current_block) {
                continue;
            }
            visited.insert(current_block);

            let dependencies = unit.edges.get_related(current_block, BlockRelation::Calls);
            for dep_block_id in dependencies {
                if dep_block_id != node.block_id {
                    let dep_unit_index = self
                        .cc
                        .get_block_info(dep_block_id)
                        .map(|(idx, _, _)| idx)
                        .unwrap_or(node.unit_index);
                    result.insert(UnitNode {
                        unit_index: dep_unit_index,
                        block_id: dep_block_id,
                    });
                    stack.push(dep_block_id);
                }
            }
        }

        result
    }

    pub fn get_block_depended(&self, node: UnitNode) -> HashSet<UnitNode> {
        if node.unit_index >= self.units.len() {
            return HashSet::new();
        }

        let unit = &self.units[node.unit_index];
        let mut result = HashSet::new();
        let mut visited = HashSet::new();
        let mut stack = vec![node.block_id];

        while let Some(current_block) = stack.pop() {
            if visited.contains(&current_block) {
                continue;
            }
            visited.insert(current_block);

            let dependencies = unit
                .edges
                .get_related(current_block, BlockRelation::CalledBy);
            for dep_block_id in dependencies {
                if dep_block_id != node.block_id {
                    let dep_unit_index = self
                        .cc
                        .get_block_info(dep_block_id)
                        .map(|(idx, _, _)| idx)
                        .unwrap_or(node.unit_index);
                    result.insert(UnitNode {
                        unit_index: dep_unit_index,
                        block_id: dep_block_id,
                    });
                    stack.push(dep_block_id);
                }
            }
        }

        result
    }
}
