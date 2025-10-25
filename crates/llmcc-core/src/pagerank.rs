use std::collections::HashMap;

use crate::block::{BlockId, BlockKind, BlockRelation};
use crate::graph_builder::{GraphNode, ProjectGraph};

/// Configuration options for the PageRank algorithm.
#[derive(Debug, Clone)]
pub struct PageRankConfig {
    /// Damping factor used when random walkers jump to a random node.
    pub damping_factor: f64,
    /// Maximum number of iterations to run the algorithm.
    pub max_iterations: usize,
    /// Convergence tolerance (L1 delta between successive rank vectors).
    pub tolerance: f64,
    /// Which edge relation to consider when traversing the graph.
    pub relation: BlockRelation,
}

impl Default for PageRankConfig {
    fn default() -> Self {
        Self {
            damping_factor: 0.85,
            max_iterations: 100,
            tolerance: 1e-6,
            relation: BlockRelation::DependsOn,
        }
    }
}

/// Result entry produced by PageRank containing metadata about the block.
#[derive(Debug, Clone)]
pub struct RankedBlock {
    pub node: GraphNode,
    pub score: f64,
    pub name: String,
    pub kind: BlockKind,
    pub file_path: Option<String>,
}

/// Computes PageRank scores over a [`ProjectGraph`].
#[derive(Debug)]
pub struct PageRanker<'graph, 'tcx> {
    graph: &'graph ProjectGraph<'tcx>,
    config: PageRankConfig,
}

impl<'graph, 'tcx> PageRanker<'graph, 'tcx> {
    /// Create a new PageRanker using the default configuration.
    pub fn new(graph: &'graph ProjectGraph<'tcx>) -> Self {
        Self {
            graph,
            config: PageRankConfig::default(),
        }
    }

    /// Create a new PageRanker with a custom configuration.
    pub fn with_config(graph: &'graph ProjectGraph<'tcx>, config: PageRankConfig) -> Self {
        Self { graph, config }
    }

    /// Borrow the current configuration.
    pub fn config(&self) -> &PageRankConfig {
        &self.config
    }

    /// Mutably borrow the current configuration for in-place adjustments.
    pub fn config_mut(&mut self) -> &mut PageRankConfig {
        &mut self.config
    }

    /// Compute PageRank scores for all blocks in the project and return them in descending order.
    pub fn rank(&self) -> Vec<RankedBlock> {
        let entries = self.collect_entries();
        let total_nodes = entries.len();
        if total_nodes == 0 {
            return Vec::new();
        }

        let outgoing = self.build_adjacency(&entries);
        let mut ranks = vec![1.0 / total_nodes as f64; total_nodes];
        let mut next_ranks = vec![0.0; total_nodes];
        let damping = self.config.damping_factor;
        let teleport = (1.0 - damping) / total_nodes as f64;

        for _ in 0..self.config.max_iterations {
            next_ranks.fill(teleport);

            let mut sink_mass = 0.0;
            for (idx, neighbours) in outgoing.iter().enumerate() {
                if neighbours.is_empty() {
                    sink_mass += ranks[idx];
                    continue;
                }

                let share = ranks[idx] * damping / neighbours.len() as f64;
                for &target_idx in neighbours {
                    next_ranks[target_idx] += share;
                }
            }

            if sink_mass > 0.0 {
                let redistributed = sink_mass * damping / total_nodes as f64;
                for value in &mut next_ranks {
                    *value += redistributed;
                }
            }

            let delta: f64 = next_ranks
                .iter()
                .zip(&ranks)
                .map(|(new_score, old_score)| (new_score - old_score).abs())
                .sum();

            ranks.copy_from_slice(&next_ranks);

            if delta < self.config.tolerance {
                break;
            }
        }

        let mut ranked: Vec<RankedBlock> = entries
            .into_iter()
            .enumerate()
            .map(|(idx, entry)| {
                let BlockEntry {
                    block_id,
                    unit_index,
                    name,
                    kind,
                    file_path,
                } = entry;

                let display_name = name
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("{}:{}", kind, block_id.as_u32()));

                RankedBlock {
                    node: GraphNode {
                        unit_index,
                        block_id,
                    },
                    score: ranks[idx],
                    name: display_name,
                    kind,
                    file_path,
                }
            })
            .collect();

        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked
    }

    /// Convenience helper returning only the top `k` ranked blocks.
    pub fn top_k(&self, k: usize) -> Vec<RankedBlock> {
        let mut ranked = self.rank();
        if ranked.len() > k {
            ranked.truncate(k);
        }
        ranked
    }

    /// Convenience helper returning blocks whose score is above the provided threshold.
    pub fn above_threshold(&self, threshold: f64) -> Vec<RankedBlock> {
        self.rank()
            .into_iter()
            .filter(|block| block.score >= threshold)
            .collect()
    }

    fn collect_entries(&self) -> Vec<BlockEntry> {
        let mut raw_entries: Vec<(BlockId, usize, Option<String>, BlockKind)> = {
            let indexes = self.graph.cc.block_indexes.borrow();
            indexes
                .block_id_index
                .iter()
                .map(|(&block_id, (unit_index, name, kind))| {
                    (block_id, *unit_index, name.clone(), *kind)
                })
                .collect()
        };

        raw_entries.sort_by_key(|(block_id, ..)| block_id.as_u32());

        raw_entries
            .into_iter()
            .map(|(block_id, unit_index, name, kind)| {
                let file_path = self
                    .graph
                    .cc
                    .files
                    .get(unit_index)
                    .and_then(|file| file.path().map(|path| path.to_string()));

                BlockEntry {
                    block_id,
                    unit_index,
                    name,
                    kind,
                    file_path,
                }
            })
            .collect()
    }

    fn build_adjacency(&self, entries: &[BlockEntry]) -> Vec<Vec<usize>> {
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); entries.len()];
        let mut index_by_block: HashMap<BlockId, usize> = HashMap::new();

        for (idx, entry) in entries.iter().enumerate() {
            index_by_block.insert(entry.block_id, idx);
        }

        for (idx, entry) in entries.iter().enumerate() {
            let Some(unit_graph) = self.graph.unit_graph(entry.unit_index) else {
                continue;
            };

            let mut targets = unit_graph
                .edges()
                .get_related(entry.block_id, self.config.relation)
                .into_iter()
                .filter_map(|dep_id| index_by_block.get(&dep_id).copied())
                .filter(|target_idx| *target_idx != idx)
                .collect::<Vec<_>>();

            targets.sort_unstable();
            targets.dedup();
            adjacency[idx] = targets;
        }

        adjacency
    }
}

#[derive(Debug, Clone)]
struct BlockEntry {
    block_id: BlockId,
    unit_index: usize,
    name: Option<String>,
    kind: BlockKind,
    file_path: Option<String>,
}
