//! Renderer-ready graph facts collected from [`ProjectGraph`].
//!
//! The collector keeps renderer APIs small: callers get stable nodes, unique
//! edges, and display metadata without handling raw graph relations.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use rayon::prelude::*;

use crate::block_rel::BlockIndexEntry;
use crate::graph::ProjectGraph;
use crate::symbol::SymKind;
use crate::{BlockId, GraphQuery, UnitMeta};
use strum_macros::{Display, EnumString, IntoStaticStr};

/// Collected graph facts for downstream renderers and analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectedGraph {
    /// Visible graph nodes, sorted by source order.
    nodes: Vec<CollectedNode>,
    /// Unique semantic edges between visible nodes.
    edges: BTreeSet<CollectedEdge>,
}

impl CollectedGraph {
    /// Collect nodes and semantic edges from a project graph.
    pub fn new(project: &ProjectGraph) -> Self {
        let nodes = NodePass::new(project).all();
        let node_ids: HashSet<_> = nodes.iter().map(|node| node.block_id).collect();
        let edges = EdgePass::new(project, &node_ids).all();
        Self { nodes, edges }
    }

    /// Return true when no nodes were collected.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Return collected nodes in source order.
    pub fn nodes(&self) -> &[CollectedNode] {
        &self.nodes
    }

    /// Return collected edges between visible nodes.
    pub fn edges(&self) -> &BTreeSet<CollectedEdge> {
        &self.edges
    }

    /// Split the graph into owned node and edge collections.
    pub fn into_parts(self) -> (Vec<CollectedNode>, BTreeSet<CollectedEdge>) {
        (self.nodes, self.edges)
    }
}

struct NodePass<'p, 'tcx> {
    query: GraphQuery<'p, 'tcx>,
}

impl<'p, 'tcx> NodePass<'p, 'tcx> {
    fn new(project: &'p ProjectGraph<'tcx>) -> Self {
        Self {
            query: project.query(),
        }
    }

    fn all(&self) -> Vec<CollectedNode> {
        let mut nodes = self.query.context().par_blocks(|entry| {
            let order = entry.sort_key();
            self.node(entry).map(|node| (order, node))
        });

        nodes.sort_by_key(|(order, _)| *order);
        nodes.into_iter().map(|(_, node)| node).collect()
    }

    fn node(&self, entry: BlockIndexEntry) -> Option<CollectedNode> {
        if !self.query.is_collected_graph_node(entry.block_id) {
            return None;
        }

        let unit = self.query.context().compile_unit(entry.unit_index);
        let block = unit.block(entry.block_id);
        let root = unit.try_root_block()?;

        Some(CollectedNode {
            block_id: entry.block_id,
            name: entry.name_or_id(),
            unit_meta: root.unit_meta(),
            source_line: block.node().start_line(),
            symbol_kind: block.try_symbol_kind(),
        })
    }
}

struct EdgePass<'p, 'tcx> {
    query: GraphQuery<'p, 'tcx>,
    visible: &'p HashSet<BlockId>,
}

impl<'p, 'tcx> EdgePass<'p, 'tcx> {
    fn new(project: &'p ProjectGraph<'tcx>, visible: &'p HashSet<BlockId>) -> Self {
        Self {
            query: project.query(),
            visible,
        }
    }

    fn all(&self) -> BTreeSet<CollectedEdge> {
        let mut block_ids: Vec<_> = self.visible.iter().copied().collect();
        block_ids.sort();

        let edge_sets: Vec<_> = block_ids
            .into_par_iter()
            .map(|block_id| self.block_edges(block_id))
            .collect();

        let mut edges = EdgeSet::new();
        for edge_set in edge_sets {
            edges.extend(edge_set);
        }
        edges.into_inner()
    }

    fn block_edges(&self, block_id: BlockId) -> EdgeSet {
        let mut edges = EdgeSet::new();

        self.fields(block_id, &mut edges);
        self.calls(block_id, &mut edges);
        self.params(block_id, &mut edges);
        self.returns(block_id, &mut edges);
        self.conformance(block_id, &mut edges);
        self.specialization(block_id, &mut edges);
        self.type_deps(block_id, &mut edges);
        self.impl_args(block_id, &mut edges);
        self.annotations(block_id, &mut edges);

        edges
    }

    fn fields(&self, id: BlockId, edges: &mut EdgeSet) {
        for field in self.query.field_types(id) {
            for type_id in field.type_ids {
                if self.is_visible(type_id) {
                    edges.insert(type_id, id, CollectedEdgeKind::Field);
                }
            }

            self.field_args(id, field.field_id, edges);
        }
    }

    fn field_args(&self, id: BlockId, field_id: BlockId, edges: &mut EdgeSet) {
        let Some(args) = self.query.field_args(field_id) else {
            return;
        };
        if !self.is_visible(args.type_id) || args.type_id == id {
            return;
        }
        if !self.query.is_nominal_type(args.type_id) {
            return;
        }

        if args.includes_type {
            edges.remove(args.type_id, id, CollectedEdgeKind::Field);
            self.nested_fields(id, &args.arg_ids, edges);
        } else {
            self.type_args(id, args.type_id, &args.arg_ids, edges);
        }
    }

    fn nested_fields(&self, id: BlockId, arg_ids: &[BlockId], edges: &mut EdgeSet) {
        for arg_id in self.visible_types(arg_ids) {
            if arg_id != id && self.query.is_nominal_type(arg_id) {
                edges.insert(arg_id, id, CollectedEdgeKind::NestedField);
            }
        }
    }

    fn type_args(
        &self,
        id: BlockId,
        generic_type_id: BlockId,
        arg_ids: &[BlockId],
        edges: &mut EdgeSet,
    ) {
        for arg_id in self.visible_types(arg_ids) {
            if arg_id != id && arg_id != generic_type_id {
                edges.insert(arg_id, generic_type_id, CollectedEdgeKind::TypeArg);
            }
        }
    }

    fn calls(&self, block_id: BlockId, edges: &mut EdgeSet) {
        for callee_id in self.query.callees(block_id) {
            if self.is_visible(callee_id) {
                edges.insert(block_id, callee_id, CollectedEdgeKind::Call);
            }
        }
    }

    fn params(&self, block_id: BlockId, edges: &mut EdgeSet) {
        for type_id in self.query.param_types(block_id) {
            if self.is_visible(type_id) {
                edges.insert(type_id, block_id, CollectedEdgeKind::Param);
            }
        }
    }

    fn returns(&self, block_id: BlockId, edges: &mut EdgeSet) {
        for type_id in self.query.return_types(block_id) {
            if self.is_visible(type_id) {
                edges.insert(block_id, type_id, CollectedEdgeKind::Return);
            }
        }
    }

    fn conformance(&self, block_id: BlockId, edges: &mut EdgeSet) {
        for contract_id in self.query.contracts(block_id) {
            if self.is_visible(contract_id) {
                edges.insert(contract_id, block_id, CollectedEdgeKind::Conformance);
            }
        }
    }

    fn specialization(&self, block_id: BlockId, edges: &mut EdgeSet) {
        for parent_id in self.query.bases(block_id) {
            if self.is_visible(parent_id) {
                edges.insert(parent_id, block_id, CollectedEdgeKind::Specialization);
            }
        }
    }

    fn type_deps(&self, block_id: BlockId, edges: &mut EdgeSet) {
        for type_id in self.query.type_deps(block_id) {
            if self.is_visible(type_id) && !edges.contains_pair(block_id, type_id) {
                edges.insert(block_id, type_id, CollectedEdgeKind::TypeDep);
            }
        }
    }

    fn impl_args(&self, block_id: BlockId, edges: &mut EdgeSet) {
        for type_arg_id in self.query.impl_args(block_id) {
            if self.is_visible(type_arg_id) && !edges.contains(type_arg_id, block_id) {
                edges.insert(type_arg_id, block_id, CollectedEdgeKind::ImplArg);
            }
        }
    }

    fn annotations(&self, block_id: BlockId, edges: &mut EdgeSet) {
        for annotation_id in self.query.annotations(block_id) {
            if self.is_visible(annotation_id) {
                edges.insert(annotation_id, block_id, CollectedEdgeKind::Annotation);
            }
        }
    }

    fn visible_types(&self, type_ids: &[BlockId]) -> Vec<BlockId> {
        type_ids
            .iter()
            .copied()
            .filter(|&block_id| self.is_visible(block_id))
            .collect()
    }

    fn is_visible(&self, block_id: BlockId) -> bool {
        self.visible.contains(&block_id)
    }
}

#[derive(Default)]
struct EdgeSet {
    edges: BTreeSet<CollectedEdge>,
}

impl EdgeSet {
    fn new() -> Self {
        Self::default()
    }

    fn insert(&mut self, from_id: BlockId, to_id: BlockId, kind: CollectedEdgeKind) {
        if from_id != to_id {
            self.edges.insert(CollectedEdge::new(from_id, to_id, kind));
        }
    }

    fn remove(&mut self, from_id: BlockId, to_id: BlockId, kind: CollectedEdgeKind) {
        self.edges.remove(&CollectedEdge::new(from_id, to_id, kind));
    }

    fn contains_pair(&self, left: BlockId, right: BlockId) -> bool {
        self.edges.iter().any(|edge| {
            (edge.from_id == left && edge.to_id == right)
                || (edge.from_id == right && edge.to_id == left)
        })
    }

    fn contains(&self, from_id: BlockId, to_id: BlockId) -> bool {
        self.edges
            .iter()
            .any(|edge| edge.from_id == from_id && edge.to_id == to_id)
    }

    fn extend(&mut self, other: Self) {
        self.edges.extend(other.edges);
    }

    fn into_inner(self) -> BTreeSet<CollectedEdge> {
        self.edges
    }
}

/// Node selected for the collected graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectedNode {
    /// Source block id in the project graph.
    pub block_id: BlockId,
    /// Display name, such as `User` or `process`.
    pub name: String,
    /// Project/package/module/file metadata for this node's compile unit.
    pub unit_meta: UnitMeta,
    /// 1-based source line for the node.
    pub source_line: usize,
    /// Symbol kind used by renderers for shape selection.
    pub symbol_kind: Option<SymKind>,
}

impl CollectedNode {
    /// Source file path.
    pub fn path(&self) -> Option<&Path> {
        self.unit_meta.file_path.as_deref()
    }

    /// Source file path formatted for display.
    pub fn path_text(&self) -> Option<String> {
        self.path().map(display_path)
    }

    /// Source location formatted as `path:line`.
    pub fn location(&self) -> Option<String> {
        self.path_text()
            .map(|path| format!("{path}:{}", self.source_line))
    }

    /// Parent folder of the source file.
    pub fn dir(&self) -> Option<String> {
        self.path()?.parent().map(display_path)
    }

    /// Source file name.
    pub fn file_name(&self) -> Option<String> {
        self.path()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
            .or_else(|| self.unit_meta.file_name.clone())
    }

    /// Source file stem.
    pub fn file_stem(&self) -> Option<String> {
        self.path()
            .and_then(|path| path.file_stem())
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
            .or_else(|| self.unit_meta.file_name.clone())
    }

    /// Package name that owns this node.
    pub fn package(&self) -> Option<&str> {
        self.unit_meta.package_name.as_deref()
    }

    /// Package root folder.
    pub fn package_root(&self) -> Option<String> {
        self.unit_meta.package_root.as_deref().map(display_path)
    }

    /// Namespace or module path that owns this node.
    pub fn namespace(&self) -> Option<&str> {
        self.unit_meta.module_name.as_deref()
    }

    /// Namespace or module root folder.
    pub fn namespace_root(&self) -> Option<String> {
        self.unit_meta.module_root.as_deref().map(display_path)
    }
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

/// Semantic edge category in the collected graph.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Display, EnumString, IntoStaticStr,
)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub enum CollectedEdgeKind {
    /// Field type -> owning type.
    Field,
    /// Nested field type argument -> owning type.
    NestedField,
    /// Type argument -> generic type.
    TypeArg,
    /// Caller -> callee.
    Call,
    /// Parameter type -> function.
    Param,
    /// Function -> return type.
    Return,
    /// Contract -> conforming type.
    Conformance,
    /// Base type or contract -> specializing type.
    Specialization,
    /// Referencing body -> referenced type.
    TypeDep,
    /// Implementation type argument -> implementation target.
    ImplArg,
    /// Annotation or decorator -> annotated declaration.
    Annotation,
}

/// Edge in the collected graph.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CollectedEdge {
    /// Source node id.
    pub from_id: BlockId,
    /// Target node id.
    pub to_id: BlockId,
    /// Semantic edge category.
    pub kind: CollectedEdgeKind,
}

impl CollectedEdge {
    /// Create an edge from `from_id` to `to_id`.
    pub fn new(from_id: BlockId, to_id: BlockId, kind: CollectedEdgeKind) -> Self {
        Self {
            from_id,
            to_id,
            kind,
        }
    }
}
