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
        Self::DependedBy
    }
}

/// Configuration options for the ranking algorithms.
#[derive(Debug, Clone)]
pub struct PageRankConfig {
    /// Damping factor used when random walkers jump to a random node.
    pub damping_factor: f64,
    /// Maximum number of iterations to run PageRank.
    pub max_iterations: usize,
    /// Convergence tolerance (L1 delta between successive rank vectors).
    pub tolerance: f64,
    /// Which edge relation to consider when traversing the graph.
    pub relation: BlockRelation,
    /// Edge direction: follow DependsOn or DependedBy edges.
    pub direction: PageRankDirection,

    // Proximity boost configuration
    pub proximity_enabled: bool,
    pub proximity_top_n: usize,
    pub proximity_max_depth: usize,
    pub proximity_attenuation: f64,
    pub proximity_strength: f64,
    /// NEW: Proximity mode - how aggressively to boost nearby nodes
    pub proximity_mode: ProximityMode,

    // HITS algorithm iterations
    pub hits_iterations: usize,

    // Betweenness centrality configuration
    pub betweenness_enabled: bool,
    pub betweenness_normalized: bool,
}

/// How aggressively to boost nodes near high-ranked nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProximityMode {
    /// Conservative: Small boost, slow decay (attenuation^depth)
    Conservative,
    /// Moderate: Medium boost, moderate decay  
    Moderate,
    /// Aggressive: Large boost, slower decay (1/(depth+1))
    Aggressive,
    /// VeryAggressive: Very large boost, minimal decay (1/(1+0.5*depth))
    VeryAggressive,
}

impl Default for PageRankConfig {
    fn default() -> Self {
        Self {
            damping_factor: 0.85,
            max_iterations: 100,
            tolerance: 1e-6,
            relation: BlockRelation::DependsOn,
            direction: PageRankDirection::default(),
            proximity_enabled: true,
            proximity_top_n: 20,
            proximity_max_depth: 4,
            proximity_attenuation: 0.6,
            proximity_strength: 10.0,
            proximity_mode: ProximityMode::VeryAggressive,
            hits_iterations: 50,
            betweenness_enabled: true,
            betweenness_normalized: true,
        }
    }
}

/// HITS algorithm scores
#[derive(Debug, Clone, Copy)]
pub struct HITSScores {
    pub authority: f64,
    pub hub: f64,
}

/// Combined ranking result with all metrics
#[derive(Debug, Clone)]
pub struct RankedBlock {
    pub node: GraphNode,
    pub pagerank: f64,
    pub hits: HITSScores,
    pub betweenness: f64,
    pub composite_score: f64,
    pub name: String,
    pub kind: BlockKind,
    pub file_path: Option<String>,
}

impl RankedBlock {
    /// Get a description of this block's architectural role
    pub fn role_description(&self) -> &'static str {
        let high_pr = self.pagerank > 0.01;
        let high_auth = self.hits.authority > 0.1;
        let high_hub = self.hits.hub > 0.1;
        let high_between = self.betweenness > 0.1;

        // Check for critical bridge first
        if high_between && (high_pr || high_auth || high_hub) {
            return "Critical Bridge - Key integration point between subsystems";
        }

        match (high_pr, high_auth, high_hub) {
            (true, true, true) => "Critical Connector - Core component used and uses many others",
            (true, true, false) => "Foundation - Core utility/type heavily depended upon",
            (true, false, true) => "Orchestrator - Key coordinator that depends on many components",
            (false, true, false) => "Specialized Utility - Niche but important dependency",
            (false, false, true) => "Leaf Controller - Entry point or handler with limited reuse",
            _ => {
                if high_between {
                    "Architectural Bridge - Connects otherwise separate components"
                } else {
                    "Support Component - Helper or isolated functionality"
                }
            }
        }
    }

    /// Debug info showing all metrics
    pub fn debug_info(&self) -> String {
        format!(
            "{} | PR:{:.6} Auth:{:.6} Hub:{:.6} Bet:{:.6} Comp:{:.6}",
            self.name,
            self.pagerank,
            self.hits.authority,
            self.hits.hub,
            self.betweenness,
            self.composite_score
        )
    }
}

/// Result from ranking computation with diagnostics
#[derive(Debug)]
pub struct RankingResult {
    pub blocks: Vec<RankedBlock>,
    pub pagerank_iterations: usize,
    pub pagerank_converged: bool,
    pub total_nodes: usize,
    pub isolated_nodes_filtered: usize,
}

impl RankingResult {
    /// Consume and return just the blocks for easy iteration
    pub fn into_blocks(self) -> Vec<RankedBlock> {
        self.blocks
    }
}

impl IntoIterator for RankingResult {
    type Item = RankedBlock;
    type IntoIter = std::vec::IntoIter<RankedBlock>;

    fn into_iter(self) -> Self::IntoIter {
        self.blocks.into_iter()
    }
}

/// Computes PageRank and HITS scores over a [`ProjectGraph`].
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

    /// Compute combined PageRank + HITS scores and return results in descending order by composite score.
    pub fn rank(&self) -> RankingResult {
        let mut entries = self.collect_entries();
        let total_nodes_initial = entries.len();

        if entries.is_empty() {
            return RankingResult {
                blocks: Vec::new(),
                pagerank_iterations: 0,
                pagerank_converged: true,
                total_nodes: 0,
                isolated_nodes_filtered: 0,
            };
        }

        let mut outgoing = self.build_adjacency(&entries);

        // Remove isolated nodes
        let (entries_filtered, outgoing_filtered, isolated_count) =
            self.filter_isolated_nodes(entries, outgoing);

        entries = entries_filtered;
        outgoing = outgoing_filtered;

        if entries.is_empty() {
            return RankingResult {
                blocks: Vec::new(),
                pagerank_iterations: 0,
                pagerank_converged: true,
                total_nodes: total_nodes_initial,
                isolated_nodes_filtered: isolated_count,
            };
        }

        // Compute PageRank
        let (pagerank_scores, pr_iterations, pr_converged) = self.compute_pagerank(&outgoing);

        // Compute HITS
        let hits_scores = self.compute_hits(&outgoing);

        // Compute Betweenness Centrality
        let betweenness_scores = if self.config.betweenness_enabled {
            self.compute_betweenness(&outgoing)
        } else {
            vec![0.0; entries.len()]
        };

        // Apply proximity multipliers if enabled
        let proximity_multipliers = if self.config.proximity_enabled {
            self.compute_proximity_multipliers(&pagerank_scores, &outgoing)
        } else {
            vec![1.0; entries.len()]
        };

        // Build final ranked results
        let mut ranked: Vec<RankedBlock> = entries
            .into_iter()
            .enumerate()
            .map(|(idx, entry)| {
                let display_name = entry
                    .name
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("{}:{}", entry.kind, entry.block_id.as_u32()));

                // Calculate weighted PageRank
                let kind_multiplier = kind_weight(entry.kind);
                let weighted_pagerank =
                    pagerank_scores[idx] * kind_multiplier * proximity_multipliers[idx];

                // Composite score combines all metrics
                let composite_score = self.compute_composite_score(
                    weighted_pagerank,
                    hits_scores[idx],
                    betweenness_scores[idx],
                );

                RankedBlock {
                    node: GraphNode {
                        unit_index: entry.unit_index,
                        block_id: entry.block_id,
                    },
                    pagerank: weighted_pagerank,
                    hits: hits_scores[idx],
                    betweenness: betweenness_scores[idx],
                    composite_score,
                    name: display_name,
                    kind: entry.kind,
                    file_path: entry.file_path,
                }
            })
            .collect();

        ranked.sort_by(|a, b| {
            b.composite_score
                .partial_cmp(&a.composite_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        RankingResult {
            blocks: ranked,
            pagerank_iterations: pr_iterations,
            pagerank_converged: pr_converged,
            total_nodes: total_nodes_initial,
            isolated_nodes_filtered: isolated_count,
        }
    }

    /// Convenience: get top k blocks by composite score
    pub fn top_k(&self, k: usize) -> Vec<RankedBlock> {
        let result = self.rank();
        result.blocks.into_iter().take(k).collect()
    }

    /// Convenience: get top k authorities (most depended upon)
    pub fn top_authorities(&self, k: usize) -> Vec<RankedBlock> {
        let mut result = self.rank();
        result.blocks.sort_by(|a, b| {
            b.hits
                .authority
                .partial_cmp(&a.hits.authority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        result.blocks.into_iter().take(k).collect()
    }

    /// Convenience: get top k hubs (biggest orchestrators)
    pub fn top_hubs(&self, k: usize) -> Vec<RankedBlock> {
        let mut result = self.rank();
        result.blocks.sort_by(|a, b| {
            b.hits
                .hub
                .partial_cmp(&a.hits.hub)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        result.blocks.into_iter().take(k).collect()
    }

    /// Convenience: get top k bridges (highest betweenness)
    pub fn top_bridges(&self, k: usize) -> Vec<RankedBlock> {
        let mut result = self.rank();
        result.blocks.sort_by(|a, b| {
            b.betweenness
                .partial_cmp(&a.betweenness)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        result.blocks.into_iter().take(k).collect()
    }

    /// Convenience: blocks above threshold composite score
    pub fn above_threshold(&self, threshold: f64) -> Vec<RankedBlock> {
        self.rank()
            .blocks
            .into_iter()
            .filter(|block| block.composite_score >= threshold)
            .collect()
    }

    /// Debug: Find specific blocks by name pattern
    pub fn find_blocks(&self, pattern: &str) -> Vec<RankedBlock> {
        self.rank()
            .blocks
            .into_iter()
            .filter(|block| block.name.to_lowercase().contains(&pattern.to_lowercase()))
            .collect()
    }

    /// Debug: Show statistics about the graph
    pub fn graph_statistics(&self) -> String {
        let entries = self.collect_entries();
        let adjacency = self.build_adjacency(&entries);

        let total_nodes = entries.len();
        let total_edges: usize = adjacency.iter().map(|n| n.len()).sum();
        let isolated: usize = adjacency
            .iter()
            .enumerate()
            .filter(|(idx, neighbors)| {
                let has_outgoing = !neighbors.is_empty();
                let has_incoming = adjacency.iter().any(|n| n.contains(idx));
                !has_outgoing && !has_incoming
            })
            .count();

        format!(
            "Graph Statistics:\n\
             - Total nodes: {}\n\
             - Total edges: {}\n\
             - Isolated nodes: {}\n\
             - Avg out-degree: {:.2}",
            total_nodes,
            total_edges,
            isolated,
            total_edges as f64 / total_nodes.max(1) as f64
        )
    }

    fn filter_isolated_nodes(
        &self,
        entries: Vec<BlockEntry>,
        outgoing: Vec<Vec<usize>>,
    ) -> (Vec<BlockEntry>, Vec<Vec<usize>>, usize) {
        let mut incoming_counts = vec![0usize; outgoing.len()];
        for neighbours in &outgoing {
            for &target_idx in neighbours {
                incoming_counts[target_idx] += 1;
            }
        }

        let mut keep_mask: Vec<bool> = outgoing
            .iter()
            .enumerate()
            .map(|(idx, neighbours)| !neighbours.is_empty() || incoming_counts[idx] > 0)
            .collect();

        let isolated_count = keep_mask.iter().filter(|&&keep| !keep).count();

        if isolated_count == 0 {
            return (entries, outgoing, 0);
        }

        let filtered_entries: Vec<BlockEntry> = entries
            .into_iter()
            .enumerate()
            .filter_map(|(idx, entry)| keep_mask[idx].then_some(entry))
            .collect();

        if filtered_entries.is_empty() {
            return (Vec::new(), Vec::new(), isolated_count);
        }

        let filtered_outgoing = self.build_adjacency(&filtered_entries);
        (filtered_entries, filtered_outgoing, isolated_count)
    }

    fn compute_pagerank(&self, adjacency: &[Vec<usize>]) -> (Vec<f64>, usize, bool) {
        let total_nodes = adjacency.len();
        let mut ranks = vec![1.0 / total_nodes as f64; total_nodes];
        let mut next_ranks = vec![0.0; total_nodes];
        let damping = self.config.damping_factor;
        let teleport = (1.0 - damping) / total_nodes as f64;

        let mut iterations = 0;
        let mut converged = false;

        for iter in 0..self.config.max_iterations {
            iterations = iter + 1;
            next_ranks.fill(teleport);

            let mut sink_mass = 0.0;
            for (idx, neighbours) in adjacency.iter().enumerate() {
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
                converged = true;
                break;
            }
        }

        (ranks, iterations, converged)
    }

    fn compute_hits(&self, adjacency: &[Vec<usize>]) -> Vec<HITSScores> {
        let n = adjacency.len();
        let mut auth = vec![1.0; n];
        let mut hub = vec![1.0; n];

        // Build reverse adjacency (incoming edges)
        let mut incoming: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, neighbors) in adjacency.iter().enumerate() {
            for &j in neighbors {
                incoming[j].push(i);
            }
        }

        for _ in 0..self.config.hits_iterations {
            let mut new_auth = vec![0.0; n];
            let mut new_hub = vec![0.0; n];

            // Authority = sum of hub scores pointing to me
            for i in 0..n {
                for &j in &incoming[i] {
                    new_auth[i] += hub[j];
                }
            }

            // Hub = sum of authority scores I point to
            for i in 0..n {
                for &j in &adjacency[i] {
                    new_hub[i] += auth[j];
                }
            }

            // Normalize to prevent overflow
            let auth_norm: f64 = new_auth.iter().map(|x| x * x).sum::<f64>().sqrt();
            let hub_norm: f64 = new_hub.iter().map(|x| x * x).sum::<f64>().sqrt();

            if auth_norm > 1e-10 {
                new_auth.iter_mut().for_each(|x| *x /= auth_norm);
            }
            if hub_norm > 1e-10 {
                new_hub.iter_mut().for_each(|x| *x /= hub_norm);
            }

            auth = new_auth;
            hub = new_hub;
        }

        (0..n)
            .map(|i| HITSScores {
                authority: auth[i],
                hub: hub[i],
            })
            .collect()
    }

    fn compute_composite_score(&self, pagerank: f64, hits: HITSScores, betweenness: f64) -> f64 {
        // Weighted combination of metrics
        // Adjust these weights based on what matters most for your analysis
        const PAGERANK_WEIGHT: f64 = 0.40;
        const AUTHORITY_WEIGHT: f64 = 0.25;
        const HUB_WEIGHT: f64 = 0.15;
        const BETWEENNESS_WEIGHT: f64 = 0.20;

        PAGERANK_WEIGHT * pagerank
            + AUTHORITY_WEIGHT * hits.authority
            + HUB_WEIGHT * hits.hub
            + BETWEENNESS_WEIGHT * betweenness
    }

    fn compute_betweenness(&self, adjacency: &[Vec<usize>]) -> Vec<f64> {
        let n = adjacency.len();
        let mut betweenness = vec![0.0; n];

        // Build reverse adjacency for undirected traversal
        let mut incoming: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, neighbors) in adjacency.iter().enumerate() {
            for &j in neighbors {
                incoming[j].push(i);
            }
        }

        // Brandes' algorithm for betweenness centrality
        for s in 0..n {
            let mut stack = Vec::new();
            let mut predecessors: Vec<Vec<usize>> = vec![Vec::new(); n];
            let mut sigma = vec![0.0; n];
            sigma[s] = 1.0;
            let mut dist = vec![-1i32; n];
            dist[s] = 0;

            // BFS to find shortest paths
            let mut queue = VecDeque::new();
            queue.push_back(s);

            while let Some(v) = queue.pop_front() {
                stack.push(v);

                // Check both outgoing and incoming edges (treat as undirected for betweenness)
                for &w in &adjacency[v] {
                    // First time we see w
                    if dist[w] < 0 {
                        queue.push_back(w);
                        dist[w] = dist[v] + 1;
                    }
                    // Shortest path to w via v
                    if dist[w] == dist[v] + 1 {
                        sigma[w] += sigma[v];
                        predecessors[w].push(v);
                    }
                }

                // Also check incoming edges
                for &w in &incoming[v] {
                    if dist[w] < 0 {
                        queue.push_back(w);
                        dist[w] = dist[v] + 1;
                    }
                    if dist[w] == dist[v] + 1 {
                        sigma[w] += sigma[v];
                        predecessors[w].push(v);
                    }
                }
            }

            // Accumulation phase - back-propagate dependencies
            let mut delta = vec![0.0; n];
            while let Some(w) = stack.pop() {
                for &v in &predecessors[w] {
                    delta[v] += (sigma[v] / sigma[w]) * (1.0 + delta[w]);
                }
                if w != s {
                    betweenness[w] += delta[w];
                }
            }
        }

        // Normalize if requested
        if self.config.betweenness_normalized && n > 2 {
            let normalization = ((n - 1) * (n - 2)) as f64;
            for score in &mut betweenness {
                *score /= normalization;
            }
        }

        betweenness
    }

    fn compute_proximity_multipliers(&self, ranks: &[f64], adjacency: &[Vec<usize>]) -> Vec<f64> {
        let node_count = ranks.len();
        if node_count == 0 {
            return Vec::new();
        }

        // Build bidirectional adjacency for proximity (consider all connections)
        let mut bidirectional: Vec<Vec<usize>> = vec![Vec::new(); node_count];
        for (i, neighbors) in adjacency.iter().enumerate() {
            for &j in neighbors {
                // Add both directions
                if !bidirectional[i].contains(&j) {
                    bidirectional[i].push(j);
                }
                if !bidirectional[j].contains(&i) {
                    bidirectional[j].push(i);
                }
            }
        }

        let mut closeness = vec![0.0; node_count];
        let mut top_indices: Vec<usize> = (0..node_count).collect();

        top_indices.sort_unstable_by(|a, b| {
            ranks[*b]
                .partial_cmp(&ranks[*a])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        top_indices.truncate(self.config.proximity_top_n.min(node_count));

        if top_indices.is_empty() {
            return vec![1.0; node_count];
        }

        let total_top_weight: f64 = top_indices.iter().map(|&idx| ranks[idx]).sum();

        // Reuse buffers for efficiency
        let mut visited = vec![false; node_count];
        let mut queue = VecDeque::with_capacity(node_count / 4);

        for &root in &top_indices {
            let importance = if total_top_weight > 0.0 {
                ranks[root] / total_top_weight
            } else {
                1.0 / top_indices.len() as f64
            };

            visited.fill(false);
            queue.clear();
            queue.push_back((root, 0));
            visited[root] = true;

            while let Some((current, depth)) = queue.pop_front() {
                let decay = self.config.proximity_attenuation.powi(depth as i32);
                closeness[current] += importance * decay;

                if depth >= self.config.proximity_max_depth {
                    continue;
                }

                // Use bidirectional adjacency
                for &neighbor in &bidirectional[current] {
                    if !visited[neighbor] {
                        visited[neighbor] = true;
                        queue.push_back((neighbor, depth + 1));
                    }
                }
            }
        }

        closeness
            .into_iter()
            .map(|boost| 1.0 + self.config.proximity_strength * boost)
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

            let relation_to_follow = match self.config.direction {
                PageRankDirection::DependsOn => self.config.relation,
                PageRankDirection::DependedBy => match self.config.relation {
                    BlockRelation::DependsOn => BlockRelation::DependedBy,
                    BlockRelation::DependedBy => BlockRelation::DependsOn,
                    _ => self.config.relation,
                },
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

fn kind_weight(kind: BlockKind) -> f64 {
    match kind {
        BlockKind::Class | BlockKind::Enum => 2.0,
        BlockKind::Func => 1.5,
        _ => 1.0,
    }
}
