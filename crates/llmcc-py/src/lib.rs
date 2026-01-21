//! Python language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod infer;
mod pattern;
pub mod token;

pub const PYTHON_PRIMITIVES: &[&str] = &[
    // Numeric types
    "int",
    "float",
    "complex",
    // Basic types
    "bool",
    "str",
    "bytes",
    "None",
    // Collection types
    "list",
    "tuple",
    "dict",
    "set",
    "frozenset",
    "range",
    // Common built-ins
    "object",
    "type",
    "callable",
    "iter",
];

pub use crate::bind::BinderVisitor;
pub use crate::collect::CollectorVisitor;

pub use llmcc_core::{
    CompileCtxt, ProjectGraph, build_llmcc_graph, build_llmcc_ir, print_llmcc_ir,
};
pub use token::LangPython;
