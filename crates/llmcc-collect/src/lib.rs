//! Node and edge collection for graph rendering.
//!
//! This crate provides format-agnostic types and collection logic for
//! graph visualization. It transforms a `ProjectGraph` into renderable
//! nodes and edges that can be consumed by various renderers (DOT, SVG, etc.).
//!
//! # Module Structure
//!
//! - [`types`]: Core types (ComponentDepth, RenderNode, RenderEdge, etc.)
//! - [`collect`]: Node and edge collection from ProjectGraph

mod collect;
mod types;

pub use collect::{collect_edges, collect_nodes};
pub use types::{
    ARCHITECTURE_KINDS, AggregatedNode, ComponentDepth, ComponentTree, RenderEdge, RenderNode,
    RenderOptions,
};
