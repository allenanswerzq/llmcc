//! PageRank-based node importance ranking.
//! PageRank-based node importance ranking.
use std::collections::HashMap;

use crate::block::{BlockId, BlockKind, BlockRelation};
use crate::graph::{ProjectGraph, UnitNode};

/// Configuration options for PageRank algorithm.
#[derive(Debug, Clone)]
pub struct PageRankConfig {
    /// Damping factor (typically 0.85).
    pub damping_factor: f64,
    /// Maximum number of iterations.
    pub max_iterations: usize,
    /// Convergence tolerance.
    pub tolerance: f64,
    /// Weight assigned to PageRank computed over `DependsOn` edges (foundational influence).
    pub influence_weight: f64,
    /// Weight assigned to PageRank computed over `DependedBy` edges (orchestration influence).
    pub orchestration_weight: f64,
}

impl Default for PageRankConfig {
    fn default() -> Self {
        Self {
            damping_factor: 0.85,
            max_iterations: 100,
            tolerance: 1e-6,
            influence_weight: 0.2,
            orchestration_weight: 0.8,
        }
    }
}

/// Result from a PageRank computation.
#[derive(Debug, Clone)]
pub struct RankedBlock {
    pub node: UnitNode,
    /// Blended score based on the configured influence/orchestration weights.
    pub score: f64,
    /// PageRank following `DependsOn` edges – highlights foundational building blocks.
    pub influence_score: f64,
    /// PageRank following `DependedBy` edges – highlights orchestrators/entry points.
    pub orchestration_score: f64,
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
    ///
    /// Uses a unified graph built from multiple edge types:
    /// - **Influence edges** (A→B means B is foundational to A):
    ///   - Calls: function calls another function
    ///   - TypeOf: field/param/return uses a type
    ///   - Implements: type implements a trait
    /// - **Orchestration edges** (A→B means A orchestrates/uses B):
    ///   - CalledBy: function is called by another
    ///   - TypeFor: type is used by field/param/return
    ///   - ImplementedBy: trait is implemented by types
    pub fn rank(&self) -> RankingResult {
        let entries = self.collect_entries();

        if entries.is_empty() {
            return RankingResult {
                blocks: Vec::new(),
                iterations: 0,
                converged: true,
            };
        }

        // Build unified adjacency graphs from multiple edge types
        let adjacency_influence = self.build_unified_adjacency(
            &entries,
            &[
                BlockRelation::Calls,      // func → func it calls
                BlockRelation::TypeOf,     // field/param → type definition
                BlockRelation::Implements, // type → trait it implements
                BlockRelation::Uses,       // generic usage
            ],
        );
        let adjacency_orchestration = self.build_unified_adjacency(
            &entries,
            &[
                BlockRelation::CalledBy,      // func ← func that calls it
                BlockRelation::TypeFor,       // type ← field/param that uses it
                BlockRelation::ImplementedBy, // trait ← types that implement it
                BlockRelation::UsedBy,        // generic usage
            ],
        );

        // Compute PageRank for both directions
        let (influence_scores, influence_iters, influence_converged) =
            self.compute_pagerank(&adjacency_influence);
        let (orchestration_scores, orchestration_iters, orchestration_converged) =
            self.compute_pagerank(&adjacency_orchestration);

        let total_weight = self.config.influence_weight + self.config.orchestration_weight;
        let (influence_weight, orchestration_weight) = if total_weight <= f64::EPSILON {
            (0.5, 0.5)
        } else {
            (
                self.config.influence_weight / total_weight,
                self.config.orchestration_weight / total_weight,
            )
        };

        let blended_scores: Vec<f64> = influence_scores
            .iter()
            .zip(&orchestration_scores)
            .map(|(influence, orchestration)| {
                influence * influence_weight + orchestration * orchestration_weight
            })
            .collect();

        let iterations = influence_iters.max(orchestration_iters);
        let converged = influence_converged && orchestration_converged;

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
                    node: UnitNode {
                        unit_index: entry.unit_index,
                        block_id: entry.block_id,
                    },
                    score: blended_scores[idx],
                    influence_score: influence_scores[idx],
                    orchestration_score: orchestration_scores[idx],
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

    /// Get all PageRank scores as a HashMap from BlockId to score.
    /// Useful for aggregating scores by component (crate/module/file).
    pub fn scores(&self) -> HashMap<BlockId, f64> {
        self.rank()
            .blocks
            .into_iter()
            .map(|rb| (rb.node.block_id, rb.score))
            .collect()
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
        let mut raw_entries: Vec<(BlockId, usize, Option<String>, BlockKind)> =
            self.graph.cc.get_all_blocks();

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

    /// Build unified adjacency from multiple relation types.
    fn build_unified_adjacency(
        &self,
        entries: &[BlockEntry],
        relations: &[BlockRelation],
    ) -> Vec<Vec<usize>> {
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); entries.len()];
        let mut index_by_block: HashMap<BlockId, usize> = HashMap::new();

        for (idx, entry) in entries.iter().enumerate() {
            index_by_block.insert(entry.block_id, idx);
        }

        for (idx, entry) in entries.iter().enumerate() {
            let mut all_targets = Vec::new();

            for &relation in relations {
                let targets = self
                    .graph
                    .cc
                    .related_map
                    .get_related(entry.block_id, relation);
                all_targets.extend(targets);
            }

            let mut filtered: Vec<usize> = all_targets
                .into_iter()
                .filter_map(|dep_id| index_by_block.get(&dep_id).copied())
                .filter(|target_idx| *target_idx != idx)
                .collect();

            filtered.sort_unstable();
            filtered.dedup();
            adjacency[idx] = filtered;
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
