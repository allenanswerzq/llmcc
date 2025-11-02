pub mod call;
pub mod class;
pub mod function;
pub mod import;
pub mod origin;
pub mod variable;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_descriptor::{
    CallDescriptor, ClassDescriptor, DescriptorMeta, FunctionDescriptor, ImportDescriptor,
    LanguageDescriptorBuilder, VariableDescriptor,
};

pub struct PythonDescriptorBuilder;

impl<'tcx> LanguageDescriptorBuilder<'tcx> for PythonDescriptorBuilder {
    fn build_function_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<FunctionDescriptor> {
        function::build(unit, node, meta)
    }

    fn build_class_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<ClassDescriptor> {
        class::build(unit, node, meta)
    }

    fn build_variable_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<VariableDescriptor> {
        variable::build(unit, node, meta)
    }

    fn build_import_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<ImportDescriptor> {
        import::build(unit, node, meta)
    }

    fn build_call_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<CallDescriptor> {
        call::build(unit, node, meta)
    }
}
