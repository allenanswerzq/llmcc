mod bind;
mod collect;
pub mod descriptor;
pub mod token;

pub use crate::bind::bind_symbols;
pub use crate::collect::{collect_symbols, CollectionResult};
pub use crate::descriptor::{
    CallArgument, CallDescriptor, CallTarget, EnumDescriptor, EnumVariant, EnumVariantField,
    EnumVariantKind, FnVisibility, FunctionDescriptor, FunctionParameter, StructDescriptor,
    StructField, StructKind, TypeExpr, VariableDescriptor, VariableKind, VariableScope,
};
pub use llmcc_core::*;
pub use token::LangRust;
