//! Go language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod token;

const GO_PRIMITIVES: &[&str] = &[
    "nil",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "int8",
    "int16",
    "int32",
    "int64",
    "float32",
    "float64",
    "complex64",
    "complex128",
    "bool",
    "int",
    "byte",
    "rune",
    "string",
];

/// Go language implementation for llmcc parsing, collection, binding, and graph building.
pub use token::LangGo;
