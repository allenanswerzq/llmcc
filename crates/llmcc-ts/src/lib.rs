//! TypeScript language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod infer;
mod pattern;
pub mod token;

pub use infer::infer_type;

pub const TYPESCRIPT_PRIMITIVES: &[&str] = &[
    // Primitive types
    "string",
    "number",
    "boolean",
    "null",
    "undefined",
    "symbol",
    "bigint",
    "void",
    "never",
    "any",
    "unknown",
    "object",
    // Common built-in types
    "Array",
    "Object",
    "Function",
    "Promise",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Date",
    "RegExp",
    "Error",
];

pub use crate::bind::BinderVisitor;
pub use crate::collect::CollectorVisitor;

pub use llmcc_core::{
    CompileCtxt, ProjectGraph, build_llmcc_graph, build_llmcc_ir, print_llmcc_ir,
};
pub use token::LangTypeScript;
