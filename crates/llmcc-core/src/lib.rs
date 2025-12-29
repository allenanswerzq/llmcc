pub mod block;
pub mod block_rel;
pub mod bump;
pub mod context;
pub mod file;
pub mod graph;
pub mod graph_builder;
pub mod graph_render;
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
pub use graph::{ProjectGraph, UnitGraph, UnitNode};
pub use graph_builder::{BlockId, BlockRelation, GraphBuildConfig, build_llmcc_graph};
pub use ir::HirId;
pub use ir_builder::{IrBuildOption, build_llmcc_ir, next_hir_id};
pub use lang_def::{LanguageTrait, LanguageTraitImpl};
pub use paste;
pub use printer::{PrintConfig, PrintFormat, print_llmcc_graph, print_llmcc_ir, render_llmcc_ir};
// TODO: Re-enable after ProjectGraph query methods are implemented
// pub use query::{GraphBlockInfo, ProjectQuery, QueryResult};
pub use symbol::{ScopeId, SymId};
pub use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};
