//! Node and edge collection for graph rendering.

mod collect;
mod types;

pub use collect::{collect_edges, collect_nodes};
pub use types::{
    ARCHITECTURE_KINDS, AggregatedNode, ComponentDepth, ComponentTree, RenderEdge, RenderNode,
    RenderOptions,
};
