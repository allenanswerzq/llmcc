//! Core IR and graph building infrastructure for llmcc.

pub mod block;
pub mod block_rel;
pub mod bump;
pub mod context;
pub mod file;
pub mod graph;
pub mod graph_builder;
pub mod hir_query;
pub mod id;
pub mod interner;
pub mod ir;
pub mod ir_builder;
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

pub use context::{CompileCtxt, CompileUnit, FileOrder};
pub use graph::{ProjectGraph, UnitGraph, UnitNode};
pub use graph_builder::{BlockRelation, GraphBuildConfig, build_llmcc_graph};
pub use hir_query::HirQuery;
pub use id::{
    BlockId, HirId, ScopeId, SymId, SymbolId, next_hir_id, reset_block_id_counter,
    reset_hir_id_counter, reset_scope_id_counter, reset_symbol_id_counter,
};
pub use ir_builder::{
    IrBuildOption, build_llmcc_ir, build_llmcc_ir_inner, get_ir_build_cpu_time_ms,
    reset_ir_build_counters,
};
pub use lang_def::{Language, LanguageHooks, NO_FIELD_ID, ParseChild};
pub use lang_registry::{LanguageHandler, LanguageHandlerImpl, LanguageRegistry};
pub use meta::{ArchDepth, UnitMeta, UnitMetaBuilder};
pub use paste;
pub use printer::{PrintConfig, PrintFormat, print_llmcc_graph, print_llmcc_ir, render_llmcc_ir};
pub use resolve::ResolveOptions;
// TODO: Re-enable after ProjectGraph query methods are implemented
// pub use query::{GraphBlockInfo, ProjectQuery, QueryResult};
pub use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};
