use std::collections::HashMap;

use crate::block::{BlockId, BlockKind, BlockRelation};
use crate::graph_builder::{GraphNode, ProjectGraph};

/// Configuration options for PageRank algorithm.
#[derive(Debug, Clone)]
pub struct PageRankConfig {
    /// Damping factor (typically 0.85).
    pub damping_factor: f64,
    /// Maximum number of iterations.
    pub max_iterations: usize,
    /// Convergence tolerance.
    pub tolerance: f64,
    /// Edge relation to follow.
    pub relation: BlockRelation,
}

impl Default for PageRankConfig {
    fn default() -> Self {
        Self {
            damping_factor: 0.85,
            max_iterations: 100,
            tolerance: 1e-6,
            relation: BlockRelation::DependedBy,
        }
    }
}

/// Result from a PageRank computation.
#[derive(Debug, Clone)]
pub struct RankedBlock {
    pub node: GraphNode,
    pub score: f64,
    pub name: String,
    pub kind: BlockKind,
    pub file_path: Option<String>,
}

/// Result from ranking computation.
#[derive(Debug)]
pub struct RankingResult {
    pub blocks: Vec<RankedBlock>,
    pub iterations: usize,
    pub converged: bool,
}

impl IntoIterator for RankingResult {
    type Item = RankedBlock;
    type IntoIter = std::vec::IntoIter<RankedBlock>;

    fn into_iter(self) -> Self::IntoIter {
        self.blocks.into_iter()
    }
}

/// Computes PageRank scores over a [`ProjectGraph`].
#[derive(Debug)]
pub struct PageRanker<'graph, 'tcx> {
    graph: &'graph ProjectGraph<'tcx>,
    config: PageRankConfig,
}

impl<'graph, 'tcx> PageRanker<'graph, 'tcx> {
    /// Create a new PageRanker with default configuration.
    pub fn new(graph: &'graph ProjectGraph<'tcx>) -> Self {
        Self {
            graph,
            config: PageRankConfig::default(),
        }
    }

    /// Create a new PageRanker with custom configuration.
    pub fn with_config(graph: &'graph ProjectGraph<'tcx>, config: PageRankConfig) -> Self {
        Self { graph, config }
    }

    /// Compute PageRank and return results sorted by score (highest first).
    pub fn rank(&self) -> RankingResult {
        let entries = self.collect_entries();

        if entries.is_empty() {
            return RankingResult {
                blocks: Vec::new(),
                iterations: 0,
                converged: true,
            };
        }

        let adjacency = self.build_adjacency(&entries);

        // Compute PageRank
        let (scores, iterations, converged) = self.compute_pagerank(&adjacency);

        // Build ranked results
        let mut ranked: Vec<RankedBlock> = entries
            .into_iter()
            .enumerate()
            .map(|(idx, entry)| {
                let display_name = entry
                    .name
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("{}:{}", entry.kind, entry.block_id.as_u32()));

                RankedBlock {
                    node: GraphNode {
                        unit_index: entry.unit_index,
                        block_id: entry.block_id,
                    },
                    score: scores[idx],
                    name: display_name,
                    kind: entry.kind,
                    file_path: entry.file_path,
                }
            })
            .collect();

        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        RankingResult {
            blocks: ranked,
            iterations,
            converged,
        }
    }

    /// Get top k blocks by PageRank score.
    pub fn top_k(&self, k: usize) -> Vec<RankedBlock> {
        self.rank().blocks.into_iter().take(k).collect()
    }

    fn compute_pagerank(&self, adjacency: &[Vec<usize>]) -> (Vec<f64>, usize, bool) {
        let n = adjacency.len();
        let mut ranks = vec![1.0 / n as f64; n];
        let mut next_ranks = vec![0.0; n];
        let damping = self.config.damping_factor;
        let teleport = (1.0 - damping) / n as f64;

        let mut iterations = 0;
        let mut converged = false;

        for iter in 0..self.config.max_iterations {
            iterations = iter + 1;
            next_ranks.fill(teleport);

            let mut sink_mass = 0.0;
            for (idx, neighbors) in adjacency.iter().enumerate() {
                if neighbors.is_empty() {
                    sink_mass += ranks[idx];
                } else {
                    let share = ranks[idx] * damping / neighbors.len() as f64;
                    for &target_idx in neighbors {
                        next_ranks[target_idx] += share;
                    }
                }
            }

            if sink_mass > 0.0 {
                let redistributed = sink_mass * damping / n as f64;
                for value in &mut next_ranks {
                    *value += redistributed;
                }
            }

            let delta: f64 = next_ranks
                .iter()
                .zip(&ranks)
                .map(|(new, old)| (new - old).abs())
                .sum();

            ranks.copy_from_slice(&next_ranks);

            if delta < self.config.tolerance {
                converged = true;
                break;
            }
        }

        (ranks, iterations, converged)
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
