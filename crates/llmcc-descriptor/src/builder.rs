use llmcc_core::Node;
use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;

use crate::{
    CallDescriptor, EnumDescriptor, FunctionDescriptor, ImplDescriptor, ImportDescriptor,
    ModuleDescriptor, StructDescriptor, TypeExpr, VariableDescriptor,
};

/// Trait implemented by language front-ends to construct shared descriptors from their HIR nodes.
/// All methods take only unit and node; callers assign metadata (fqn, scope, etc.) after building.
pub trait DescriptorTrait<'tcx> {
    fn build_function(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<FunctionDescriptor> {
        let _ = unit;
        let _ = node;
        None
    }

    fn build_impl(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ImplDescriptor> {
        let _ = unit;
        let _ = node;
        None
    }

    fn build_struct(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<StructDescriptor> {
        let _ = unit;
        let _ = node;
        None
    }

    fn build_enum(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<EnumDescriptor> {
        let _ = unit;
        let _ = node;
        None
    }

    fn build_module(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ModuleDescriptor> {
        let _ = unit;
        let _ = node;
        None
    }

    fn build_variable(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<VariableDescriptor> {
        let _ = unit;
        let _ = node;
        None
    }

    fn build_import(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<ImportDescriptor> {
        let _ = unit;
        let _ = node;
        None
    }

    fn build_call(unit: CompileUnit<'tcx>, node: &HirNode<'tcx>) -> Option<CallDescriptor> {
        let _ = unit;
        let _ = node;
        None
    }

    fn build_type_expr(unit: CompileUnit<'tcx>, node: Node<'tcx>) -> TypeExpr {
        TypeExpr::Unknown(unit.get_text(node.start_byte(), node.end_byte()))
    }
}
