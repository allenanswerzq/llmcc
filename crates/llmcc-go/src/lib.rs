//! Go language support for llmcc.
#[macro_use]
extern crate llmcc_core;

mod bind;
mod collect;
mod infer;
pub mod token;

pub use infer::infer_type;

pub const GO_PRIMITIVES: &[&str] = &[
    "any",
    "bool",
    "byte",
    "comparable",
    "complex64",
    "complex128",
    "error",
    "float32",
    "float64",
    "int",
    "int8",
    "int16",
    "int32",
    "int64",
    "rune",
    "string",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "uintptr",
];

pub use crate::bind::BinderVisitor;
pub use crate::collect::CollectorVisitor;

pub use llmcc_core::{
    CompileCtxt, ProjectGraph, UnitMeta, build_llmcc_graph, build_llmcc_ir, print_llmcc_ir,
};
pub use token::LangGo;

pub(crate) fn package_scope_name(meta: &UnitMeta) -> Option<String> {
    let file_path = meta.file_path.as_ref()?;
    let dir = file_path.parent()?;
    let mut parts = vec![format!("crate{}", meta.crate_index)];

    if let Some(root) = meta.package_root.as_ref()
        && let Ok(rel) = dir.strip_prefix(root)
    {
        for comp in rel.components() {
            if let Some(s) = comp.as_os_str().to_str()
                && !s.is_empty()
            {
                parts.push(sanitize_scope_part(s));
            }
        }
    } else if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
        parts.push(sanitize_scope_part(name));
    }

    Some(format!("go_pkg_{}", parts.join("_")))
}

fn sanitize_scope_part(part: &str) -> String {
    part.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
