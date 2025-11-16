pub mod arena;
pub mod block;
pub mod block_rel;
pub mod context;
pub mod file;
pub mod graph_builder;
pub(crate) mod graph_render;
pub mod interner;
pub mod ir;
pub mod ir_builder;
#[macro_use]
pub mod lang_def;
pub mod pagerank;
pub mod printer;
pub mod query;
pub mod scope;
pub mod symbol;
pub mod visit;

pub type DynError = Box<dyn std::error::Error + Send + Sync>;

pub use context::{CompileCtxt, CompileUnit};
pub use graph_builder::{
    BlockId, BlockRelation, GraphBuildConfig, GraphNode, ProjectGraph, UnitGraph, build_llmcc_graph,
};
pub use ir::HirId;
pub use ir_builder::{IrBuildOption, build_llmcc_ir};
pub use lang_def::{LanguageTrait, LanguageTraitImpl};
pub use pagerank::{PageRankConfig, PageRanker, RankedBlock};
pub use paste;
pub use printer::{PrintConfig, PrintFormat, print_llmcc_graph, print_llmcc_ir, render_llmcc_ir};
pub use query::{GraphBlockInfo, ProjectQuery, QueryResult};
pub use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};
