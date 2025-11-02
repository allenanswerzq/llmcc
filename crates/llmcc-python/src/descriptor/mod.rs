pub mod call;
pub mod origin;

pub use call::build_call_descriptor;
pub use llmcc_descriptor::{
    CallArgument, CallChain, CallDescriptor, CallKind, CallSegment, CallSymbol, CallTarget,
    ClassDescriptor, ClassField, FunctionDescriptor, FunctionParameter, FunctionQualifiers,
    ImportDescriptor, ImportKind, TypeExpr, VariableDescriptor, VariableKind, VariableScope,
};
pub use origin::build_origin;
