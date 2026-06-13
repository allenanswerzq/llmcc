//! Project and unit graph structures.

use rayon::prelude::*;
use std::collections::BTreeMap;

use crate::block::{BasicBlock, BlockBase, BlockId, BlockRelation};
use crate::context::{CompileCtxt, CompileUnit};
use crate::graph_semantics::{
    CallSiteBlock, CallableBlock, ContractBlock, GraphLinkBlock, ImplementationBlock,
    MemberFieldBlock, NominalTypeBlock, StructuralContractBlock, TypeAliasBlock, TypeRefBlock,
    VariantContainerBlock,
};
use crate::symbol::{SymId, Symbol};

#[derive(Debug, Clone)]
pub struct UnitGraph {
    /// Compile unit this graph belongs to.
    unit_index: usize,
    /// Root block id of this unit.
    root: BlockId,
}

impl UnitGraph {
    pub fn new(unit_index: usize, root: BlockId) -> Self {
        Self { unit_index, root }
    }

    pub fn unit_index(&self) -> usize {
        self.unit_index
    }

    pub fn root(&self) -> BlockId {
        self.root
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitNode {
    pub unit_index: usize,
    pub block_id: BlockId,
}

/// Block-relation graph for a complete compilation project.
#[derive(Debug)]
pub struct ProjectGraph<'tcx> {
    /// Compilation context containing units, blocks, symbols, and relations.
    cc: &'tcx CompileCtxt<'tcx>,
    /// Per-unit roots, kept in unit-index order.
    units: Vec<UnitGraph>,
}

impl<'tcx> ProjectGraph<'tcx> {
    pub fn new(cc: &'tcx CompileCtxt<'tcx>) -> Self {
        Self {
            cc,
            units: Vec::new(),
        }
    }

    /// Return the compilation context backing this project graph.
    pub fn context(&self) -> &'tcx CompileCtxt<'tcx> {
        self.cc
    }

    /// Insert or replace one unit graph while preserving unit-index order.
    pub fn add_unit(&mut self, graph: UnitGraph) {
        match self
            .units
            .binary_search_by_key(&graph.unit_index(), UnitGraph::unit_index)
        {
            Ok(index) => self.units[index] = graph,
            Err(index) => self.units.insert(index, graph),
        }
    }

    /// Insert or replace multiple unit graphs.
    pub fn add_units(&mut self, graphs: impl IntoIterator<Item = UnitGraph>) {
        let mut units_by_index: BTreeMap<usize, UnitGraph> = self
            .units
            .drain(..)
            .map(|graph| (graph.unit_index(), graph))
            .collect();

        for graph in graphs {
            units_by_index.insert(graph.unit_index(), graph);
        }

        self.units = units_by_index.into_values().collect();
    }

    /// Return all unit graphs in unit-index order.
    pub fn units(&self) -> &[UnitGraph] {
        &self.units
    }

    /// Return the unit graph for a compile-unit index, if present.
    pub fn try_unit_graph(&self, index: usize) -> Option<&UnitGraph> {
        self.units
            .binary_search_by_key(&index, UnitGraph::unit_index)
            .ok()
            .map(|position| &self.units[position])
    }

    /// Link all unit graphs by discovering and recording block relationships.
    pub fn link_blocks(&self) {
        self.units.par_iter().for_each(|unit_graph| {
            let unit = self.cc.compile_unit(unit_graph.unit_index());
            let root_block = unit.block(unit_graph.root());
            self.link_subtree(&unit, &root_block, None);
        });
    }

    /// Link one block and then recursively link its children.
    fn link_subtree(
        &self,
        unit: &CompileUnit<'tcx>,
        block: &BasicBlock<'tcx>,
        parent: Option<BlockId>,
    ) {
        let block_id = block.id();

        if let Some(parent_id) = parent {
            self.add_contains(parent_id, block_id);
        }

        block.link_into_graph(self, unit, block_id);

        for child_id in block.children() {
            let child = unit.block(child_id);
            self.link_subtree(unit, &child, Some(block_id));
        }
    }

    /// Insert one directed relation.
    #[inline]
    fn insert_relation(&self, from: BlockId, relation: BlockRelation, to: BlockId) {
        self.cc.related_map.insert(from, relation, to);
    }

    /// Insert one directed relation and its inverse relation when defined.
    #[inline]
    fn insert_relation_pair(&self, from: BlockId, relation: BlockRelation, to: BlockId) {
        self.cc.related_map.insert_pair(from, relation, to);
    }

    /// Insert bidirectional parent-child containment relations.
    #[inline]
    fn add_contains(&self, parent: BlockId, child: BlockId) {
        self.insert_relation_pair(parent, BlockRelation::Contains, child);
    }

    /// Insert bidirectional caller-callee relations.
    #[inline]
    pub(crate) fn add_call(&self, caller: BlockId, callee: BlockId) {
        self.insert_relation_pair(caller, BlockRelation::Calls, callee);
    }

    /// Insert bidirectional type/value usage relations.
    #[inline]
    pub(crate) fn add_use(&self, user: BlockId, used: BlockId) {
        self.insert_relation_pair(user, BlockRelation::Uses, used);
    }

    /// Insert bidirectional typed-block/type-definition relations.
    #[inline]
    pub(crate) fn add_type_relation(&self, owner: BlockId, type_id: BlockId) {
        self.insert_relation_pair(owner, BlockRelation::TypeOf, type_id);
    }

    /// Insert one-way function/method parameter ownership relation.
    #[inline]
    pub(crate) fn add_parameter(&self, owner: BlockId, parameter: BlockId) {
        self.insert_relation(owner, BlockRelation::HasParameters, parameter);
    }

    /// Insert one-way function/method return ownership relation.
    #[inline]
    pub(crate) fn add_return(&self, owner: BlockId, return_block: BlockId) {
        self.insert_relation(owner, BlockRelation::HasReturn, return_block);
    }

    /// Insert bidirectional aggregate/member field ownership relations.
    #[inline]
    pub(crate) fn add_field(&self, owner: BlockId, field: BlockId) {
        self.insert_relation_pair(owner, BlockRelation::HasField, field);
    }

    /// Insert bidirectional member callable ownership relations.
    #[inline]
    pub(crate) fn add_method(&self, owner: BlockId, method: BlockId) {
        self.insert_relation_pair(owner, BlockRelation::HasMethod, method);
    }

    /// Insert bidirectional implementation/extension target relations.
    #[inline]
    pub(crate) fn add_implementation_target(&self, implementation: BlockId, target: BlockId) {
        self.insert_relation_pair(implementation, BlockRelation::ImplFor, target);
    }

    /// Insert bidirectional implementation-contract relations.
    #[inline]
    pub(crate) fn add_conformance(&self, implementer: BlockId, contract: BlockId) {
        self.insert_relation_pair(implementer, BlockRelation::Implements, contract);
    }

    /// Insert bidirectional type/contract specialization relations.
    #[inline]
    pub(crate) fn add_specialization(&self, derived: BlockId, base: BlockId) {
        self.insert_relation_pair(derived, BlockRelation::Extends, base);
    }

    /// Record type dependencies from symbol ids, resolving aliases/type links first.
    fn record_type_deps_from_symbols(
        &self,
        unit: &CompileUnit<'tcx>,
        target: &BlockBase<'tcx>,
        symbol_ids: impl IntoIterator<Item = SymId>,
    ) {
        for symbol_id in symbol_ids {
            if let Some(type_block_id) = unit.try_type_block_id(symbol_id) {
                target.add_type_dep(type_block_id);
            }
        }
    }

    /// Link bounds for type parameters declared in an owner symbol's scope.
    fn link_type_parameter_bounds(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        owner: Option<&Symbol>,
    ) {
        let Some(owner) = owner else {
            return;
        };

        let Some(scope_id) = owner.try_owned_scope() else {
            return;
        };

        unit.scope(scope_id).for_each_symbol(|symbol| {
            if !symbol.kind().is_type_parameter() {
                return;
            }

            if let Some(bound_block_id) = unit.try_type_of_block_id(symbol) {
                self.add_use(block_id, bound_block_id);
            }
        });
    }

    /// Link a typed block to its resolved type definition.
    pub(crate) fn link_type_ref(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        block: &impl TypeRefBlock<'tcx>,
    ) {
        if let Some(type_id) = block.type_ref() {
            self.add_type_relation(block_id, type_id);
            return;
        }

        if let Some(type_id) = unit.try_type_ref_block_id(block.base().symbol()) {
            block.set_resolved_type_ref(type_id);
            self.add_type_relation(block_id, type_id);
        }
    }

    /// Link a callable block.
    pub(crate) fn link_callable<C>(&self, unit: &CompileUnit<'tcx>, block_id: BlockId, callable: &C)
    where
        C: CallableBlock<'tcx> + ?Sized,
    {
        // Structural edges.
        for param_id in callable.parameters() {
            self.add_parameter(block_id, param_id);
        }
        if let Some(ret_id) = callable.return_block() {
            self.add_return(block_id, ret_id);
        }

        // Generic bounds and type annotations recorded on the function symbol.
        self.link_type_parameter_bounds(unit, block_id, callable.symbol());
        if let Some(nested_types) = callable.nested_type_ids() {
            self.record_type_deps_from_symbols(unit, callable.base(), nested_types);
        }

        // Decorator dependencies.
        if let Some(decorators) = callable.decorator_ids() {
            for decorator_id in decorators {
                if let Some(decorator_block_id) = unit.try_symbol_block_id(decorator_id) {
                    callable.add_type_dep(decorator_block_id);
                }
            }
        }

        // Call dependencies discovered from descendant blocks.
        for child_id in callable.children() {
            self.collect_call_dependencies(unit, callable, child_id);
        }

        // Emit dependency edges after collection so relation writes have one owner.
        for type_id in callable.type_deps() {
            self.add_use(block_id, type_id);
        }
        for func_id in callable.func_deps() {
            self.add_call(block_id, func_id);
        }
    }

    /// Collect call-derived dependencies from a function descendant subtree.
    fn collect_call_dependencies(
        &self,
        unit: &CompileUnit<'tcx>,
        caller: &(impl CallableBlock<'tcx> + ?Sized),
        block_id: BlockId,
    ) {
        let block = unit.block(block_id);

        if matches!(&block, BasicBlock::Func(_)) {
            return;
        }

        if let Some(callee_sym) = block.node().query(unit).try_resolved() {
            let is_call_block = matches!(&block, BasicBlock::Call(_));
            let is_call_target = callee_sym.kind().is_call_dependency_target();

            if is_call_block || is_call_target {
                self.record_callee_dependency(unit, caller, callee_sym);
            }
        }

        for child_id in block.children() {
            self.collect_call_dependencies(unit, caller, child_id);
        }
    }

    /// Record type and function dependencies implied by a callee symbol.
    fn record_callee_dependency(
        &self,
        unit: &CompileUnit<'tcx>,
        caller: &(impl CallableBlock<'tcx> + ?Sized),
        callee_sym: &Symbol,
    ) {
        let callee_kind = callee_sym.kind();
        let callee_block_id = callee_sym.block_id();

        if callee_kind.is_callable_body() {
            if callee_kind.has_call_receiver_type()
                && let Some(type_block_id) = unit.try_type_of_block_id(callee_sym)
            {
                caller.add_type_dep(type_block_id);
            }

            if let Some(callee_block_id) = callee_block_id {
                caller.add_func_dep(callee_block_id);
            }
        } else if callee_kind.is_constructable_type()
            && let Some(callee_block_id) = callee_block_id
        {
            caller.add_type_dep(callee_block_id);
        }
    }

    /// Link a nominal or aggregate type block.
    pub(crate) fn link_nominal_type(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        nominal: &impl NominalTypeBlock<'tcx>,
    ) {
        // Structural edges.
        for field_id in nominal.fields() {
            self.add_field(block_id, field_id);
        }
        for method_id in nominal.methods() {
            self.add_method(block_id, method_id);
        }

        // Base-type edge and display metadata.
        if let Some(symbol) = nominal.symbol() {
            if let Some((base_symbol, base_block_id)) = unit.try_type_of_with_block_id(symbol) {
                self.add_specialization(block_id, base_block_id);
                let base_name = unit.resolve_name(base_symbol.name);
                nominal.set_base_type(base_name, Some(base_block_id));
            }
        }

        // Nested type metadata can represent implemented contracts, constraints,
        // decorator targets, generic arguments, or other type-shaped dependencies.
        if let Some(nested) = nominal.nested_type_ids() {
            for type_id in nested {
                if let Some((type_sym, type_block_id)) = unit.try_symbol_with_block_id(type_id) {
                    nominal.add_type_dep(type_block_id);

                    let type_kind = type_sym.kind();
                    if type_kind.is_type_constraint() {
                        self.add_use(block_id, type_block_id);
                    } else if type_kind.is_implementation_contract() {
                        self.add_use(block_id, type_block_id);
                        self.add_conformance(block_id, type_block_id);
                    }
                }
            }
        }

        // Decorator dependencies.
        if let Some(decorators) = nominal.decorator_ids() {
            for decorator_id in decorators {
                if let Some(decorator_block_id) = unit.try_symbol_block_id(decorator_id) {
                    nominal.add_type_dep(decorator_block_id);
                    self.add_use(block_id, decorator_block_id);
                }
            }
        }
    }

    /// Link an implementation, extension, or conformance block.
    pub(crate) fn link_implementation<I>(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        implementation: &I,
    ) where
        I: ImplementationBlock + ?Sized,
    {
        // Structural edges.
        for method_id in implementation.methods() {
            self.add_method(block_id, method_id);
        }

        // Target type edge and target generic arguments.
        if let Some(target_id) = implementation.resolved_target() {
            implementation.set_target_ref(target_id);
            self.add_implementation_target(block_id, target_id);

            if let Some(nested_types) = implementation.target_nested_type_ids() {
                let target_block = unit.block(target_id);
                self.record_type_deps_from_symbols(unit, target_block.base(), nested_types);
            }
        }

        // Implemented contract edge.
        if let Some(contract_id) = implementation.resolved_contract() {
            implementation.set_contract_ref(contract_id);
            self.add_conformance(block_id, contract_id);
        }
    }

    /// Link a contract block.
    pub(crate) fn link_contract<C>(&self, unit: &CompileUnit<'tcx>, block_id: BlockId, contract: &C)
    where
        C: ContractBlock<'tcx> + ?Sized,
    {
        // Structural edges.
        for method_id in contract.methods() {
            self.add_method(block_id, method_id);
        }

        // Generic bounds.
        self.link_type_parameter_bounds(unit, block_id, contract.symbol());
    }

    /// Link a structural contract block.
    pub(crate) fn link_structural_contract<C>(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        contract: &C,
    ) where
        C: StructuralContractBlock<'tcx> + ?Sized,
    {
        self.link_contract(unit, block_id, contract);

        // Field ownership is an additional structural-contract capability.
        for field_id in contract.fields() {
            self.add_field(block_id, field_id);
        }

        // Extended contract edges and display metadata.
        if let Some(nested) = contract.base_contract_ids() {
            for base_type_id in nested {
                if let Some((base_sym, base_block_id)) = unit.try_symbol_with_block_id(base_type_id)
                {
                    self.add_specialization(block_id, base_block_id);

                    let base_name = unit.resolve_name(base_sym.name);
                    contract.add_base_contract(base_name, Some(base_block_id));
                }
            }
        }
    }

    /// Link a block that owns variant-like member fields.
    pub(crate) fn link_variant_container(
        &self,
        block_id: BlockId,
        block: &impl VariantContainerBlock,
    ) {
        for variant_id in block.variant_fields() {
            self.add_field(block_id, variant_id);
        }
    }

    /// Link a call-site block.
    pub(crate) fn link_call_site(&self, block_id: BlockId, call: &impl CallSiteBlock) {
        if let Some(callee_id) = call.callee() {
            self.add_call(block_id, callee_id);
        }
    }

    /// Link a field block to its type definition and nested fields.
    pub(crate) fn link_member_field(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        field: &impl MemberFieldBlock<'tcx>,
    ) {
        self.link_type_ref(unit, block_id, field);

        for child_id in field.children() {
            self.add_field(block_id, child_id);
        }
    }

    /// Link a type alias block to its target type definition.
    pub(crate) fn link_alias(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        alias: &impl TypeAliasBlock<'tcx>,
    ) {
        if let Some(type_id) = unit.try_type_ref_block_id(alias.base().symbol()) {
            self.add_type_relation(block_id, type_id);
        }
    }
}
