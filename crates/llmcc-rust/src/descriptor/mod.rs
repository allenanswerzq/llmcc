pub mod call;
pub mod enumeration;
pub mod function;
pub mod structure;
pub mod variable;

pub use llmcc_descriptor::{
    CallArgument, CallChain, CallDescriptor, CallKind, CallSegment, CallSymbol, CallTarget,
    EnumDescriptor, EnumVariant, EnumVariantField, EnumVariantKind, FunctionDescriptor,
    FunctionParameter, FunctionQualifiers, ParameterKind, StructDescriptor, StructField,
    StructKind, TypeExpr, VariableDescriptor, VariableKind, VariableScope, Visibility,
};
