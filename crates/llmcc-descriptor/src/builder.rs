use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;

use crate::{
    call::CallKind, variable::VariableScope, CallDescriptor, ClassDescriptor, EnumDescriptor,
    FunctionDescriptor, ImportDescriptor, StructDescriptor, VariableDescriptor,
};

/// Additional metadata provided to descriptor builders.
#[derive(Debug, Clone, Default)]
pub enum DescriptorMeta<'a> {
    #[default]
    None,
    Function {
        fqn: Option<&'a str>,
    },
    Class {
        fqn: Option<&'a str>,
    },
    Struct {
        fqn: Option<&'a str>,
    },
    Enum {
        fqn: Option<&'a str>,
    },
    Variable {
        fqn: Option<&'a str>,
        name: Option<&'a str>,
        scope: Option<VariableScope>,
    },
    Import,
    Call {
        enclosing: Option<&'a str>,
        fqn: Option<&'a str>,
        kind_hint: Option<CallKind>,
    },
    Custom(&'a str),
}

/// Trait implemented by language front-ends to construct shared descriptors from their HIR nodes.
pub trait LanguageDescriptorBuilder<'tcx> {
    fn build_function_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<FunctionDescriptor> {
        let _ = unit;
        let _ = node;
        let _ = meta;
        None
    }

    fn build_class_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<ClassDescriptor> {
        let _ = unit;
        let _ = node;
        let _ = meta;
        None
    }

    fn build_struct_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<StructDescriptor> {
        let _ = unit;
        let _ = node;
        let _ = meta;
        None
    }

    fn build_enum_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<EnumDescriptor> {
        let _ = unit;
        let _ = node;
        let _ = meta;
        None
    }

    fn build_variable_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<VariableDescriptor> {
        let _ = unit;
        let _ = node;
        let _ = meta;
        None
    }

    fn build_import_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<ImportDescriptor> {
        let _ = unit;
        let _ = node;
        let _ = meta;
        None
    }

    fn build_call_descriptor(
        unit: CompileUnit<'tcx>,
        node: &HirNode<'tcx>,
        meta: DescriptorMeta<'_>,
    ) -> Option<CallDescriptor> {
        let _ = unit;
        let _ = node;
        let _ = meta;
        None
    }
}
