//! DOT renderer options and component aggregation types.

use std::collections::BTreeMap;

use strum_macros::{Display, EnumString, FromRepr, IntoStaticStr};

/// Component grouping depth for architecture graph visualization.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumString, FromRepr, IntoStaticStr,
)]
#[repr(usize)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub enum ComponentDepth {
    /// Project level - aggregate all nodes per project.
    #[strum(serialize = "project", serialize = "0")]
    Project,
    /// Package level - aggregate all nodes per package or library boundary.
    #[strum(serialize = "package", serialize = "1")]
    Package,
    /// Namespace level - aggregate all nodes per module, namespace, or equivalent grouping.
    #[strum(serialize = "namespace", serialize = "2")]
    Namespace,
    /// File level - show individual nodes with file clustering.
    #[default]
    #[strum(serialize = "file", serialize = "3")]
    File,
}

impl ComponentDepth {
    pub fn is_aggregated(self) -> bool {
        !matches!(self, Self::File)
    }

    pub fn shows_file_detail(self) -> bool {
        matches!(self, Self::File)
    }
}

impl From<usize> for ComponentDepth {
    fn from(value: usize) -> Self {
        Self::from_repr(value).unwrap_or_default()
    }
}

impl From<ComponentDepth> for usize {
    fn from(value: ComponentDepth) -> Self {
        value as usize
    }
}

/// Options for DOT graph rendering.
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// If true, show all nodes even those without edges.
    pub show_orphan_nodes: bool,
    /// If set, filter to only top K nodes by PageRank score.
    pub pagerank_top_k: Option<usize>,
    /// If true, cluster namespaces by their parent package in namespace-level graphs.
    pub cluster_by_package: bool,
    /// If true, use shortened labels for namespace-level graphs.
    pub short_labels: bool,
}

/// Hierarchical tree for organizing file-level DOT clusters.
#[derive(Default)]
pub(crate) struct ComponentTree {
    pub(crate) node_indices: Vec<usize>,
    pub(crate) children: BTreeMap<String, (String, ComponentTree)>,
}

impl ComponentTree {
    pub(crate) fn insert(&mut self, path: &[(String, &'static str)], node_idx: usize) {
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

/// Aggregated component node for project/package/namespace DOT views.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct AggregatedNode {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) component_type: &'static str,
    pub(crate) node_count: usize,
    pub(crate) package_name: Option<String>,
    pub(crate) folder: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_depth_numeric_conversions() {
        assert_eq!(ComponentDepth::from(0), ComponentDepth::Project);
        assert_eq!(ComponentDepth::from(1), ComponentDepth::Package);
        assert_eq!(ComponentDepth::from(2), ComponentDepth::Namespace);
        assert_eq!(ComponentDepth::from(3), ComponentDepth::File);
        assert_eq!(ComponentDepth::from(99), ComponentDepth::File);

        assert_eq!(usize::from(ComponentDepth::Project), 0);
        assert_eq!(usize::from(ComponentDepth::Package), 1);
        assert_eq!(usize::from(ComponentDepth::Namespace), 2);
        assert_eq!(usize::from(ComponentDepth::File), 3);
    }

    #[test]
    fn component_depth_string_conversions_are_derived() {
        assert_eq!(ComponentDepth::Project.to_string(), "project");
        assert_eq!(
            "project".parse::<ComponentDepth>(),
            Ok(ComponentDepth::Project)
        );
        assert_eq!(ComponentDepth::Package.to_string(), "package");
        assert_eq!("1".parse::<ComponentDepth>(), Ok(ComponentDepth::Package));
        assert_eq!(ComponentDepth::Namespace.to_string(), "namespace");
        assert_eq!(
            "namespace".parse::<ComponentDepth>(),
            Ok(ComponentDepth::Namespace)
        );
    }
}
