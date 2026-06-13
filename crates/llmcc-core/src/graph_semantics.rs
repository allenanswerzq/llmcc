//! Semantic graph-linking capabilities.
//!
//! These traits describe graph behavior independently of today's concrete
//! [`BasicBlock`](crate::block::BasicBlock) storage variants. A language can
//! introduce a new block kind and opt into existing graph-linking behavior by
//! implementing the relevant capability, instead of changing `ProjectGraph`'s
//! linking algorithms.

use crate::block::{BlockBase, BlockId};
use crate::context::CompileUnit;
use crate::graph::ProjectGraph;
use crate::symbol::{SymId, Symbol};

/// Semantic capability for blocks that know how to link themselves into a graph.
///
/// This is the only place where concrete block storage needs dynamic dispatch.
/// `ProjectGraph` traversal calls this trait instead of matching every
/// [`BasicBlock`] variant itself.
pub(crate) trait GraphLinkBlock<'tcx> {
    fn link_into_graph(
        &self,
        graph: &ProjectGraph<'tcx>,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
    );
}

/// Semantic capability for blocks that own callable bodies.
pub(crate) trait CallableBlock<'blk> {
    fn base(&self) -> &BlockBase<'blk>;
    fn symbol(&self) -> Option<&'blk Symbol>;
    fn parameters(&self) -> Vec<BlockId>;
    fn return_block(&self) -> Option<BlockId>;
    fn nested_type_ids(&self) -> Option<Vec<SymId>>;
    fn decorator_ids(&self) -> Option<Vec<SymId>>;
    fn children(&self) -> Vec<BlockId>;
    fn add_type_dep(&self, type_id: BlockId);
    fn type_deps(&self) -> Vec<BlockId>;
    fn add_func_dep(&self, func_id: BlockId);
    fn func_deps(&self) -> Vec<BlockId>;
}

/// Semantic capability for nominal or aggregate type blocks.
pub(crate) trait NominalTypeBlock<'blk> {
    fn symbol(&self) -> Option<&'blk Symbol>;
    fn fields(&self) -> Vec<BlockId>;
    fn methods(&self) -> Vec<BlockId>;
    fn nested_type_ids(&self) -> Option<Vec<SymId>>;
    fn decorator_ids(&self) -> Option<Vec<SymId>>;
    fn add_type_dep(&self, type_id: BlockId);
    fn set_base_type(&self, name: String, block_id: Option<BlockId>);
}

/// Semantic capability for implementation, extension, or conformance blocks.
pub(crate) trait ImplementationBlock {
    fn methods(&self) -> Vec<BlockId>;
    fn resolved_target(&self) -> Option<BlockId>;
    fn set_target_ref(&self, target_id: BlockId);
    fn target_nested_type_ids(&self) -> Option<Vec<SymId>>;
    fn resolved_contract(&self) -> Option<BlockId>;
    fn set_contract_ref(&self, contract_id: BlockId);
}

/// Semantic capability for blocks that represent contracts or constraints.
pub(crate) trait ContractBlock<'blk> {
    fn methods(&self) -> Vec<BlockId>;
    fn symbol(&self) -> Option<&'blk Symbol>;
}

/// Semantic capability for contracts that also expose structural members and
/// can specialize other contracts.
pub(crate) trait StructuralContractBlock<'blk>: ContractBlock<'blk> {
    fn fields(&self) -> Vec<BlockId>;
    fn base_contract_ids(&self) -> Option<Vec<SymId>>;
    fn add_base_contract(&self, name: String, block_id: Option<BlockId>);
}

/// Semantic capability for blocks that expose variant-like member fields.
pub(crate) trait VariantContainerBlock {
    fn variant_fields(&self) -> Vec<BlockId>;
}

/// Semantic capability for blocks that represent call sites.
pub(crate) trait CallSiteBlock {
    fn callee(&self) -> Option<BlockId>;
}

/// Typed blocks that can cache a resolved type-definition block id.
pub(crate) trait TypeRefBlock<'blk> {
    fn base(&self) -> &BlockBase<'blk>;
    fn type_ref(&self) -> Option<BlockId>;
    fn set_resolved_type_ref(&self, type_id: BlockId);
}

/// Semantic capability for member fields that can contain nested fields.
pub(crate) trait MemberFieldBlock<'blk>: TypeRefBlock<'blk> {
    fn children(&self) -> Vec<BlockId>;
}

/// Semantic capability for type-alias blocks.
pub(crate) trait TypeAliasBlock<'blk> {
    fn base(&self) -> &BlockBase<'blk>;
}
