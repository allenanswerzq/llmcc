//! Core types for graph rendering.

use std::collections::BTreeMap;

use llmcc_core::BlockId;
use llmcc_core::block::BlockKind;
use llmcc_core::symbol::SymKind;

// Configuration

/// Block kinds to include in architecture graph:
/// - Types (Class, Trait, Interface, Enum) - the building blocks
/// - Free functions (Func) - entry points and pipelines
///
/// NOTE: Methods are EXCLUDED - they are implementation details of types.
/// NOTE: Fields are EXCLUDED - we only show type composition edges.
pub const ARCHITECTURE_KINDS: [BlockKind; 5] = [
    BlockKind::Class,
    BlockKind::Trait,
    BlockKind::Interface,
    BlockKind::Enum,
    BlockKind::Func,
];

// Component Depth

/// Component grouping depth for architecture graph visualization.
///
/// Controls the level of abstraction in the architecture graph:
/// - Lower depths show high-level relationships (project/crate dependencies)
/// - Higher depths show detailed relationships (individual types and functions)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComponentDepth {
    /// Project level - aggregate all nodes per project
    Project,
    /// Crate level - aggregate all nodes per crate
    Crate,
    /// Module level - aggregate all nodes per module
    Module,
    /// File level - show individual nodes with file clustering (default)
    #[default]
    File,
}

impl ComponentDepth {
    /// Convert from numeric depth (for CLI compatibility)
    pub fn from_number(n: usize) -> Self {
        match n {
            0 => Self::Project,
            1 => Self::Crate,
            2 => Self::Module,
            _ => Self::File,
        }
    }

    /// Convert to numeric depth
    pub fn as_number(&self) -> usize {
        match self {
            Self::Project => 0,
            Self::Crate => 1,
            Self::Module => 2,
            Self::File => 3,
        }
    }

    /// Check if this is an aggregated view (not showing individual nodes)
    pub fn is_aggregated(&self) -> bool {
        !matches!(self, Self::File)
    }

    /// Check if showing individual file-level detail
    pub fn shows_file_detail(&self) -> bool {
        matches!(self, Self::File)
    }
}

// Render Options

/// Options for graph rendering.
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// If true, show all nodes even those without edges.
    /// If false (default), only show nodes that have at least one edge.
    pub show_orphan_nodes: bool,
    /// If set, filter to only top K nodes by PageRank score.
    pub pagerank_top_k: Option<usize>,
    /// If true, cluster modules by their parent crate in module-level graphs.
    pub cluster_by_crate: bool,
    /// If true, use shortened labels (just module name instead of crate::module).
    pub short_labels: bool,
}

// Render Node & Edge

/// Node representation for rendering.
#[derive(Clone)]
pub struct RenderNode {
    pub block_id: BlockId,
    /// Display name (e.g., "User", "process")
    pub name: String,
    /// File location (e.g., "src/model/user.rs:42")
    pub location: Option<String>,
    /// Crate name from Cargo.toml (e.g., "sample")
    pub crate_name: Option<String>,
    /// Crate/package root folder path
    pub crate_root: Option<String>,
    /// Module path (e.g., "utils::helpers")
    pub module_path: Option<String>,
    /// Module root folder path
    pub module_root: Option<String>,
    /// File name (e.g., "lib.rs")
    pub file_name: Option<String>,
    /// Symbol kind (Struct, Trait, Enum, Function, Method)
    pub sym_kind: Option<SymKind>,
}

/// Edge with semantic labels.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RenderEdge {
    pub from_id: BlockId,
    pub to_id: BlockId,
    /// Semantic role of source (e.g., "caller", "struct")
    pub from_label: &'static str,
    /// Semantic role of target (e.g., "callee", "field")
    pub to_label: &'static str,
}

// Component Tree (for file-level detail rendering)

/// Hierarchical tree for organizing nodes by component path.
#[derive(Default)]
pub struct ComponentTree {
    /// Direct child nodes at this level (indices into nodes array)
    pub node_indices: Vec<usize>,
    /// Child component subtrees (name -> (level_type, subtree))
    pub children: BTreeMap<String, (String, ComponentTree)>,
}

impl ComponentTree {
    /// Insert a node at the given path.
    /// `path` is a list of (name, level_type) pairs.
    pub fn insert(&mut self, path: &[(String, &'static str)], node_idx: usize) {
        if path.is_empty() {
            self.node_indices.push(node_idx);
        } else {
            let (name, level_type) = &path[0];
            let child = self
                .children
                .entry(name.clone())
                .or_insert_with(|| (level_type.to_string(), ComponentTree::default()));
            child.1.insert(&path[1..], node_idx);
        }
    }
}

// Aggregated Node (for crate/module/project level)

/// An aggregated component node (represents a crate, module, or project).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AggregatedNode {
    /// Unique identifier for this component
    pub id: String,
    /// Display label
    pub label: String,
    /// Component type: "project", "crate", or "module"
    pub component_type: &'static str,
    /// Number of nodes aggregated into this component
    pub node_count: usize,
    /// Crate name (for clustering modules by crate)
    pub crate_name: Option<String>,
    /// Folder path for this component (for code agents to explore)
    pub folder: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_depth_conversion() {
        assert_eq!(ComponentDepth::from_number(0), ComponentDepth::Project);
        assert_eq!(ComponentDepth::from_number(1), ComponentDepth::Crate);
        assert_eq!(ComponentDepth::from_number(2), ComponentDepth::Module);
        assert_eq!(ComponentDepth::from_number(3), ComponentDepth::File);
        assert_eq!(ComponentDepth::from_number(99), ComponentDepth::File);
    }

    #[test]
    fn test_component_depth_properties() {
        assert!(ComponentDepth::Project.is_aggregated());
        assert!(ComponentDepth::Crate.is_aggregated());
        assert!(ComponentDepth::Module.is_aggregated());
        assert!(!ComponentDepth::File.is_aggregated());

        assert!(!ComponentDepth::Project.shows_file_detail());
        assert!(ComponentDepth::File.shows_file_detail());
    }
}
