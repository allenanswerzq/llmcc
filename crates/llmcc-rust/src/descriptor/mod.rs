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
        function::from_hir(unit, node, fqn)
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
        structure::from_struct(unit, node, fqn)
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
        enumeration::from_enum(unit, node, fqn)
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
            "let_declaration" => Some(variable::from_let(
                unit,
                node,
                name.to_string(),
                fqn.to_string(),
            )),
            "const_item" => Some(variable::from_const_item(
                unit,
                node,
                name.to_string(),
                fqn.to_string(),
            )),
            "static_item" => Some(variable::from_static_item(
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
            Some(call::from_call(unit, node, enclosing))
        } else {
            Some(call::from_call(unit, node, None))
        }
    }
}
