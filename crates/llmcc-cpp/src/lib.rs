//! C++ language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod infer;
mod token;

const CPP_PRIMITIVES: &[&str] = &[
    "int",
    "short",
    "long",
    "char",
    "signed",
    "unsigned",
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
    "float",
    "double",
    "bool",
    "_Bool",
    "void",
    "wchar_t",
    "char16_t",
    "char32_t",
    "char8_t",
    "auto",
    "nullptr_t",
];

/// C/C++ language implementation for llmcc parsing, collection, binding, and graph building.
pub use token::LangCpp;
