//! JavaScript language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod token;

const JAVASCRIPT_PRIMITIVES: &[&str] = &[
    "__placeholder__",
    "bool",
    "number",
    "string",
    "function",
    "object",
];

/// JavaScript language implementation for llmcc parsing, collection, binding, and graph building.
pub use token::LangJavaScript;
