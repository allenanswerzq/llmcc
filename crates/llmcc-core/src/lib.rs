//! Core IR and graph building infrastructure for llmcc.

pub mod arena;
pub mod block;
pub mod block_rel;
pub mod context;
pub mod file;
pub mod graph;
mod graph_adapters;
pub mod graph_builder;
pub mod graph_collect;
pub mod graph_query;
mod graph_semantics;
pub mod id;
pub mod interner;
pub mod ir;
pub mod ir_builder;
pub mod ir_query;
#[macro_use]
pub mod lang_def;
pub mod lang_registry;
pub mod meta;
pub mod pagerank;
pub mod printer;
pub mod resolve;
pub mod scope;
pub mod symbol;
pub mod visit;

pub use llmcc_error::{Error, ErrorKind, ErrorStatus, Result};

pub use block::{BasicBlock, BlockKind, BlockRelation};
pub use context::{CompileCtxt, CompileUnit, FileOrder};
pub use graph::{ProjectGraph, UnitGraph, UnitNode};
pub use graph_builder::{GraphBuildOptions, build_graphs};
pub use graph_collect::{
    AggregateVisitor, AggregatedEdge, AggregatedGraph, AggregatedGraphVisitor, AggregatedNode,
    AggregatedNodeKind, CollectedEdge, CollectedEdgeKind, CollectedGraph, CollectedGraphVisitor,
    CollectedNode,
};
pub use graph_query::{FieldArgs, FieldTypes, GraphQuery};
pub use id::{
    BlockId, HirId, ScopeId, SymId, SymbolId, next_hir_id, reset_block_id_counter,
    reset_hir_id_counter, reset_scope_id_counter, reset_symbol_id_counter,
};
pub use ir_builder::{HirBuildMetrics, HirBuildOptions, build_file_hir, build_hir};
pub use ir_query::HirQuery;
pub use lang_def::{HirBuildAction, Language, LanguageDefinition, NO_FIELD_ID, ParseChild};
pub use lang_registry::{LanguageHandler, LanguageHandlerImpl, LanguageRegistry};
pub use meta::{ArchitectureLevel, UnitMeta, UnitMetaIndex};
pub use paste;
pub use printer::{
    IrRender, PrintConfig, PrintFormat, print_block_tree, print_block_tree_with, print_ir,
    print_ir_with, render_block_tree, render_block_tree_with, render_ir, render_ir_with,
    write_block_tree, write_block_tree_with, write_ir, write_ir_with,
};
pub use resolve::ResolveOptions;
pub use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};
