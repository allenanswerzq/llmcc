//! Python language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod token;

const PYTHON_PRIMITIVES: &[&str] = &[
    "int",
    "str",
    "float",
    "dict",
    "tuple",
    "set",
    "list",
    "None",
    "bool",
    "function",
    "__placeholder__",
];

/// Python language implementation for llmcc parsing, collection, binding, and graph building.
pub use token::LangPython;
