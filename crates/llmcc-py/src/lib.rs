#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod infer;
pub mod token;
mod util;

pub const PYTHON_BUILTINS: &[&str] = &[
    // Numeric types
    "int", "float", "complex",
    // Sequence types
    "str", "bytes", "bytearray", "list", "tuple", "range",
    // Set types
    "set", "frozenset",
    // Mapping type
    "dict",
    // Boolean
    "bool",
    // None type
    "None", "NoneType",
    // Callable
    "callable",
    // Type
    "type", "object",
    // Built-in functions (commonly used)
    "len", "print", "open", "input", "range", "enumerate", "zip", "map", "filter",
    "sorted", "reversed", "sum", "min", "max", "abs", "round", "pow",
    "isinstance", "issubclass", "hasattr", "getattr", "setattr", "delattr",
    "id", "hash", "repr", "str", "int", "float", "bool", "list", "dict", "set", "tuple",
    "iter", "next", "super", "property", "classmethod", "staticmethod",
];

pub use crate::bind::BinderVisitor;
pub use crate::collect::CollectorVisitor;

pub use llmcc_core::{
    CompileCtxt, ProjectGraph, build_llmcc_graph, build_llmcc_ir, print_llmcc_ir,
};
pub use token::LangPython;
