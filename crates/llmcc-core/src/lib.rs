//! Core IR and graph building infrastructure for llmcc.

pub mod block;
pub mod block_rel;
pub mod bump;
pub mod context;
pub mod file;
pub mod graph;
pub mod graph_builder;
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
pub mod query;
pub mod resolve;
pub mod scope;
pub mod symbol;
pub mod visit;

pub use llmcc_error::{Error, ErrorKind, ErrorStatus, Result};

pub use block::{BasicBlock, BlockKind, BlockRelation};
pub use context::{CompileCtxt, CompileUnit, FileOrder};
pub use graph::{ProjectGraph, UnitGraph, UnitNode};
pub use graph_builder::{GraphBuildOptions, build_graphs};
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
pub use printer::{PrintConfig, PrintFormat, print_llmcc_graph, print_llmcc_ir, render_llmcc_ir};
pub use resolve::ResolveOptions;
// TODO: Re-enable after ProjectGraph query methods are implemented
// pub use query::{GraphBlockInfo, ProjectQuery, QueryResult};
pub use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};
