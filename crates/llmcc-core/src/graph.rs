//! Project and unit graph structures.

use rayon::prelude::*;
use std::collections::BTreeMap;

use crate::block::{
    BasicBlock, BlockAlias, BlockBase, BlockCall, BlockClass, BlockConst, BlockEnum, BlockField,
    BlockFunc, BlockId, BlockImpl, BlockInterface, BlockParameter, BlockRelation, BlockReturn,
    BlockTrait,
};
use crate::context::{CompileCtxt, CompileUnit};
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

        match block {
            BasicBlock::Func(func) => self.link_func(unit, block_id, func),
            BasicBlock::Class(class) => self.link_class(unit, block_id, class),
            BasicBlock::Impl(impl_block) => self.link_impl(unit, block_id, impl_block),
            BasicBlock::Trait(trait_block) => self.link_trait(unit, block_id, trait_block),
            BasicBlock::Interface(iface_block) => self.link_interface(unit, block_id, iface_block),
            BasicBlock::Enum(enum_block) => self.link_enum(unit, block_id, enum_block),
            BasicBlock::Call(call) => self.link_call(unit, block_id, call),
            BasicBlock::Field(field) => self.link_field(unit, block_id, field),
            BasicBlock::Return(ret) => self.link_return(unit, block_id, ret),
            BasicBlock::Parameter(param) => self.link_parameter(unit, block_id, param),
            BasicBlock::Const(const_block) => self.link_const(unit, block_id, const_block),
            BasicBlock::Alias(alias) => self.link_alias(unit, block_id, alias),
            _ => {}
        }

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
    fn add_call(&self, caller: BlockId, callee: BlockId) {
        self.insert_relation_pair(caller, BlockRelation::Calls, callee);
    }

    /// Insert bidirectional type/value usage relations.
    #[inline]
    fn add_use(&self, user: BlockId, used: BlockId) {
        self.insert_relation_pair(user, BlockRelation::Uses, used);
    }

    /// Insert bidirectional typed-block/type-definition relations.
    #[inline]
    fn add_type_relation(&self, owner: BlockId, type_id: BlockId) {
        self.insert_relation_pair(owner, BlockRelation::TypeOf, type_id);
    }

    /// Insert one-way function/method parameter ownership relation.
    #[inline]
    fn add_parameter(&self, owner: BlockId, parameter: BlockId) {
        self.insert_relation(owner, BlockRelation::HasParameters, parameter);
    }

    /// Insert one-way function/method return ownership relation.
    #[inline]
    fn add_return(&self, owner: BlockId, return_block: BlockId) {
        self.insert_relation(owner, BlockRelation::HasReturn, return_block);
    }

    /// Insert bidirectional aggregate/member field ownership relations.
    #[inline]
    fn add_field(&self, owner: BlockId, field: BlockId) {
        self.insert_relation_pair(owner, BlockRelation::HasField, field);
    }

    /// Insert bidirectional member callable ownership relations.
    #[inline]
    fn add_method(&self, owner: BlockId, method: BlockId) {
        self.insert_relation_pair(owner, BlockRelation::HasMethod, method);
    }

    /// Insert bidirectional implementation-target relations.
    #[inline]
    fn add_impl_for(&self, impl_block: BlockId, target: BlockId) {
        self.insert_relation_pair(impl_block, BlockRelation::ImplFor, target);
    }

    /// Insert bidirectional implementation-contract relations.
    #[inline]
    fn add_implements(&self, implementer: BlockId, contract: BlockId) {
        self.insert_relation_pair(implementer, BlockRelation::Implements, contract);
    }

    /// Insert bidirectional type generalization relations.
    #[inline]
    fn add_extends(&self, derived: BlockId, base: BlockId) {
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
    fn link_type_ref(
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

    /// Link a function or method block.
    fn link_func(&self, unit: &CompileUnit<'tcx>, block_id: BlockId, func: &BlockFunc<'tcx>) {
        // Structural edges.
        for param_id in func.parameters() {
            self.add_parameter(block_id, param_id);
        }
        if let Some(ret_id) = func.return_block() {
            self.add_return(block_id, ret_id);
        }

        // Generic bounds and type annotations recorded on the function symbol.
        self.link_type_parameter_bounds(unit, block_id, func.symbol());
        if let Some(nested_types) = func.nested_types() {
            self.record_type_deps_from_symbols(unit, func.base(), nested_types);
        }

        // Decorator dependencies.
        if let Some(decorators) = func.decorators() {
            for decorator_id in decorators {
                if let Some(decorator_block_id) = unit.try_symbol_block_id(decorator_id) {
                    func.add_type_dep(decorator_block_id);
                }
            }
        }

        // Call dependencies discovered from descendant blocks.
        for child_id in func.children() {
            self.collect_call_dependencies(unit, func, child_id);
        }

        // Emit dependency edges after collection so relation writes have one owner.
        for type_id in func.type_deps() {
            self.add_use(block_id, type_id);
        }
        for type_id in func.func_deps() {
            self.add_call(block_id, type_id);
        }
    }

    /// Collect call-derived dependencies from a function descendant subtree.
    fn collect_call_dependencies(
        &self,
        unit: &CompileUnit<'tcx>,
        caller_func: &BlockFunc<'tcx>,
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
                self.record_callee_dependency(unit, caller_func, callee_sym);
            }
        }

        for child_id in block.children() {
            self.collect_call_dependencies(unit, caller_func, child_id);
        }
    }

    /// Record type and function dependencies implied by a callee symbol.
    fn record_callee_dependency(
        &self,
        unit: &CompileUnit<'tcx>,
        caller_func: &BlockFunc<'tcx>,
        callee_sym: &Symbol,
    ) {
        let callee_kind = callee_sym.kind();
        let callee_block_id = callee_sym.block_id();

        if callee_kind.is_callable_body() {
            if callee_kind.has_call_receiver_type()
                && let Some(type_block_id) = unit.try_type_of_block_id(callee_sym)
            {
                caller_func.add_type_dep(type_block_id);
            }

            if let Some(callee_block_id) = callee_block_id {
                caller_func.add_func_dep(callee_block_id);
            }
        } else if callee_kind.is_constructable_type()
            && let Some(callee_block_id) = callee_block_id
        {
            caller_func.add_type_dep(callee_block_id);
        }
    }

    /// Link a nominal type block.
    fn link_class(&self, unit: &CompileUnit<'tcx>, block_id: BlockId, class: &BlockClass<'tcx>) {
        // Structural edges.
        for field_id in class.fields() {
            self.add_field(block_id, field_id);
        }
        for method_id in class.methods() {
            self.add_method(block_id, method_id);
        }

        // Base-type edge and display metadata.
        if let Some(class_sym) = class.symbol() {
            if let Some((extends_sym, extends_block_id)) = unit.try_type_of_with_block_id(class_sym)
            {
                self.add_extends(block_id, extends_block_id);
                let extends_name = unit.resolve_name(extends_sym.name);
                class.set_extends(extends_name, Some(extends_block_id));
            }
        }

        // Nested type metadata can represent implemented contracts or type constraints.
        if let Some(nested) = class.nested_types() {
            for type_id in nested {
                if let Some((type_sym, type_block_id)) = unit.try_symbol_with_block_id(type_id) {
                    class.add_type_dep(type_block_id);

                    let type_kind = type_sym.kind();
                    if type_kind.is_type_constraint() {
                        self.add_use(block_id, type_block_id);
                    } else if type_kind.is_implementation_contract() {
                        self.add_use(block_id, type_block_id);
                        self.add_implements(block_id, type_block_id);
                    }
                }
            }
        }

        // Decorator dependencies.
        if let Some(decorators) = class.decorators() {
            for decorator_id in decorators {
                if let Some(decorator_block_id) = unit.try_symbol_block_id(decorator_id) {
                    class.add_type_dep(decorator_block_id);
                    self.add_use(block_id, decorator_block_id);
                }
            }
        }
    }

    /// Link an implementation block.
    fn link_impl(&self, unit: &CompileUnit<'tcx>, block_id: BlockId, impl_block: &BlockImpl<'tcx>) {
        // Structural edges.
        for method_id in impl_block.methods() {
            self.add_method(block_id, method_id);
        }

        // Target type edge and target generic arguments.
        if let Some(target_id) = impl_block.resolved_target() {
            impl_block.set_target_ref(target_id);
            self.add_impl_for(block_id, target_id);

            if let Some(nested_types) = impl_block.target_nested_types() {
                let target_block = unit.block(target_id);
                self.record_type_deps_from_symbols(unit, target_block.base(), nested_types);
            }
        }

        // Implemented contract edge.
        if let Some(contract_id) = impl_block.resolved_trait() {
            impl_block.set_trait_ref(contract_id);
            self.add_implements(block_id, contract_id);
        }
    }

    /// Link a contract/trait block.
    fn link_trait(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        trait_block: &BlockTrait<'tcx>,
    ) {
        // Structural edges.
        for method_id in trait_block.methods() {
            self.add_method(block_id, method_id);
        }

        // Generic bounds.
        self.link_type_parameter_bounds(unit, block_id, trait_block.symbol());
    }

    /// Link an interface-like contract block.
    fn link_interface(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        iface_block: &BlockInterface<'tcx>,
    ) {
        // Structural edges.
        for method_id in iface_block.methods() {
            self.add_method(block_id, method_id);
        }
        for field_id in iface_block.fields() {
            self.add_field(block_id, field_id);
        }

        // Extended contract edges and display metadata.
        if let Some(nested) = iface_block.nested_types() {
            for base_type_id in nested {
                if let Some((base_sym, base_block_id)) = unit.try_symbol_with_block_id(base_type_id)
                {
                    self.add_extends(block_id, base_block_id);

                    let base_name = unit.resolve_name(base_sym.name);
                    iface_block.add_extends(base_name, Some(base_block_id));
                }
            }
        }

        // Generic bounds.
        self.link_type_parameter_bounds(unit, block_id, iface_block.symbol());
    }

    /// Link an enum block.
    fn link_enum(
        &self,
        _unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        enum_block: &BlockEnum<'tcx>,
    ) {
        // Structural edges.
        for variant_id in enum_block.variants() {
            self.add_field(block_id, variant_id);
        }
    }

    /// Link a call-site block.
    fn link_call(&self, _unit: &CompileUnit<'tcx>, block_id: BlockId, call: &BlockCall<'tcx>) {
        if let Some(callee_id) = call.callee() {
            self.add_call(block_id, callee_id);
        }
    }

    /// Link a return block to its type definition.
    fn link_return(&self, unit: &CompileUnit<'tcx>, block_id: BlockId, ret: &BlockReturn<'tcx>) {
        self.link_type_ref(unit, block_id, ret);
    }

    /// Link a parameter block to its type definition.
    fn link_parameter(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        param: &BlockParameter<'tcx>,
    ) {
        self.link_type_ref(unit, block_id, param);
    }

    /// Link a field block to its type definition and nested fields.
    fn link_field(&self, unit: &CompileUnit<'tcx>, block_id: BlockId, field: &BlockField<'tcx>) {
        self.link_type_ref(unit, block_id, field);

        // Structural edges for enum variants with aggregate-style fields.
        for child_id in field.children() {
            self.add_field(block_id, child_id);
        }
    }

    /// Link a const block to its type definition.
    fn link_const(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        const_block: &BlockConst<'tcx>,
    ) {
        self.link_type_ref(unit, block_id, const_block);
    }

    /// Link a type alias block to its target type definition.
    fn link_alias(&self, unit: &CompileUnit<'tcx>, block_id: BlockId, alias: &BlockAlias<'tcx>) {
        if let Some(type_id) = unit.try_type_ref_block_id(alias.base().symbol()) {
            self.add_type_relation(block_id, type_id);
        }
    }
}

/// Typed blocks that can cache a resolved type-definition block id.
trait TypeRefBlock<'blk> {
    fn base(&self) -> &BlockBase<'blk>;
    fn type_ref(&self) -> Option<BlockId>;
    fn set_resolved_type_ref(&self, type_id: BlockId);
}

impl<'blk> TypeRefBlock<'blk> for BlockReturn<'blk> {
    fn base(&self) -> &BlockBase<'blk> {
        self.base()
    }

    fn type_ref(&self) -> Option<BlockId> {
        self.type_ref()
    }

    fn set_resolved_type_ref(&self, type_id: BlockId) {
        BlockReturn::set_type_ref(self, type_id);
    }
}

impl<'blk> TypeRefBlock<'blk> for BlockParameter<'blk> {
    fn base(&self) -> &BlockBase<'blk> {
        self.base()
    }

    fn type_ref(&self) -> Option<BlockId> {
        self.type_ref()
    }

    fn set_resolved_type_ref(&self, type_id: BlockId) {
        BlockParameter::set_type_ref(self, type_id);
    }
}

impl<'blk> TypeRefBlock<'blk> for BlockField<'blk> {
    fn base(&self) -> &BlockBase<'blk> {
        self.base()
    }

    fn type_ref(&self) -> Option<BlockId> {
        self.type_ref()
    }

    fn set_resolved_type_ref(&self, type_id: BlockId) {
        BlockField::set_type_ref(self, type_id);
    }
}

impl<'blk> TypeRefBlock<'blk> for BlockConst<'blk> {
    fn base(&self) -> &BlockBase<'blk> {
        self.base()
    }

    fn type_ref(&self) -> Option<BlockId> {
        self.type_ref()
    }

    fn set_resolved_type_ref(&self, type_id: BlockId) {
        BlockConst::set_type_ref(self, type_id);
    }
}
