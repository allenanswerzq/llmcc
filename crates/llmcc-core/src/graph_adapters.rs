//! Implementations of semantic graph-linking capabilities for built-in blocks.

use crate::block::{
    BasicBlock, BlockAlias, BlockBase, BlockCall, BlockClass, BlockConst, BlockEnum, BlockField,
    BlockFunc, BlockId, BlockImpl, BlockInterface, BlockParameter, BlockReturn, BlockTrait,
};
use crate::context::CompileUnit;
use crate::graph::ProjectGraph;
use crate::graph_semantics::{
    CallSiteBlock, CallableBlock, ContractBlock, GraphLinkBlock, ImplementationBlock,
    MemberFieldBlock, NominalTypeBlock, StructuralContractBlock, TypeAliasBlock, TypeRefBlock,
    VariantContainerBlock,
};
use crate::symbol::{SymId, Symbol};

impl<'tcx> GraphLinkBlock<'tcx> for BasicBlock<'tcx> {
    fn link_into_graph(
        &self,
        graph: &ProjectGraph<'tcx>,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
    ) {
        match self {
            BasicBlock::Func(func) => graph.link_callable(unit, block_id, *func),
            BasicBlock::Class(class) => graph.link_nominal_type(unit, block_id, *class),
            BasicBlock::Impl(implementation) => {
                graph.link_implementation(unit, block_id, *implementation)
            }
            BasicBlock::Trait(contract) => graph.link_contract(unit, block_id, *contract),
            BasicBlock::Interface(contract) => {
                graph.link_structural_contract(unit, block_id, *contract)
            }
            BasicBlock::Enum(enum_block) => graph.link_variant_container(block_id, *enum_block),
            BasicBlock::Call(call) => graph.link_call_site(block_id, *call),
            BasicBlock::Field(field) => graph.link_member_field(unit, block_id, *field),
            BasicBlock::Return(ret) => graph.link_type_ref(unit, block_id, *ret),
            BasicBlock::Parameter(param) => graph.link_type_ref(unit, block_id, *param),
            BasicBlock::Const(const_block) => graph.link_type_ref(unit, block_id, *const_block),
            BasicBlock::Alias(alias) => graph.link_alias(unit, block_id, *alias),
            _ => {}
        }
    }
}

impl<'blk> CallableBlock<'blk> for BlockFunc<'blk> {
    fn base(&self) -> &BlockBase<'blk> {
        BlockFunc::base(self)
    }

    fn symbol(&self) -> Option<&'blk Symbol> {
        BlockFunc::symbol(self)
    }

    fn parameters(&self) -> Vec<BlockId> {
        BlockFunc::parameters(self)
    }

    fn return_block(&self) -> Option<BlockId> {
        BlockFunc::return_block(self)
    }

    fn nested_type_ids(&self) -> Option<Vec<SymId>> {
        BlockFunc::nested_types(self)
    }

    fn decorator_ids(&self) -> Option<Vec<SymId>> {
        BlockFunc::decorators(self)
    }

    fn children(&self) -> Vec<BlockId> {
        BlockFunc::children(self)
    }

    fn add_type_dep(&self, type_id: BlockId) {
        BlockFunc::add_type_dep(self, type_id);
    }

    fn type_deps(&self) -> Vec<BlockId> {
        BlockFunc::type_deps(self).into_iter().collect()
    }

    fn add_func_dep(&self, func_id: BlockId) {
        BlockFunc::add_func_dep(self, func_id);
    }

    fn func_deps(&self) -> Vec<BlockId> {
        BlockFunc::func_deps(self).into_iter().collect()
    }
}

impl<'blk> NominalTypeBlock<'blk> for BlockClass<'blk> {
    fn symbol(&self) -> Option<&'blk Symbol> {
        BlockClass::symbol(self)
    }

    fn fields(&self) -> Vec<BlockId> {
        BlockClass::fields(self)
    }

    fn methods(&self) -> Vec<BlockId> {
        BlockClass::methods(self)
    }

    fn nested_type_ids(&self) -> Option<Vec<SymId>> {
        BlockClass::nested_types(self)
    }

    fn decorator_ids(&self) -> Option<Vec<SymId>> {
        BlockClass::decorators(self)
    }

    fn add_type_dep(&self, type_id: BlockId) {
        BlockClass::add_type_dep(self, type_id);
    }

    fn set_base_type(&self, name: String, block_id: Option<BlockId>) {
        BlockClass::set_extends(self, name, block_id);
    }
}

impl<'blk> ImplementationBlock for BlockImpl<'blk> {
    fn methods(&self) -> Vec<BlockId> {
        BlockImpl::methods(self)
    }

    fn resolved_target(&self) -> Option<BlockId> {
        BlockImpl::resolved_target(self)
    }

    fn set_target_ref(&self, target_id: BlockId) {
        BlockImpl::set_target_ref(self, target_id);
    }

    fn target_nested_type_ids(&self) -> Option<Vec<SymId>> {
        BlockImpl::target_nested_types(self)
    }

    fn resolved_contract(&self) -> Option<BlockId> {
        BlockImpl::resolved_trait(self)
    }

    fn set_contract_ref(&self, contract_id: BlockId) {
        BlockImpl::set_trait_ref(self, contract_id);
    }
}

impl<'blk> ContractBlock<'blk> for BlockTrait<'blk> {
    fn methods(&self) -> Vec<BlockId> {
        BlockTrait::methods(self)
    }

    fn symbol(&self) -> Option<&'blk Symbol> {
        BlockTrait::symbol(self)
    }
}

impl<'blk> ContractBlock<'blk> for BlockInterface<'blk> {
    fn methods(&self) -> Vec<BlockId> {
        BlockInterface::methods(self)
    }

    fn symbol(&self) -> Option<&'blk Symbol> {
        BlockInterface::symbol(self)
    }
}

impl<'blk> StructuralContractBlock<'blk> for BlockInterface<'blk> {
    fn fields(&self) -> Vec<BlockId> {
        BlockInterface::fields(self)
    }

    fn base_contract_ids(&self) -> Option<Vec<SymId>> {
        BlockInterface::nested_types(self)
    }

    fn add_base_contract(&self, name: String, block_id: Option<BlockId>) {
        BlockInterface::add_extends(self, name, block_id);
    }
}

impl<'blk> VariantContainerBlock for BlockEnum<'blk> {
    fn variant_fields(&self) -> Vec<BlockId> {
        BlockEnum::variants(self)
    }
}

impl<'blk> CallSiteBlock for BlockCall<'blk> {
    fn callee(&self) -> Option<BlockId> {
        BlockCall::callee(self)
    }
}

impl<'blk> TypeRefBlock<'blk> for BlockReturn<'blk> {
    fn base(&self) -> &BlockBase<'blk> {
        BlockReturn::base(self)
    }

    fn type_ref(&self) -> Option<BlockId> {
        BlockReturn::type_ref(self)
    }

    fn set_resolved_type_ref(&self, type_id: BlockId) {
        BlockReturn::set_type_ref(self, type_id);
    }
}

impl<'blk> TypeRefBlock<'blk> for BlockParameter<'blk> {
    fn base(&self) -> &BlockBase<'blk> {
        BlockParameter::base(self)
    }

    fn type_ref(&self) -> Option<BlockId> {
        BlockParameter::type_ref(self)
    }

    fn set_resolved_type_ref(&self, type_id: BlockId) {
        BlockParameter::set_type_ref(self, type_id);
    }
}

impl<'blk> TypeRefBlock<'blk> for BlockField<'blk> {
    fn base(&self) -> &BlockBase<'blk> {
        BlockField::base(self)
    }

    fn type_ref(&self) -> Option<BlockId> {
        BlockField::type_ref(self)
    }

    fn set_resolved_type_ref(&self, type_id: BlockId) {
        BlockField::set_type_ref(self, type_id);
    }
}

impl<'blk> TypeRefBlock<'blk> for BlockConst<'blk> {
    fn base(&self) -> &BlockBase<'blk> {
        BlockConst::base(self)
    }

    fn type_ref(&self) -> Option<BlockId> {
        BlockConst::type_ref(self)
    }

    fn set_resolved_type_ref(&self, type_id: BlockId) {
        BlockConst::set_type_ref(self, type_id);
    }
}

impl<'blk> MemberFieldBlock<'blk> for BlockField<'blk> {
    fn children(&self) -> Vec<BlockId> {
        BlockField::children(self)
    }
}

impl<'blk> TypeAliasBlock<'blk> for BlockAlias<'blk> {
    fn base(&self) -> &BlockBase<'blk> {
        BlockAlias::base(self)
    }
}
