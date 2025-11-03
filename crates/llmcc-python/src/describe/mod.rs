pub mod call;
pub mod class;
pub mod function;
pub mod import;
pub mod origin;
pub mod variable;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_descriptor::{
    CallDescriptor, ClassDescriptor, DescriptorTrait, FunctionDescriptor, ImportDescriptor,
    VariableDescriptor,
};

pub struct PythonDescriptorBuilder;

impl<'tcx> DescriptorTrait<'tcx> for PythonDescriptorBuilder {
    fn build_function(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<FunctionDescriptor> {
        function::build(unit, node)
    }

    fn build_impl(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ClassDescriptor> {
        class::build(unit, node)
    }

    fn build_variable(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<VariableDescriptor> {
        variable::build(unit, node)
    }

    fn build_import(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ImportDescriptor> {
        import::build(unit, node)
    }

    fn build_call(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<CallDescriptor> {
        call::build(unit, node)
    }
}
