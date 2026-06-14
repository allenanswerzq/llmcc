//! Weighted PageRank over block dependency graphs.
//!
//! The ranker computes two PageRank passes over the same block universe:
//! dependency/influence edges for foundational code and reverse/orchestration
//! edges for entry points or coordinators. The final score is a normalized
//! blend of those two passes.

use std::cmp::Ordering;
use std::collections::HashMap;

use strum_macros::{Display, EnumString};

use crate::block::{BlockId, BlockKind, BlockRelation};
use crate::graph::{ProjectGraph, UnitNode};
use crate::{Error, ErrorKind, Result};

const DEFAULT_DAMPING_FACTOR: f64 = 0.85;
const DEFAULT_MAX_ITERATIONS: usize = 100;
const DEFAULT_TOLERANCE: f64 = 1e-6;
const DEFAULT_INFLUENCE_WEIGHT: f64 = 0.25;
const DEFAULT_ORCHESTRATION_WEIGHT: f64 = 0.75;
const DEFAULT_CROSS_FILE_PENALTY: f64 = 0.8;
const DEFAULT_MAX_OUT_DEGREE: usize = 150;
const DEFAULT_TELEPORT_PRIOR_STRENGTH: f64 = 0.12;
const FALLBACK_KIND_PRIOR: f64 = 0.1;

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
            damping_factor: DEFAULT_DAMPING_FACTOR,
            max_iterations: DEFAULT_MAX_ITERATIONS,
            tolerance: DEFAULT_TOLERANCE,
            influence_weight: DEFAULT_INFLUENCE_WEIGHT,
            orchestration_weight: DEFAULT_ORCHESTRATION_WEIGHT,
            cross_file_penalty: DEFAULT_CROSS_FILE_PENALTY,
            max_out_degree: Some(DEFAULT_MAX_OUT_DEGREE),
            teleport_prior_strength: DEFAULT_TELEPORT_PRIOR_STRENGTH,
            edge_weights: EdgeWeights::default(),
            kind_priors: BlockKind::pagerank_priors(),
        }
    }
}

impl PageRankConfig {
    /// Return the default PageRank configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the damping factor. Must be finite and in `(0, 1)`.
    pub fn with_damping_factor(mut self, value: f64) -> Self {
        self.damping_factor = value;
        self
    }

    /// Set the maximum number of PageRank iterations.
    pub fn with_max_iterations(mut self, value: usize) -> Self {
        self.max_iterations = value;
        self
    }

    /// Set the convergence tolerance. Must be finite and positive.
    pub fn with_tolerance(mut self, value: f64) -> Self {
        self.tolerance = value;
        self
    }

    /// Set the final blend between influence and orchestration scores.
    pub fn with_score_weights(mut self, influence: f64, orchestration: f64) -> Self {
        self.influence_weight = influence;
        self.orchestration_weight = orchestration;
        self
    }

    /// Set the multiplier applied to cross-file edges.
    pub fn with_cross_file_penalty(mut self, value: f64) -> Self {
        self.cross_file_penalty = value;
        self
    }

    /// Set the maximum retained outgoing edge count per block.
    pub fn with_max_out_degree(mut self, value: Option<usize>) -> Self {
        self.max_out_degree = value;
        self
    }

    /// Set the blend between uniform and kind-prior teleport vectors.
    pub fn with_teleport_prior_strength(mut self, value: f64) -> Self {
        self.teleport_prior_strength = value;
        self
    }

    /// Validate all numeric invariants before ranking.
    pub fn validate(&self) -> Result<()> {
        validate_open_probability("damping_factor", self.damping_factor)?;
        validate_positive_usize("max_iterations", self.max_iterations)?;
        validate_positive_finite("tolerance", self.tolerance)?;
        validate_non_negative_finite("influence_weight", self.influence_weight)?;
        validate_non_negative_finite("orchestration_weight", self.orchestration_weight)?;

        if self.influence_weight + self.orchestration_weight <= f64::EPSILON {
            return Err(invalid_config(
                "score_weights",
                "at least one score weight must be positive",
            ));
        }

        validate_closed_probability("cross_file_penalty", self.cross_file_penalty)?;
        validate_closed_probability("teleport_prior_strength", self.teleport_prior_strength)?;

        if let Some(max_out_degree) = self.max_out_degree {
            validate_positive_usize("max_out_degree", max_out_degree)?;
        }

        validate_relation_weights("edge_weights.influence", &self.edge_weights.influence)?;
        validate_relation_weights(
            "edge_weights.orchestration",
            &self.edge_weights.orchestration,
        )?;
        validate_kind_priors(&self.kind_priors)?;

        Ok(())
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

impl RankedBlock {
    /// Return this block's score for `metric`.
    pub fn score_for(&self, metric: RankMetric) -> f64 {
        match metric {
            RankMetric::Combined => self.score,
            RankMetric::Influence => self.influence_score,
            RankMetric::Orchestration => self.orchestration_score,
        }
    }
}

/// Score metric used when sorting ranked blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Display, EnumString)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub enum RankMetric {
    /// Weighted blend of influence and orchestration scores.
    #[default]
    Combined,
    /// Dependency-following score for foundational code.
    Influence,
    /// Reverse-dependency score for coordinating code.
    Orchestration,
}

/// Result from ranking computation.
#[derive(Debug)]
pub struct RankingResult {
    pub blocks: Vec<RankedBlock>,
    pub iterations: usize,
    pub converged: bool,
}

impl RankingResult {
    /// Return an empty converged ranking result.
    pub fn empty() -> Self {
        Self {
            blocks: Vec::new(),
            iterations: 0,
            converged: true,
        }
    }

    /// Return ranked blocks as a slice.
    pub fn blocks(&self) -> &[RankedBlock] {
        &self.blocks
    }

    /// Return the number of ranked blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Return whether no blocks were ranked.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Return the top `k` blocks by combined score.
    pub fn top(&self, k: usize) -> Vec<RankedBlock> {
        self.top_by(RankMetric::Combined, k)
    }

    /// Return the top `k` blocks for a specific score metric.
    pub fn top_by(&self, metric: RankMetric, k: usize) -> Vec<RankedBlock> {
        let mut blocks = self.blocks.clone();
        sort_ranked_blocks_by(&mut blocks, metric);
        blocks.into_iter().take(k).collect()
    }

    /// Return combined scores by block id.
    pub fn scores(&self) -> HashMap<BlockId, f64> {
        self.blocks
            .iter()
            .map(|block| (block.node.block_id, block.score))
            .collect()
    }
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
    pub fn with(graph: &'graph ProjectGraph<'tcx>, config: PageRankConfig) -> Self {
        Self { graph, config }
    }

    /// Compute weighted PageRank and return results sorted by score (highest first).
    pub fn rank(&self) -> Result<RankingResult> {
        self.config.validate()?;

        let entries = self.collect_entries();

        if entries.is_empty() {
            return Ok(RankingResult::empty());
        }

        let adjacency_influence =
            self.build_weighted_adjacency(&entries, &self.config.edge_weights.influence);
        let adjacency_orchestration =
            self.build_weighted_adjacency(&entries, &self.config.edge_weights.orchestration);

        let teleport = self.build_teleport_vector(&entries);

        let influence = self.compute_pagerank_weighted(&adjacency_influence, &teleport);
        let orchestration = self.compute_pagerank_weighted(&adjacency_orchestration, &teleport);

        let (influence_weight, orchestration_weight) = self.normalized_score_weights();

        let blended_scores: Vec<f64> = influence
            .scores
            .iter()
            .zip(&orchestration.scores)
            .map(|(influence, orchestration)| {
                influence * influence_weight + orchestration * orchestration_weight
            })
            .collect();

        let iterations = influence.iterations.max(orchestration.iterations);
        let converged = influence.converged && orchestration.converged;

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
                    influence_score: influence.scores[idx],
                    orchestration_score: orchestration.scores[idx],
                    name: display_name,
                    kind: entry.kind,
                    file_path: entry.file_path,
                }
            })
            .collect();

        sort_ranked_blocks_by(&mut ranked, RankMetric::Combined);

        Ok(RankingResult {
            blocks: ranked,
            iterations,
            converged,
        })
    }

    /// Get top k blocks by weighted PageRank score.
    pub fn top(&self, k: usize) -> Result<Vec<RankedBlock>> {
        self.rank().map(|ranking| ranking.top(k))
    }

    /// Get top k blocks by influence score (foundational dependencies).
    pub fn top_influence(&self, k: usize) -> Result<Vec<RankedBlock>> {
        self.rank()
            .map(|ranking| ranking.top_by(RankMetric::Influence, k))
    }

    /// Get top k blocks by orchestration score (entry points / coordinators).
    pub fn top_orchestration(&self, k: usize) -> Result<Vec<RankedBlock>> {
        self.rank()
            .map(|ranking| ranking.top_by(RankMetric::Orchestration, k))
    }

    /// Get all weighted PageRank scores as a HashMap from BlockId to score.
    pub fn scores(&self) -> Result<HashMap<BlockId, f64>> {
        self.rank().map(|ranking| ranking.scores())
    }

    fn compute_pagerank_weighted(
        &self,
        adjacency: &WeightedAdjacency,
        teleport: &[f64],
    ) -> PageRankRun {
        compute_weighted_pagerank(
            adjacency,
            teleport,
            self.config.damping_factor,
            self.config.max_iterations,
            self.config.tolerance,
        )
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
                .unwrap_or(FALLBACK_KIND_PRIOR);
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

    fn normalized_score_weights(&self) -> (f64, f64) {
        let total_weight = self.config.influence_weight + self.config.orchestration_weight;
        (
            self.config.influence_weight / total_weight,
            self.config.orchestration_weight / total_weight,
        )
    }

    fn collect_entries(&self) -> Vec<BlockEntry> {
        let mut raw_entries = self.graph.context().blocks();
        raw_entries.sort_by_key(|entry| entry.block_id.as_u32());
        raw_entries
            .into_iter()
            .map(|entry| {
                let file_path = self
                    .graph
                    .context()
                    .file(entry.unit_index)
                    .and_then(|file| file.path().map(|path| path.to_string()));

                BlockEntry {
                    block_id: entry.block_id,
                    unit_index: entry.unit_index,
                    name: entry.name,
                    kind: entry.kind,
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
    ) -> WeightedAdjacency {
        let mut adjacency: WeightedAdjacency = vec![Vec::new(); entries.len()];
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
                    .context()
                    .block_relations()
                    .related(entry.block_id, relation);

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

            let mut weighted: Vec<WeightedEdge> = weighted_targets
                .into_iter()
                .map(|(target, weight)| WeightedEdge { target, weight })
                .collect();
            sort_weighted_edges(&mut weighted);

            if let Some(max_out) = self.config.max_out_degree
                && weighted.len() > max_out
            {
                weighted.truncate(max_out);
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

type WeightedAdjacency = Vec<Vec<WeightedEdge>>;

#[derive(Debug, Clone, Copy, PartialEq)]
struct WeightedEdge {
    target: usize,
    weight: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct PageRankRun {
    scores: Vec<f64>,
    iterations: usize,
    converged: bool,
}

fn compute_weighted_pagerank(
    adjacency: &WeightedAdjacency,
    teleport: &[f64],
    damping_factor: f64,
    max_iterations: usize,
    tolerance: f64,
) -> PageRankRun {
    let node_count = adjacency.len();
    let mut ranks = teleport.to_vec();
    let mut next_ranks = vec![0.0; node_count];

    let mut iterations = 0;
    let mut converged = false;

    for iter in 0..max_iterations {
        iterations = iter + 1;
        for (idx, value) in teleport.iter().enumerate() {
            next_ranks[idx] = (1.0 - damping_factor) * value;
        }

        let mut sink_mass = 0.0;
        for (idx, neighbors) in adjacency.iter().enumerate() {
            if neighbors.is_empty() {
                sink_mass += ranks[idx];
                continue;
            }

            let total_weight: f64 = neighbors.iter().map(|edge| edge.weight).sum();
            if total_weight <= f64::EPSILON {
                sink_mass += ranks[idx];
                continue;
            }

            for edge in neighbors {
                let share = ranks[idx] * damping_factor * (edge.weight / total_weight);
                next_ranks[edge.target] += share;
            }
        }

        if sink_mass > 0.0 {
            for (idx, value) in teleport.iter().enumerate() {
                next_ranks[idx] += sink_mass * damping_factor * value;
            }
        }

        let delta: f64 = next_ranks
            .iter()
            .zip(&ranks)
            .map(|(new, old)| (new - old).abs())
            .sum();

        ranks.copy_from_slice(&next_ranks);

        if delta < tolerance {
            converged = true;
            break;
        }
    }

    PageRankRun {
        scores: ranks,
        iterations,
        converged,
    }
}

fn sort_weighted_edges(edges: &mut [WeightedEdge]) {
    edges.sort_by(|left, right| {
        right
            .weight
            .total_cmp(&left.weight)
            .then_with(|| left.target.cmp(&right.target))
    });
}

fn sort_ranked_blocks_by(blocks: &mut [RankedBlock], metric: RankMetric) {
    blocks.sort_by(|left, right| compare_ranked_blocks(left, right, metric));
}

fn compare_ranked_blocks(left: &RankedBlock, right: &RankedBlock, metric: RankMetric) -> Ordering {
    right
        .score_for(metric)
        .total_cmp(&left.score_for(metric))
        .then_with(|| left.node.unit_index.cmp(&right.node.unit_index))
        .then_with(|| left.node.block_id.cmp(&right.node.block_id))
}

fn validate_open_probability(field: &'static str, value: f64) -> Result<()> {
    if value.is_finite() && value > 0.0 && value < 1.0 {
        return Ok(());
    }
    Err(invalid_config(
        field,
        "must be finite and in the open range (0, 1)",
    ))
}

fn validate_closed_probability(field: &'static str, value: f64) -> Result<()> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        return Ok(());
    }
    Err(invalid_config(
        field,
        "must be finite and in the range [0, 1]",
    ))
}

fn validate_positive_finite(field: &'static str, value: f64) -> Result<()> {
    if value.is_finite() && value > 0.0 {
        return Ok(());
    }
    Err(invalid_config(field, "must be finite and greater than 0"))
}

fn validate_non_negative_finite(field: &'static str, value: f64) -> Result<()> {
    if value.is_finite() && value >= 0.0 {
        return Ok(());
    }
    Err(invalid_config(
        field,
        "must be finite and greater than or equal to 0",
    ))
}

fn validate_positive_usize(field: &'static str, value: usize) -> Result<()> {
    if value > 0 {
        return Ok(());
    }
    Err(invalid_config(field, "must be greater than 0"))
}

fn validate_relation_weights(field: &'static str, weights: &[(BlockRelation, f64)]) -> Result<()> {
    for (_, weight) in weights {
        validate_non_negative_finite(field, *weight)?;
    }
    Ok(())
}

fn validate_kind_priors(priors: &[(BlockKind, f64)]) -> Result<()> {
    for (_, prior) in priors {
        validate_non_negative_finite("kind_priors", *prior)?;
    }
    Ok(())
}

fn invalid_config(field: &'static str, reason: impl Into<String>) -> Error {
    Error::new(ErrorKind::ConfigInvalid, "invalid PageRank configuration")
        .with_operation("pagerank.validate")
        .with_context("field", field)
        .with_context("reason", reason)
}
