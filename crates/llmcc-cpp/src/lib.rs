//! C++ language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod infer;
mod pattern;
pub mod token;

pub use infer::infer_type;

/// C/C++ primitive types
pub const CPP_PRIMITIVES: &[&str] = &[
    // Integer types
    "int",
    "short",
    "long",
    "char",
    "signed",
    "unsigned",
    // Sized integer types (C99/C++11)
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "size_t",
    "ssize_t",
    "ptrdiff_t",
    "intptr_t",
    "uintptr_t",
    // Floating point types
    "float",
    "double",
    // Boolean
    "bool",
    "_Bool",
    // Void
    "void",
    // Wide character types
    "wchar_t",
    "char16_t",
    "char32_t",
    "char8_t",
    // C++ specific
    "auto",
    "nullptr_t",
];

pub use crate::bind::BinderVisitor;
pub use crate::collect::CollectorVisitor;

pub use llmcc_core::{
    CompileCtxt, ProjectGraph, build_llmcc_graph, build_llmcc_ir, print_llmcc_ir,
};
pub use token::LangCpp;
