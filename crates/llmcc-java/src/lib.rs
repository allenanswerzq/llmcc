//! Java language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod token;

const JAVA_PRIMITIVES: &[&str] = &[
    "void", "byte", "short", "int", "long", "float", "double", "boolean", "char", "string", "null",
];

/// Java language implementation for llmcc parsing, collection, binding, and graph building.
pub use token::LangJava;
