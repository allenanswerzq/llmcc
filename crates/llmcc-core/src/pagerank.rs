//! PageRank-based node importance ranking (weighted + biased variant).

use std::collections::HashMap;

use crate::block::{BlockId, BlockKind, BlockRelation};
use crate::graph::{ProjectGraph, UnitNode};

/// Configuration options for the weighted PageRank algorithm.
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
    /// Penalty applied when an edge crosses file boundaries. 1.0 means no penalty.
    pub cross_file_penalty: f64,
    /// Optional cap on outgoing edges per node (keeps top weights only).
    pub max_out_degree: Option<usize>,
    /// Blend between uniform teleport (0.0) and kind-prior teleport (1.0).
    pub teleport_prior_strength: f64,
    /// Edge weights per relation category.
    pub edge_weights: EdgeWeights,
    /// Per-kind priors used for teleport biasing.
    pub kind_priors: Vec<(BlockKind, f64)>,
}

impl Default for PageRankConfig {
    fn default() -> Self {
        Self {
            damping_factor: 0.85,
            max_iterations: 100,
            tolerance: 1e-6,
            influence_weight: 0.25,
            orchestration_weight: 0.75,
            cross_file_penalty: 0.8,
            max_out_degree: Some(150),
            teleport_prior_strength: 0.12,
            edge_weights: EdgeWeights::default(),
            kind_priors: vec![
                (BlockKind::Root, 0.2),
                (BlockKind::Module, 0.6),
                (BlockKind::Class, 1.0),
                (BlockKind::Trait, 1.0),
                (BlockKind::Interface, 1.0),
                (BlockKind::Func, 1.0),
                (BlockKind::Method, 1.0),
                (BlockKind::Impl, 0.8),
                (BlockKind::Enum, 0.8),
                (BlockKind::Alias, 0.6),
                (BlockKind::Const, 0.4),
                (BlockKind::Field, 0.3),
                (BlockKind::Parameter, 0.2),
                (BlockKind::Return, 0.2),
                (BlockKind::Call, 0.1),
                (BlockKind::Scope, 0.1),
                (BlockKind::Undefined, 0.05),
            ],
        }
    }
}

/// Relation weights for influence/orchestration graphs.
#[derive(Debug, Clone)]
pub struct EdgeWeights {
    pub influence: Vec<(BlockRelation, f64)>,
    pub orchestration: Vec<(BlockRelation, f64)>,
}

impl Default for EdgeWeights {
    fn default() -> Self {
        Self {
            influence: vec![
                (BlockRelation::Calls, 1.0),
                (BlockRelation::TypeOf, 1.0),
                (BlockRelation::Implements, 0.8),
                (BlockRelation::Uses, 0.35),
            ],
            orchestration: vec![
                (BlockRelation::CalledBy, 1.0),
                (BlockRelation::TypeFor, 1.0),
                (BlockRelation::ImplementedBy, 0.8),
                (BlockRelation::UsedBy, 0.35),
            ],
        }
    }
}

/// Result from a weighted PageRank computation.
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

/// Computes weighted PageRank scores over a [`ProjectGraph`].
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

    /// Compute weighted PageRank and return results sorted by score (highest first).
    pub fn rank(&self) -> RankingResult {
        let entries = self.collect_entries();

        if entries.is_empty() {
            return RankingResult {
                blocks: Vec::new(),
                iterations: 0,
                converged: true,
            };
        }

        let adjacency_influence =
            self.build_weighted_adjacency(&entries, &self.config.edge_weights.influence);
        let adjacency_orchestration =
            self.build_weighted_adjacency(&entries, &self.config.edge_weights.orchestration);

        let teleport = self.build_teleport_vector(&entries);

        let (influence_scores, influence_iters, influence_converged) =
            self.compute_pagerank_weighted(&adjacency_influence, &teleport);
        let (orchestration_scores, orchestration_iters, orchestration_converged) =
            self.compute_pagerank_weighted(&adjacency_orchestration, &teleport);

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

    /// Get top k blocks by weighted PageRank score.
    pub fn top_k(&self, k: usize) -> Vec<RankedBlock> {
        self.rank().blocks.into_iter().take(k).collect()
    }

    /// Get top k blocks by influence score (foundational dependencies).
    pub fn top_k_influence(&self, k: usize) -> Vec<RankedBlock> {
        let mut blocks = self.rank().blocks;
        blocks.sort_by(|a, b| {
            b.influence_score
                .partial_cmp(&a.influence_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        blocks.into_iter().take(k).collect()
    }

    /// Get top k blocks by orchestration score (entry points / coordinators).
    pub fn top_k_orchestration(&self, k: usize) -> Vec<RankedBlock> {
        let mut blocks = self.rank().blocks;
        blocks.sort_by(|a, b| {
            b.orchestration_score
                .partial_cmp(&a.orchestration_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        blocks.into_iter().take(k).collect()
    }

    /// Get all weighted PageRank scores as a HashMap from BlockId to score.
    pub fn scores(&self) -> HashMap<BlockId, f64> {
        self.rank()
            .blocks
            .into_iter()
            .map(|rb| (rb.node.block_id, rb.score))
            .collect()
    }

    fn compute_pagerank_weighted(
        &self,
        adjacency: &[Vec<(usize, f64)>],
        teleport: &[f64],
    ) -> (Vec<f64>, usize, bool) {
        let n = adjacency.len();
        let mut ranks = teleport.to_vec();
        let mut next_ranks = vec![0.0; n];
        let damping = self.config.damping_factor;

        let mut iterations = 0;
        let mut converged = false;

        for iter in 0..self.config.max_iterations {
            iterations = iter + 1;
            for (idx, value) in teleport.iter().enumerate() {
                next_ranks[idx] = (1.0 - damping) * value;
            }

            let mut sink_mass = 0.0;
            for (idx, neighbors) in adjacency.iter().enumerate() {
                if neighbors.is_empty() {
                    sink_mass += ranks[idx];
                    continue;
                }

                let total_weight: f64 = neighbors.iter().map(|(_, w)| w).sum();
                if total_weight <= f64::EPSILON {
                    sink_mass += ranks[idx];
                    continue;
                }

                for &(target_idx, weight) in neighbors {
                    let share = ranks[idx] * damping * (weight / total_weight);
                    next_ranks[target_idx] += share;
                }
            }

            if sink_mass > 0.0 {
                for (idx, value) in teleport.iter().enumerate() {
                    next_ranks[idx] += sink_mass * damping * value;
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

    fn build_teleport_vector(&self, entries: &[BlockEntry]) -> Vec<f64> {
        let mut priors = Vec::with_capacity(entries.len());
        for entry in entries {
            let prior = self
                .config
                .kind_priors
                .iter()
                .find(|(kind, _)| *kind == entry.kind)
                .map(|(_, weight)| *weight)
                .unwrap_or(0.1);
            priors.push(prior);
        }

        let sum_priors: f64 = priors.iter().sum();
        let uniform = 1.0 / entries.len() as f64;
        let strength = self.config.teleport_prior_strength.clamp(0.0, 1.0);

        let mut teleport = Vec::with_capacity(entries.len());
        for prior in priors {
            let prior_norm = if sum_priors <= f64::EPSILON {
                uniform
            } else {
                prior / sum_priors
            };
            let blended = (1.0 - strength) * uniform + strength * prior_norm;
            teleport.push(blended);
        }

        teleport
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

    /// Build weighted adjacency from relation types.
    fn build_weighted_adjacency(
        &self,
        entries: &[BlockEntry],
        relations: &[(BlockRelation, f64)],
    ) -> Vec<Vec<(usize, f64)>> {
        let mut adjacency: Vec<Vec<(usize, f64)>> = vec![Vec::new(); entries.len()];
        let mut index_by_block: HashMap<BlockId, usize> = HashMap::new();

        for (idx, entry) in entries.iter().enumerate() {
            index_by_block.insert(entry.block_id, idx);
        }

        for (idx, entry) in entries.iter().enumerate() {
            let mut weighted_targets: HashMap<usize, f64> = HashMap::new();

            for &(relation, weight) in relations {
                if weight <= 0.0 {
                    continue;
                }

                let targets = self
                    .graph
                    .cc
                    .related_map
                    .get_related(entry.block_id, relation);

                for dep_id in targets {
                    if let Some(&target_idx) = index_by_block.get(&dep_id) {
                        if target_idx == idx {
                            continue;
                        }

                        let mut edge_weight = weight;
                        if self.config.cross_file_penalty < 1.0
                            && entry.file_path.is_some()
                            && entries[target_idx].file_path.is_some()
                            && entry.file_path != entries[target_idx].file_path
                        {
                            edge_weight *= self.config.cross_file_penalty;
                        }

                        *weighted_targets.entry(target_idx).or_insert(0.0) += edge_weight;
                    }
                }
            }

            let mut weighted: Vec<(usize, f64)> = weighted_targets.into_iter().collect();
            weighted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            if let Some(max_out) = self.config.max_out_degree {
                if weighted.len() > max_out {
                    weighted.truncate(max_out);
                }
            }

            adjacency[idx] = weighted;
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
