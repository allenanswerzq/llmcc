//! Rust language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod infer;
mod pattern;
pub mod token;

pub const RUST_PRIMITIVES: &[&str] = &[
    // Numeric types
    "i32", "i64", "i16", "i8", "i128", "isize", "u32", "u64", "u16", "u8", "u128", "usize", "f32",
    "f64", // Basic types
    "bool", "char", "str", "String",
];

pub use crate::bind::BinderVisitor;
pub use crate::collect::CollectorVisitor;

pub use llmcc_core::{
    CompileCtxt, ProjectGraph, build_llmcc_graph, build_llmcc_ir, print_llmcc_ir,
};
pub use token::LangRust;
