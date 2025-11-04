pub mod arena;
pub mod block;
pub mod block_rel;
pub mod context;
pub mod file;
pub mod graph_builder;
pub mod interner;
pub mod ir;
pub mod ir_builder;
pub mod lang_def;
pub mod module_path;
pub mod pagerank;
pub mod printer;
pub mod query;
pub mod symbol;
pub mod trie;
pub mod visit;

pub type DynError = Box<dyn std::error::Error + Send + Sync>;

pub use context::{CompileCtxt, CompileUnit};
pub use graph_builder::{
    build_llmcc_graph, BlockId, BlockRelation, GraphBuildConfig, GraphNode, ProjectGraph, UnitGraph,
};
pub use ir::HirId;
pub use ir_builder::{build_llmcc_ir, IrBuildConfig};
pub use lang_def::LanguageTrait;
pub use pagerank::{PageRankConfig, PageRanker, RankedBlock};
pub use paste;
pub use printer::{print_llmcc_graph, print_llmcc_ir};
pub use query::{GraphBlockInfo, ProjectQuery, QueryResult};
pub use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};
