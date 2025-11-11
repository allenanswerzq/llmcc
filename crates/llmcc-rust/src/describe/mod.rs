pub mod call;
pub mod enumeration;
pub mod function;
pub mod implementation;
pub mod module;
pub mod parameter;
pub mod structure;
pub mod variable;

use llmcc_core::Node;
use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_descriptor::{DescriptorTrait, ImplDescriptor};

pub use llmcc_descriptor::{
    CallArgument, CallChain, CallDescriptor, CallKind, CallSegment, CallSymbol, CallTarget,
    ClassDescriptor, EnumDescriptor, EnumVariant, EnumVariantField, EnumVariantKind,
    FunctionDescriptor, FunctionParameter, FunctionQualifiers, ModuleDescriptor, ParameterKind,
    StructDescriptor, StructField, StructKind, TypeExpr, VariableDescriptor, VariableKind,
    VariableScope, Visibility,
};

pub struct RustDescriptor;

impl<'tcx> DescriptorTrait<'tcx> for RustDescriptor {
    fn build_function(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<FunctionDescriptor> {
        function::build(unit, node)
    }

    fn build_struct(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<StructDescriptor> {
        structure::build(unit, node)
    }

    fn build_enum(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<EnumDescriptor> {
        enumeration::build(unit, node)
    }

    fn build_module(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ModuleDescriptor> {
        module::build(unit, node)
    }

    fn build_variable(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<VariableDescriptor> {
        let kind = node.inner_ts_node().kind();
        match kind {
            "let_declaration" => variable::build_let(unit, node),
            "const_item" => variable::build_const_item(unit, node),
            "static_item" => variable::build_static_item(unit, node),
            _ => None,
        }
    }

    fn build_call(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<CallDescriptor> {
        Some(call::build(unit, node, None))
    }

    fn build_impl(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ImplDescriptor> {
        implementation::build(unit, node)
    }

    fn build_type_expr(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> TypeExpr {
        function::parse_type_expr(unit, node)
    }
}
