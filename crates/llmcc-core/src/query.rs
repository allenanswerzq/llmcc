use crate::block::BlockKind;
use crate::graph_builder::{GraphNode, ProjectGraph};
use std::collections::HashMap;

/// Query API for semantic code questions built on top of ProjectGraph:
/// - given a function name, find all related code
/// - given a struct name, find all related code
/// - given a module/folder, find related modules
/// - given a file name, extract important structures (functions, types, etc.)
///
/// Output format: plain text suitable for LLM ingestion

/// Represents a semantic code block from the project graph
#[derive(Debug, Clone)]
pub struct GraphBlockInfo {
    pub name: String,
    pub kind: String,
    pub file_path: Option<String>,
    pub node: GraphNode,
}

impl GraphBlockInfo {
    pub fn format_for_llm(&self) -> String {
        format!(
            "{} [{}] at {}",
            self.name,
            self.kind,
            self.file_path.as_ref().unwrap_or(&"<unknown>".to_string()),
        )
    }
}

/// Query results grouped by relevance and type
#[derive(Debug, Default)]
pub struct QueryResult {
    pub primary: Vec<GraphBlockInfo>,
    pub related: Vec<GraphBlockInfo>,
    pub definitions: HashMap<String, GraphBlockInfo>,
}

impl QueryResult {
    pub fn format_for_llm(&self) -> String {
        let mut output = String::new();

        if !self.primary.is_empty() {
            output.push_str("=== PRIMARY RESULTS ===\n");
            for block in &self.primary {
                output.push_str(&block.format_for_llm());
                output.push_str("\n");
            }
            output.push_str("\n");
        }

        if !self.related.is_empty() {
            output.push_str("=== RELATED BLOCKS ===\n");
            for block in &self.related {
                output.push_str(&block.format_for_llm());
                output.push_str("\n");
            }
            output.push_str("\n");
        }

        if !self.definitions.is_empty() {
            output.push_str("=== DEFINITIONS ===\n");
            for (name, block) in &self.definitions {
                output.push_str(&format!("{}: {}\n", name, block.format_for_llm()));
            }
        }

        output
    }
}

/// Main query interface built on ProjectGraph
pub struct ProjectQuery<'tcx> {
    graph: &'tcx ProjectGraph<'tcx>,
}

impl<'tcx> ProjectQuery<'tcx> {
    pub fn new(graph: &'tcx ProjectGraph<'tcx>) -> Self {
        Self { graph }
    }

    /// Find all blocks with a given name
    pub fn find_by_name(&self, name: &str) -> QueryResult {
        let mut result = QueryResult::default();

        let blocks = self.graph.blocks_by_name(name);
        for node in blocks {
            if let Some(block_info) = self.node_to_block_info(node) {
                result.primary.push(block_info.clone());
                result
                    .definitions
                    .insert(block_info.name.clone(), block_info);
            }
        }

        result
    }

    /// Find all functions in the project
    pub fn find_all_functions(&self) -> QueryResult {
        self.find_by_kind(BlockKind::Func)
    }

    /// Find all structs in the project
    pub fn find_all_structs(&self) -> QueryResult {
        self.find_by_kind(BlockKind::Class)
    }

    /// Find all items of a specific kind
    pub fn find_by_kind(&self, kind: BlockKind) -> QueryResult {
        let mut result = QueryResult::default();

        let blocks = self.graph.blocks_by_kind(kind);
        for node in blocks {
            if let Some(block_info) = self.node_to_block_info(node) {
                result.primary.push(block_info.clone());
            }
        }

        result
    }

    /// Get all blocks defined in a specific file/unit
    pub fn file_structure(&self, unit_index: usize) -> QueryResult {
        let mut result = QueryResult::default();

        let blocks = self.graph.blocks_in(unit_index);
        for node in blocks {
            if let Some(block_info) = self.node_to_block_info(node) {
                result.primary.push(block_info.clone());
            }
        }

        result
    }

    /// Find all blocks that are related to a given block
    pub fn find_related(&self, name: &str) -> QueryResult {
        let mut result = QueryResult::default();

        // Find the primary block
        if let Some(primary_node) = self.graph.block_by_name(name) {
            if let Some(block_info) = self.node_to_block_info(primary_node) {
                result.primary.push(block_info.clone());
                result
                    .definitions
                    .insert(block_info.name.clone(), block_info);

                // Find all related blocks
                let related_blocks = self.graph.find_related_blocks(primary_node);
                for related_node in related_blocks {
                    if let Some(related_info) = self.node_to_block_info(related_node) {
                        result.related.push(related_info);
                    }
                }
            }
        }

        result
    }

    /// Find all blocks related to a given block recursively
    pub fn find_related_recursive(&self, name: &str) -> QueryResult {
        let mut result = QueryResult::default();

        // Find the primary block
        if let Some(primary_node) = self.graph.block_by_name(name) {
            if let Some(block_info) = self.node_to_block_info(primary_node) {
                result.primary.push(block_info.clone());
                result
                    .definitions
                    .insert(block_info.name.clone(), block_info);

                // Find all related blocks recursively
                let all_related = self.graph.find_related_blocks_recursive(primary_node);
                for related_node in all_related {
                    if let Some(related_info) = self.node_to_block_info(related_node) {
                        result.related.push(related_info);
                    }
                }
            }
        }

        result
    }

    /// Traverse graph with BFS from a starting block
    pub fn traverse_bfs(&self, start_name: &str) -> Vec<GraphBlockInfo> {
        let mut results = Vec::new();

        if let Some(start_node) = self.graph.block_by_name(start_name) {
            self.graph.traverse_bfs(start_node, |node| {
                if let Some(block_info) = self.node_to_block_info(node) {
                    results.push(block_info);
                }
            });
        }

        results
    }

    /// Traverse graph with DFS from a starting block
    pub fn traverse_dfs(&self, start_name: &str) -> Vec<GraphBlockInfo> {
        let mut results = Vec::new();

        if let Some(start_node) = self.graph.block_by_name(start_name) {
            self.graph.traverse_dfs(start_node, |node| {
                if let Some(block_info) = self.node_to_block_info(node) {
                    results.push(block_info);
                }
            });
        }

        results
    }

    /// Find all blocks by kind in a specific unit
    pub fn find_by_kind_in_unit(&self, kind: BlockKind, unit_index: usize) -> QueryResult {
        let mut result = QueryResult::default();

        let blocks = self.graph.blocks_by_kind_in(kind, unit_index);
        for node in blocks {
            if let Some(block_info) = self.node_to_block_info(node) {
                result.primary.push(block_info.clone());
            }
        }

        result
    }

    /// Helper: convert a GraphNode to block info
    fn node_to_block_info(&self, node: GraphNode) -> Option<GraphBlockInfo> {
        let (unit_index, name, kind) = self.graph.block_info(node.block_id)?;

        // Get file path from compile context
        let file_path = self
            .graph
            .cc
            .files
            .get(unit_index)
            .and_then(|file| file.path().map(|s| s.to_string()));

        Some(GraphBlockInfo {
            name: name.unwrap_or_else(|| format!("_unnamed_{}", node.block_id.0)),
            kind: format!("{:?}", kind),
            file_path,
            node,
        })
    }
}
