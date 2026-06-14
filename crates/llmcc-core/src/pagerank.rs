//! Weighted PageRank over block dependency graphs.
//!
//! The ranker computes two PageRank passes over the same block universe:
//! dependency/influence edges for foundational code and reverse/orchestration
//! edges for entry points or coordinators. The final score is a normalized
//! blend of those two passes.
//!
//! Cycles are expected in real dependency graphs. The damping factor keeps the
//! iteration convergent, and callers can inspect [`RankingResult::converged`]
//! to detect runs that hit the configured iteration limit.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;

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

/// Options for weighted PageRank.
#[derive(Debug, Clone)]
pub struct PageRankOptions {
    /// Damping factor (typically 0.85).
    damping_factor: f64,
    /// Maximum number of iterations.
    max_iterations: usize,
    /// Convergence tolerance.
    tolerance: f64,
    /// Weight assigned to dependency-following influence scores.
    influence_weight: f64,
    /// Weight assigned to reverse-dependency orchestration scores.
    orchestration_weight: f64,
    /// Penalty applied when an edge crosses file boundaries. 1.0 means no penalty.
    cross_file_penalty: f64,
    /// Optional cap on outgoing edges per node (keeps top weights only).
    max_out_degree: Option<usize>,
    /// Blend between uniform teleport (0.0) and kind-prior teleport (1.0).
    teleport_prior_strength: f64,
    /// Relation weights used to build the influence and orchestration graphs.
    relation_weights: RelationWeights,
    /// Per-kind teleport prior overrides.
    ///
    /// Missing kinds fall back to [`BlockKind::pagerank_prior`].
    kind_priors: HashMap<BlockKind, f64>,
}

impl Default for PageRankOptions {
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
            relation_weights: RelationWeights::default(),
            kind_priors: HashMap::new(),
        }
    }
}

impl PageRankOptions {
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

    /// Replace relation weights used for PageRank graph construction.
    pub fn with_relation_weights(mut self, weights: RelationWeights) -> Self {
        self.relation_weights = weights;
        self
    }

    /// Replace all per-kind teleport prior overrides.
    pub fn with_kind_priors(mut self, priors: impl IntoIterator<Item = (BlockKind, f64)>) -> Self {
        self.kind_priors = priors.into_iter().collect();
        self
    }

    /// Set or replace one per-kind teleport prior override.
    pub fn with_kind_prior(mut self, kind: BlockKind, prior: f64) -> Self {
        self.kind_priors.insert(kind, prior);
        self
    }

    /// Return the configured teleport prior for `kind`.
    pub fn kind_prior(&self, kind: BlockKind) -> f64 {
        self.kind_priors
            .get(&kind)
            .copied()
            .unwrap_or_else(|| kind.pagerank_prior())
    }

    /// Validate all numeric invariants before ranking.
    pub fn validate(&self) -> Result<()> {
        validate_open_probability("damping_factor", self.damping_factor)?;
        validate_positive_usize("max_iterations", self.max_iterations)?;
        validate_positive_finite("tolerance", self.tolerance)?;
        self.score_weights()?;

        validate_closed_probability("cross_file_penalty", self.cross_file_penalty)?;
        validate_closed_probability("teleport_prior_strength", self.teleport_prior_strength)?;

        if let Some(max_out_degree) = self.max_out_degree {
            validate_positive_usize("max_out_degree", max_out_degree)?;
        }

        self.relation_weights.validate()?;
        validate_kind_priors(&self.kind_priors)?;

        Ok(())
    }

    fn score_weights(&self) -> Result<ScoreWeights> {
        ScoreWeights::normalize(self.influence_weight, self.orchestration_weight)
    }
}

/// Normalized final-score blend weights.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ScoreWeights {
    /// Weight applied to dependency-following PageRank scores.
    influence: f64,
    /// Weight applied to reverse-dependency PageRank scores.
    orchestration: f64,
}

impl ScoreWeights {
    /// Normalize raw influence/orchestration weights.
    fn normalize(influence: f64, orchestration: f64) -> Result<Self> {
        validate_non_negative_finite("influence_weight", influence)?;
        validate_non_negative_finite("orchestration_weight", orchestration)?;

        let total = influence + orchestration;
        if total <= f64::EPSILON {
            return Err(invalid_config(
                "score_weights",
                "at least one score weight must be positive",
            ));
        }

        Ok(Self {
            influence: influence / total,
            orchestration: orchestration / total,
        })
    }
}

/// Relation weights for influence/orchestration graphs.
#[derive(Debug, Clone)]
pub struct RelationWeights {
    influence: HashMap<BlockRelation, f64>,
    orchestration: HashMap<BlockRelation, f64>,
}

impl Default for RelationWeights {
    fn default() -> Self {
        Self {
            influence: BlockRelation::pagerank_influence_weights()
                .into_iter()
                .collect(),
            orchestration: BlockRelation::pagerank_orchestration_weights()
                .into_iter()
                .collect(),
        }
    }
}

impl RelationWeights {
    /// Create relation weights for the two PageRank passes.
    pub fn new(
        influence: impl IntoIterator<Item = (BlockRelation, f64)>,
        orchestration: impl IntoIterator<Item = (BlockRelation, f64)>,
    ) -> Self {
        Self {
            influence: influence.into_iter().collect(),
            orchestration: orchestration.into_iter().collect(),
        }
    }

    /// Replace weights for dependency-following influence edges.
    pub fn with_influence(
        mut self,
        weights: impl IntoIterator<Item = (BlockRelation, f64)>,
    ) -> Self {
        self.influence = weights.into_iter().collect();
        self
    }

    /// Replace weights for reverse-dependency orchestration edges.
    pub fn with_orchestration(
        mut self,
        weights: impl IntoIterator<Item = (BlockRelation, f64)>,
    ) -> Self {
        self.orchestration = weights.into_iter().collect();
        self
    }

    /// Return dependency-following influence relation weights.
    pub fn influence(&self) -> impl Iterator<Item = (BlockRelation, f64)> + '_ {
        self.influence
            .iter()
            .map(|(relation, weight)| (*relation, *weight))
    }

    /// Return reverse-dependency orchestration relation weights.
    pub fn orchestration(&self) -> impl Iterator<Item = (BlockRelation, f64)> + '_ {
        self.orchestration
            .iter()
            .map(|(relation, weight)| (*relation, *weight))
    }

    fn validate(&self) -> Result<()> {
        for weight in self.influence.values() {
            validate_non_negative_finite("relation_weights.influence", *weight)?;
        }
        for weight in self.orchestration.values() {
            validate_non_negative_finite("relation_weights.orchestration", *weight)?;
        }
        Ok(())
    }
}

/// Result from a weighted PageRank computation.
#[derive(Debug, Clone)]
pub struct RankedBlock {
    node: UnitNode,
    /// Blended score based on the configured influence/orchestration weights.
    score: f64,
    /// Dependency-following PageRank score for foundational building blocks.
    influence_score: f64,
    /// Reverse-dependency PageRank score for orchestrators and entry points.
    orchestration_score: f64,
    label: String,
    kind: BlockKind,
    file_path: Option<String>,
}

impl RankedBlock {
    fn from_entry(
        entry: BlockEntry,
        score: f64,
        influence_score: f64,
        orchestration_score: f64,
    ) -> Self {
        let label = entry.to_string();

        Self {
            node: UnitNode {
                unit_index: entry.unit_index,
                block_id: entry.block_id,
            },
            score,
            influence_score,
            orchestration_score,
            label,
            kind: entry.kind,
            file_path: entry.file_path,
        }
    }

    /// Return this block's score for `metric`.
    pub fn score_for(&self, metric: RankMetric) -> f64 {
        match metric {
            RankMetric::Combined => self.score,
            RankMetric::Influence => self.influence_score,
            RankMetric::Orchestration => self.orchestration_score,
        }
    }

    /// Return this block's id.
    pub fn block_id(&self) -> BlockId {
        self.node.block_id
    }

    /// Return this block's unit index.
    pub fn unit_index(&self) -> usize {
        self.node.unit_index
    }

    /// Return this block's display label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Return this block's kind.
    pub fn kind(&self) -> BlockKind {
        self.kind
    }

    /// Return this block's source file path, if known.
    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    /// Return this block's combined score.
    pub fn score(&self) -> f64 {
        self.score
    }

    /// Return this block's influence score.
    pub fn influence_score(&self) -> f64 {
        self.influence_score
    }

    /// Return this block's orchestration score.
    pub fn orchestration_score(&self) -> f64 {
        self.orchestration_score
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

#[derive(Debug, Clone)]
struct RankedBlocks {
    blocks: Vec<RankedBlock>,
}

impl RankedBlocks {
    fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    fn sort_by(&mut self, metric: RankMetric) {
        self.blocks
            .sort_by(|left, right| Self::compare(left, right, metric));
    }

    fn compare(left: &RankedBlock, right: &RankedBlock, metric: RankMetric) -> Ordering {
        right
            .score_for(metric)
            .total_cmp(&left.score_for(metric))
            .then_with(|| left.node.unit_index.cmp(&right.node.unit_index))
            .then_with(|| left.node.block_id.cmp(&right.node.block_id))
    }

    fn as_slice(&self) -> &[RankedBlock] {
        &self.blocks
    }

    fn len(&self) -> usize {
        self.blocks.len()
    }

    fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    fn top(&self, k: usize) -> Vec<RankedBlock> {
        self.blocks.iter().take(k).cloned().collect()
    }

    fn top_by(&self, metric: RankMetric, k: usize) -> Vec<RankedBlock> {
        if metric == RankMetric::Combined {
            return self.top(k);
        }

        let mut blocks: Vec<_> = self.blocks.iter().collect();
        blocks.sort_by(|left, right| Self::compare(left, right, metric));
        blocks.into_iter().take(k).cloned().collect()
    }

    fn scores_by_block(&self, metric: RankMetric) -> HashMap<BlockId, f64> {
        self.blocks
            .iter()
            .map(|block| (block.block_id(), block.score_for(metric)))
            .collect()
    }

    fn into_vec(self) -> Vec<RankedBlock> {
        self.blocks
    }
}

impl FromIterator<RankedBlock> for RankedBlocks {
    fn from_iter<T: IntoIterator<Item = RankedBlock>>(iter: T) -> Self {
        Self {
            blocks: iter.into_iter().collect(),
        }
    }
}

/// Result from ranking computation.
#[derive(Debug)]
pub struct RankingResult {
    blocks: RankedBlocks,
    iterations: usize,
    converged: bool,
}

impl RankingResult {
    /// Return an empty converged ranking result.
    pub fn empty() -> Self {
        Self {
            blocks: RankedBlocks::new(),
            iterations: 0,
            converged: true,
        }
    }

    /// Return ranked blocks as a slice.
    pub fn blocks(&self) -> &[RankedBlock] {
        self.blocks.as_slice()
    }

    /// Return the number of ranked blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Return whether no blocks were ranked.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Return the maximum iterations used by the two PageRank passes.
    pub fn iterations(&self) -> usize {
        self.iterations
    }

    /// Return whether both PageRank passes converged.
    pub fn converged(&self) -> bool {
        self.converged
    }

    /// Return the top `k` blocks by combined score.
    pub fn top(&self, k: usize) -> Vec<RankedBlock> {
        self.blocks.top(k)
    }

    /// Return the top `k` blocks for a specific score metric.
    pub fn top_by(&self, metric: RankMetric, k: usize) -> Vec<RankedBlock> {
        self.blocks.top_by(metric, k)
    }

    /// Return scores by block id for `metric`.
    pub fn scores_by_block(&self, metric: RankMetric) -> HashMap<BlockId, f64> {
        self.blocks.scores_by_block(metric)
    }

    /// Consume this result and return all ranked blocks.
    pub fn into_blocks(self) -> Vec<RankedBlock> {
        self.blocks.into_vec()
    }
}

impl IntoIterator for RankingResult {
    type Item = RankedBlock;
    type IntoIter = std::vec::IntoIter<RankedBlock>;

    fn into_iter(self) -> Self::IntoIter {
        self.blocks.into_vec().into_iter()
    }
}

/// Computes weighted PageRank scores over a [`ProjectGraph`].
#[derive(Debug)]
pub struct PageRanker<'graph, 'tcx> {
    graph: &'graph ProjectGraph<'tcx>,
    options: PageRankOptions,
}

impl<'graph, 'tcx> PageRanker<'graph, 'tcx> {
    /// Create a new PageRanker with default configuration.
    pub fn new(graph: &'graph ProjectGraph<'tcx>) -> Self {
        Self {
            graph,
            options: PageRankOptions::default(),
        }
    }

    /// Create a new PageRanker with custom options.
    pub fn with_options(graph: &'graph ProjectGraph<'tcx>, options: PageRankOptions) -> Self {
        Self { graph, options }
    }

    /// Return this ranker's options.
    pub fn options(&self) -> &PageRankOptions {
        &self.options
    }

    /// Compute weighted PageRank and return results sorted by score (highest first).
    pub fn rank(&self) -> Result<RankingResult> {
        self.options.validate()?;

        let entries = self.collect_entries();
        if entries.is_empty() {
            return Ok(RankingResult::empty());
        }

        let adjacency_influence =
            self.build_adjacency(&entries, self.options.relation_weights.influence());
        let adjacency_orchestration =
            self.build_adjacency(&entries, self.options.relation_weights.orchestration());

        let teleport = self.build_teleport_vector(&entries);

        let influence = PageRankRun::compute(&adjacency_influence, &teleport, &self.options);
        let orchestration =
            PageRankRun::compute(&adjacency_orchestration, &teleport, &self.options);

        let blended_scores =
            PageRankRun::blend(&influence, &orchestration, self.options.score_weights()?);

        let iterations = influence.iterations.max(orchestration.iterations);
        let converged = influence.converged && orchestration.converged;

        let mut ranked: RankedBlocks = entries
            .into_iter()
            .enumerate()
            .map(|(idx, entry)| {
                RankedBlock::from_entry(
                    entry,
                    blended_scores[idx],
                    influence.scores[idx],
                    orchestration.scores[idx],
                )
            })
            .collect();
        ranked.sort_by(RankMetric::Combined);

        Ok(RankingResult {
            blocks: ranked,
            iterations,
            converged,
        })
    }

    fn build_teleport_vector(&self, entries: &[BlockEntry]) -> Vec<f64> {
        let mut priors = Vec::with_capacity(entries.len());
        for entry in entries {
            priors.push(self.options.kind_prior(entry.kind));
        }

        let sum_priors: f64 = priors.iter().sum();
        let uniform = 1.0 / entries.len() as f64;
        let strength = self.options.teleport_prior_strength;

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
    fn build_adjacency(
        &self,
        entries: &[BlockEntry],
        relations: impl IntoIterator<Item = (BlockRelation, f64)>,
    ) -> WeightedAdjacency {
        let relations: Vec<_> = relations.into_iter().collect();
        let mut adjacency = Vec::with_capacity(entries.len());
        let mut index_by_block: HashMap<BlockId, usize> = HashMap::new();

        for (idx, entry) in entries.iter().enumerate() {
            index_by_block.insert(entry.block_id, idx);
        }

        for (idx, entry) in entries.iter().enumerate() {
            let mut weighted = WeightedEdges::new();
            for &(relation, weight) in &relations {
                if weight <= 0.0 {
                    continue;
                }

                let targets = self.graph.related_blocks(entry.block_id, relation);
                for dep_id in targets {
                    if let Some(&target_idx) = index_by_block.get(&dep_id) {
                        if target_idx == idx {
                            continue;
                        }

                        let edge_weight = entry.weight_to(
                            &entries[target_idx],
                            weight,
                            self.options.cross_file_penalty,
                        );

                        weighted.insert(target_idx, edge_weight);
                    }
                }
            }

            weighted.keep_top(self.options.max_out_degree);
            adjacency.push(weighted);
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

impl BlockEntry {
    fn weight_to(&self, target: &Self, base_weight: f64, cross_file_penalty: f64) -> f64 {
        if cross_file_penalty < 1.0
            && self.file_path.is_some()
            && target.file_path.is_some()
            && self.file_path != target.file_path
        {
            base_weight * cross_file_penalty
        } else {
            base_weight
        }
    }
}

impl fmt::Display for BlockEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = self.name.as_deref().filter(|name| !name.is_empty()) {
            formatter.write_str(name)
        } else {
            write!(formatter, "{}:{}", self.kind, self.block_id.as_u32())
        }
    }
}

type WeightedAdjacency = Vec<WeightedEdges>;

#[derive(Debug, Clone, PartialEq)]
struct WeightedEdges {
    weights_by_target: HashMap<usize, f64>,
    edges_by_weight: Vec<WeightedEdge>,
    total_weight: f64,
}

impl WeightedEdges {
    fn new() -> Self {
        Self {
            weights_by_target: HashMap::new(),
            edges_by_weight: Vec::new(),
            total_weight: 0.0,
        }
    }

    fn insert(&mut self, target: usize, weight: f64) {
        *self.weights_by_target.entry(target).or_insert(0.0) += weight;
    }

    fn keep_top(&mut self, max_len: Option<usize>) {
        self.edges_by_weight = self
            .weights_by_target
            .iter()
            .map(|(target, weight)| WeightedEdge {
                target: *target,
                weight: *weight,
            })
            .collect();

        self.edges_by_weight.sort_by(|left, right| {
            right
                .weight
                .total_cmp(&left.weight)
                .then_with(|| left.target.cmp(&right.target))
        });

        if let Some(max_len) = max_len {
            self.edges_by_weight.truncate(max_len);
        }
        self.total_weight = self.edges_by_weight.iter().map(|edge| edge.weight).sum();
    }

    fn is_empty(&self) -> bool {
        self.edges_by_weight.is_empty()
    }

    fn total_weight(&self) -> f64 {
        self.total_weight
    }

    fn iter(&self) -> impl Iterator<Item = &WeightedEdge> {
        self.edges_by_weight.iter()
    }
}

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

impl PageRankRun {
    fn compute(adjacency: &WeightedAdjacency, teleport: &[f64], options: &PageRankOptions) -> Self {
        let node_count = adjacency.len();
        let mut ranks = teleport.to_vec();
        let mut next_ranks = vec![0.0; node_count];

        let mut iterations = 0;
        let mut converged = false;

        for iter in 0..options.max_iterations {
            iterations = iter + 1;
            for (idx, value) in teleport.iter().enumerate() {
                next_ranks[idx] = (1.0 - options.damping_factor) * value;
            }

            let mut sink_mass = 0.0;
            for (idx, neighbors) in adjacency.iter().enumerate() {
                if neighbors.is_empty() {
                    sink_mass += ranks[idx];
                    continue;
                }

                let total_weight = neighbors.total_weight();
                if total_weight <= f64::EPSILON {
                    sink_mass += ranks[idx];
                    continue;
                }

                for edge in neighbors.iter() {
                    let share = ranks[idx] * options.damping_factor * (edge.weight / total_weight);
                    next_ranks[edge.target] += share;
                }
            }

            if sink_mass > 0.0 {
                for (idx, value) in teleport.iter().enumerate() {
                    next_ranks[idx] += sink_mass * options.damping_factor * value;
                }
            }

            let delta: f64 = next_ranks
                .iter()
                .zip(&ranks)
                .map(|(new, old)| (new - old).abs())
                .sum();

            ranks.copy_from_slice(&next_ranks);

            if delta < options.tolerance {
                converged = true;
                break;
            }
        }

        Self {
            scores: ranks,
            iterations,
            converged,
        }
    }

    fn blend(influence: &Self, orchestration: &Self, weights: ScoreWeights) -> Vec<f64> {
        debug_assert_eq!(
            influence.scores.len(),
            orchestration.scores.len(),
            "PageRank runs must cover the same node set"
        );

        influence
            .scores
            .iter()
            .zip(&orchestration.scores)
            .map(|(influence, orchestration)| {
                influence * weights.influence + orchestration * weights.orchestration
            })
            .collect()
    }
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

fn validate_kind_priors(priors: &HashMap<BlockKind, f64>) -> Result<()> {
    for prior in priors.values() {
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
