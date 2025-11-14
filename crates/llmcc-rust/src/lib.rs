#[macro_use]
extern crate llmcc_core;

// mod bind;
mod collect;
pub mod util;
pub mod token;

// pub use crate::bind::bind_symbols;
pub use crate::collect::DeclVisitor;
pub use llmcc_core::{
    CompileCtxt, ProjectGraph, build_llmcc_graph, build_llmcc_ir,
    print_llmcc_ir,
};
pub use token::LangRust;
