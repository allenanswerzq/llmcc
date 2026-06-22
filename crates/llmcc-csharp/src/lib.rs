//! C# language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod token;

const CSHARP_PRIMITIVES: &[&str] = &[
    "void", "var", "bool", "byte", "sbyte", "char", "decimal", "double", "float", "int", "uint",
    "nint", "nuint", "long", "ulong", "short", "ushort", "object", "string", "dynamic", "null",
];

/// C# language implementation for llmcc parsing, collection, binding, and graph building.
pub use token::LangCSharp;
