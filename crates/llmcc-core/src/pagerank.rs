use std::collections::{HashMap, VecDeque};

use crate::block::{BlockId, BlockKind, BlockRelation};
use crate::graph_builder::{GraphNode, ProjectGraph};

/// Edge direction for PageRank traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRankDirection {
    /// Follow DependsOn edges: rank flows toward heavily-depended-upon nodes (data types).
    DependsOn,
    /// Follow DependedBy edges: rank flows toward orchestrators that many nodes depend on.
    DependedBy,
}

impl Default for PageRankDirection {
    fn default() -> Self {
        // Default to DependedBy to surface orchestrators and coordinators
        // (more useful for understanding codebase behavior)
        Self::DependedBy
    }
}

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
    /// Edge direction: follow DependsOn or DependedBy edges.
    pub direction: PageRankDirection,
}

impl Default for PageRankConfig {
    fn default() -> Self {
        Self {
            damping_factor: 0.85,
            max_iterations: 100,
            tolerance: 1e-6,
            relation: BlockRelation::DependsOn,
            direction: PageRankDirection::default(),
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
        let mut entries = self.collect_entries();
        if entries.is_empty() {
            return Vec::new();
        }

        let mut outgoing = self.build_adjacency(&entries);

        // Remove isolated nodes with neither incoming nor outgoing edges so they don't skew scoring.
        let mut incoming_counts = vec![0usize; outgoing.len()];
        for neighbours in &outgoing {
            for &target_idx in neighbours {
                incoming_counts[target_idx] += 1;
            }
        }

        let mut any_filtered = false;
        let mut keep_mask = Vec::with_capacity(entries.len());
        for (idx, neighbours) in outgoing.iter().enumerate() {
            let has_outgoing = !neighbours.is_empty();
            let has_incoming = incoming_counts[idx] > 0;
            let keep = has_outgoing || has_incoming;
            if !keep {
                any_filtered = true;
            }
            keep_mask.push(keep);
        }

        if any_filtered {
            entries = entries
                .into_iter()
                .enumerate()
                .filter_map(|(idx, entry)| keep_mask[idx].then_some(entry))
                .collect();

            if entries.is_empty() {
                return Vec::new();
            }

            outgoing = self.build_adjacency(&entries);
        }

        let total_nodes = entries.len();
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

        // Apply block-kind weighting: favor Classes and Funcs over data types
        fn kind_weight(kind: BlockKind) -> f64 {
            match kind {
                BlockKind::Class | BlockKind::Enum => 2.0,
                _ => 1.0,
            }
        }

        let proximity_multipliers = compute_proximity_multipliers(&ranks, &outgoing);

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

                // Apply kind weight multiplier to the raw rank score
                let weighted_score = ranks[idx] * kind_weight(kind) * proximity_multipliers[idx];

                RankedBlock {
                    node: GraphNode {
                        unit_index,
                        block_id,
                    },
                    score: weighted_score,
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

            // Use the configured direction: follow DependsOn or DependedBy edges
            let relation_to_follow = match self.config.direction {
                PageRankDirection::DependsOn => self.config.relation,
                PageRankDirection::DependedBy => {
                    // Reverse the relation: if we want DependedBy, follow the reverse of DependsOn
                    match self.config.relation {
                        BlockRelation::DependsOn => BlockRelation::DependedBy,
                        BlockRelation::DependedBy => BlockRelation::DependsOn,
                        _ => self.config.relation,
                    }
                }
            };

            let mut targets = unit_graph
                .edges()
                .get_related(entry.block_id, relation_to_follow)
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

fn compute_proximity_multipliers(ranks: &[f64], adjacency: &[Vec<usize>]) -> Vec<f64> {
    let node_count = ranks.len();
    if node_count == 0 {
        return Vec::new();
    }

    let mut multipliers = vec![1.0; node_count];
    let mut closeness = vec![0.0; node_count];

    let mut top_indices: Vec<usize> = (0..node_count).collect();
    top_indices.sort_by(|a, b| {
        ranks[*b]
            .partial_cmp(&ranks[*a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    const TOP_LIMIT: usize = 20;
    const MAX_DEPTH: usize = 4;
    const ATTENUATION: f64 = 0.6;
    const STRENGTH: f64 = 0.75;

    if top_indices.len() > TOP_LIMIT {
        top_indices.truncate(TOP_LIMIT);
    }

    if top_indices.is_empty() {
        return multipliers;
    }

    let total_top_weight: f64 = top_indices.iter().map(|&idx| ranks[idx]).sum();
    let uniform_weight = 1.0 / top_indices.len() as f64;

    for &root in top_indices.iter() {
        let importance = if total_top_weight > 0.0 {
            ranks[root] / total_top_weight
        } else {
            uniform_weight
        };

        let mut visited = vec![false; node_count];
        let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
        queue.push_back((root, 0));
        visited[root] = true;

        while let Some((current, depth)) = queue.pop_front() {
            let decay = ATTENUATION.powf(depth as f64);
            closeness[current] += importance * decay;

            if depth >= MAX_DEPTH {
                continue;
            }

            if let Some(neighbors) = adjacency.get(current) {
                for &neighbor in neighbors {
                    if visited[neighbor] {
                        continue;
                    }
                    visited[neighbor] = true;
                    queue.push_back((neighbor, depth + 1));
                }
            }
        }
    }

    for (idx, boost) in closeness.into_iter().enumerate() {
        multipliers[idx] += STRENGTH * boost;
    }

    multipliers
}
