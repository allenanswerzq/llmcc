pub mod call;
pub mod enumeration;
pub mod function;
pub mod structure;
pub mod variable;

use llmcc_descriptor::{DescriptorMeta, LanguageDescriptorBuilder};

pub use llmcc_descriptor::{
    CallArgument, CallChain, CallDescriptor, CallKind, CallSegment, CallSymbol, CallTarget,
    EnumDescriptor, EnumVariant, EnumVariantField, EnumVariantKind, FunctionDescriptor,
    FunctionParameter, FunctionQualifiers, ParameterKind, StructDescriptor, StructField,
    StructKind, TypeExpr, VariableDescriptor, VariableKind, VariableScope, Visibility,
};

pub struct RustDescriptorBuilder;

impl<'tcx> LanguageDescriptorBuilder<'tcx> for RustDescriptorBuilder {
    fn build_function_descriptor(
        unit: llmcc_core::context::CompileUnit<'tcx>,
        node: &llmcc_core::ir::HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<FunctionDescriptor> {
        let fqn = match meta {
            DescriptorMeta::Function { fqn } => fqn,
            _ => None,
        };
        function::build(unit, node, fqn)
    }

    fn build_struct_descriptor(
        unit: llmcc_core::context::CompileUnit<'tcx>,
        node: &llmcc_core::ir::HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<StructDescriptor> {
        let fqn = match meta {
            DescriptorMeta::Struct { fqn } => fqn,
            _ => None,
        };
        structure::build(unit, node, fqn)
    }

    fn build_enum_descriptor(
        unit: llmcc_core::context::CompileUnit<'tcx>,
        node: &llmcc_core::ir::HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<EnumDescriptor> {
        let fqn = match meta {
            DescriptorMeta::Enum { fqn } => fqn,
            _ => None,
        };
        enumeration::build(unit, node, fqn)
    }

    fn build_variable_descriptor(
        unit: llmcc_core::context::CompileUnit<'tcx>,
        node: &llmcc_core::ir::HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<VariableDescriptor> {
        let (fqn, name) = match meta {
            DescriptorMeta::Variable { fqn, name, .. } => (fqn, name),
            _ => (None, None),
        };

        let fqn = fqn?;
        let name = name?;
        let kind = node.inner_ts_node().kind();
        match kind {
            "let_declaration" => Some(variable::build_let(
                unit,
                node,
                name.to_string(),
                fqn.to_string(),
            )),
            "const_item" => Some(variable::build_const_item(
                unit,
                node,
                name.to_string(),
                fqn.to_string(),
            )),
            "static_item" => Some(variable::build_static_item(
                unit,
                node,
                name.to_string(),
                fqn.to_string(),
            )),
            _ => None,
        }
    }

    fn build_call_descriptor(
        unit: llmcc_core::context::CompileUnit<'tcx>,
        node: &llmcc_core::ir::HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<CallDescriptor> {
        if let DescriptorMeta::Call { enclosing, .. } = meta {
            Some(call::build(unit, node, enclosing))
        } else {
            Some(call::build(unit, node, None))
        }
    }
}
