//! Rust language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod infer;
mod pattern;
mod token;

const RUST_PRIMITIVES: &[&str] = &[
    "i32", "i64", "i16", "i8", "i128", "isize", "u32", "u64", "u16", "u8", "u128", "usize", "f32",
    "f64", "bool", "char", "str", "String",
];

/// Rust language implementation for llmcc parsing, collection, binding, and graph building.
pub use token::LangRust;
