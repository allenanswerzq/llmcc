use rayon::prelude::*;

use crate::block::{BasicBlock, BlockId, BlockRelation};
use crate::context::{CompileCtxt, CompileUnit};

#[derive(Debug, Clone)]
pub struct UnitGraph {
    /// Compile unit this graph belongs to
    unit_index: usize,
    /// Root block ID of this unit
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

/// ProjectGraph represents a complete compilation project with all units
/// and their inter-dependencies.
#[derive(Debug)]
pub struct ProjectGraph<'tcx> {
    /// Reference to the compilation context containing all symbols
    pub cc: &'tcx CompileCtxt<'tcx>,
    /// Per-unit graphs containing blocks and intra-unit relations
    units: Vec<UnitGraph>,
}

impl<'tcx> ProjectGraph<'tcx> {
    pub fn new(cc: &'tcx CompileCtxt<'tcx>) -> Self {
        Self {
            cc,
            units: Vec::new(),
        }
    }

    pub fn add_child(&mut self, graph: UnitGraph) {
        self.units.push(graph);
        self.units.sort_by_key(|g| g.unit_index());
    }

    /// Add multiple unit graphs to the project graph.
    pub fn add_children(&mut self, graphs: Vec<UnitGraph>) {
        self.units.extend(graphs);
        self.units.sort_by_key(|g| g.unit_index());
    }

    /// Get the units in this project graph.
    pub fn units(&self) -> &[UnitGraph] {
        &self.units
    }

    /// Get a specific unit graph by index, if it exists.
    pub fn unit_graph(&self, index: usize) -> Option<&UnitGraph> {
        self.units.iter().find(|u| u.unit_index() == index)
    }

    /// Get top-k limit (currently always None - no PageRank filtering).
    pub fn top_k(&self) -> Option<usize> {
        None
    }

    /// Check if PageRank ranking is enabled (currently always false).
    pub fn pagerank_enabled(&self) -> bool {
        false
    }

    /// Connect all blocks by discovering and recording their relationships.
    pub fn connect_blocks(&self) {
        // Process each unit in parallel - they are independent
        self.units.par_iter().for_each(|unit_graph| {
            let unit = CompileUnit {
                cc: self.cc,
                index: unit_graph.unit_index(),
            };
            let root_block = unit.bb(unit_graph.root());
            self.dfs_connect(&unit, &root_block, None);
        });
    }

    /// Recursively connect blocks in pre-order DFS traversal.
    fn dfs_connect(&self, unit: &CompileUnit<'tcx>, block: &BasicBlock<'tcx>, parent: Option<BlockId>) {
        let block_id = block.id();

        // 1. Link structural parent/child relationship
        if let Some(parent_id) = parent {
            self.add_relation(parent_id, BlockRelation::Contains, block_id);
            self.add_relation(block_id, BlockRelation::ContainedBy, parent_id);
        }

        // 2. Link kind-specific relationships
        match block {
            BasicBlock::Func(func) => self.link_func(unit, block_id, func),
            BasicBlock::Class(class) => self.link_class(block_id, class),
            BasicBlock::Impl(impl_block) => self.link_impl(unit, block_id, impl_block),
            BasicBlock::Trait(trait_block) => self.link_trait(block_id, trait_block),
            BasicBlock::Enum(enum_block) => self.link_enum(block_id, enum_block),
            BasicBlock::Call(call) => self.link_call(unit, block_id, call),
            BasicBlock::Field(field) => self.link_field(unit, block_id, field),
            // Root, Stmt, Const, Parameters, Return - no special linking needed
            _ => {}
        }

        // 3. Recurse into children (pre-order: process this node before children)
        for child_id in block.children() {
            let child = unit.bb(child_id);
            self.dfs_connect(unit, &child, Some(block_id));
        }
    }

    /// Add a relationship to the related_map.
    #[inline]
    fn add_relation(&self, from: BlockId, relation: BlockRelation, to: BlockId) {
        self.cc.related_map.add_relation_impl(from, relation, to);
    }

    /// Link function/method relationships.
    fn link_func(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        func: &crate::block::BlockFunc<'tcx>,
    ) {
        // Parameters - now individual BlockParameter blocks
        for param_id in func.get_parameters() {
            self.add_relation(block_id, BlockRelation::HasParameters, param_id);
        }

        // Return type
        if let Some(ret_id) = func.get_returns() {
            self.add_relation(block_id, BlockRelation::HasReturn, ret_id);
        }

        // Find calls within this function's children and link to callees
        // Also populate type_deps and func_deps
        for child_id in func.base.get_children() {
            self.find_calls_recursive(unit, block_id, func, child_id);
        }
    }

    /// Recursively find call blocks and link them to this function as caller.
    /// Also populates func_deps (free functions) and type_deps (static method receivers).
    fn find_calls_recursive(
        &self,
        unit: &CompileUnit<'tcx>,
        caller_func_id: BlockId,
        caller_func: &crate::block::BlockFunc<'tcx>,
        block_id: BlockId,
    ) {
        let block = unit.bb(block_id);

        if let BasicBlock::Call(call) = &block {
            // Get the callee symbol to check its kind
            if let Some(callee_sym) = call.base.node.ident_symbol(unit) {
                let callee_kind = callee_sym.kind();

                match callee_kind {
                    crate::symbol::SymKind::Function => {
                        // Free function call → add to func_deps
                        if let Some(callee_block_id) = callee_sym.block_id() {
                            caller_func.add_func_dep(callee_block_id);
                            // Also establish caller-callee relation
                            self.add_relation(caller_func_id, BlockRelation::Calls, callee_block_id);
                            self.add_relation(callee_block_id, BlockRelation::CalledBy, caller_func_id);
                        }
                    }
                    crate::symbol::SymKind::Method => {
                        // Method call → check if it has a type receiver (Foo::method)
                        // The type is tracked via type_of on the callee symbol
                        if let Some(type_sym_id) = callee_sym.type_of() {
                            if let Some(type_sym) = self.cc.opt_get_symbol(type_sym_id) {
                                if let Some(type_block_id) = type_sym.block_id() {
                                    caller_func.add_type_dep(type_block_id);
                                }
                            }
                        }
                        // Establish caller-callee relation for methods too
                        if let Some(callee_block_id) = callee_sym.block_id() {
                            self.add_relation(caller_func_id, BlockRelation::Calls, callee_block_id);
                            self.add_relation(callee_block_id, BlockRelation::CalledBy, caller_func_id);
                        }
                    }
                    _ => {
                        // Other kinds (e.g., Struct for associated functions like Foo::new)
                        // Add type to type_deps
                        if let Some(callee_block_id) = callee_sym.block_id() {
                            if callee_kind == crate::symbol::SymKind::Struct
                                || callee_kind == crate::symbol::SymKind::Enum
                            {
                                caller_func.add_type_dep(callee_block_id);
                            }
                        }
                    }
                }
            }
        }

        // Recurse into children
        for child_id in block.children() {
            self.find_calls_recursive(unit, caller_func_id, caller_func, child_id);
        }
    }

    /// Resolve a call expression to its target function's BlockId.
    fn resolve_callee(
        &self,
        unit: &CompileUnit<'tcx>,
        call: &crate::block::BlockCall<'tcx>,
    ) -> Option<BlockId> {
        // Symbol resolution was done by bind.rs - just follow the links
        call.base.node.ident_symbol(unit)?.block_id()
    }

    /// Link struct/class relationships.
    fn link_class(&self, block_id: BlockId, class: &crate::block::BlockClass<'tcx>) {
        // Fields
        for field_id in class.get_fields() {
            self.add_relation(block_id, BlockRelation::HasField, field_id);
            self.add_relation(field_id, BlockRelation::FieldOf, block_id);
        }

        // Methods
        for method_id in class.get_methods() {
            self.add_relation(block_id, BlockRelation::HasMethod, method_id);
            self.add_relation(method_id, BlockRelation::MethodOf, block_id);
        }
    }

    /// Link impl block relationships.
    fn link_impl(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        impl_block: &crate::block::BlockImpl<'tcx>,
    ) {
        // Methods
        for method_id in impl_block.get_methods() {
            self.add_relation(block_id, BlockRelation::HasMethod, method_id);
            self.add_relation(method_id, BlockRelation::MethodOf, block_id);
        }

        // Target type (impl SomeType { ... })
        let target_id = if let Some(target_id) = impl_block.get_target() {
            Some(target_id)
        } else {
            // Try to resolve from HirNode symbol
            if let Some(target_id) = self.resolve_impl_target(unit, impl_block) {
                // Store the resolved target in the impl block for future access
                impl_block.set_target(target_id);
                Some(target_id)
            } else {
                None
            }
        };

        if let Some(target_id) = target_id {
            self.add_relation(block_id, BlockRelation::ImplFor, target_id);
            self.add_relation(target_id, BlockRelation::HasImpl, block_id);

            // Move impl methods to the target class as children
            let target_block = unit.bb(target_id);
            if let Some(class) = target_block.as_class() {
                for method_id in impl_block.get_methods() {
                    class.add_method(method_id);
                    // Add method as child of the class
                    class.base.add_child(method_id);
                    // Update method's parent to point to class
                    let method_block = unit.bb(method_id);
                    if let Some(base) = method_block.base() {
                        base.set_parent(target_id);
                    }
                }
            }

            // Remove impl block from its parent's children (typically root)
            if let Some(parent_id) = impl_block.base.get_parent() {
                let parent_block = unit.bb(parent_id);
                if let Some(base) = parent_block.base() {
                    base.remove_child(block_id);
                }
            }
        }

        // Trait reference (impl SomeTrait for SomeType { ... })
        if let Some(trait_id) = impl_block.get_trait_ref() {
            self.add_relation(block_id, BlockRelation::Implements, trait_id);
            self.add_relation(trait_id, BlockRelation::ImplementedBy, block_id);
        } else {
            // Try to resolve from HirNode symbol
            if let Some(trait_id) = self.resolve_impl_trait(unit, impl_block) {
                self.add_relation(block_id, BlockRelation::Implements, trait_id);
                self.add_relation(trait_id, BlockRelation::ImplementedBy, block_id);
            }
        }
    }

    /// Resolve the target type of an impl block.
    fn resolve_impl_target(
        &self,
        unit: &CompileUnit<'tcx>,
        impl_block: &crate::block::BlockImpl<'tcx>,
    ) -> Option<BlockId> {
        // Look through children to find type identifier with a resolved symbol
        for child in impl_block.base.node.children(unit) {
            // Find identifier children (type identifiers)
            if let Some(ident) = child.find_ident(unit) {
                // Use the identifier name to look up the struct in block indexes
                let blocks = unit.cc.find_blocks_by_name(&ident.name);
                for (_, kind, block_id) in blocks {
                    // Find a struct/class with this name (not impl blocks)
                    if kind == crate::block::BlockKind::Class {
                        return Some(block_id);
                    }
                }
            }
        }
        None
    }

    /// Resolve the trait reference of an impl block.
    fn resolve_impl_trait(
        &self,
        _unit: &CompileUnit<'tcx>,
        _impl_block: &crate::block::BlockImpl<'tcx>,
    ) -> Option<BlockId> {
        // Trait resolution requires looking at the impl's trait bound
        // This would need language-specific field access (e.g., LangRust::field_trait)
        // For now, return None - trait_ref should be set during graph building
        None
    }

    /// Link trait relationships.
    fn link_trait(&self, block_id: BlockId, trait_block: &crate::block::BlockTrait<'tcx>) {
        // Methods
        for method_id in trait_block.get_methods() {
            self.add_relation(block_id, BlockRelation::HasMethod, method_id);
            self.add_relation(method_id, BlockRelation::MethodOf, block_id);
        }
    }

    /// Link enum relationships.
    fn link_enum(&self, block_id: BlockId, enum_block: &crate::block::BlockEnum<'tcx>) {
        // Variants are like fields
        for variant_id in enum_block.get_variants() {
            self.add_relation(block_id, BlockRelation::HasField, variant_id);
            self.add_relation(variant_id, BlockRelation::FieldOf, block_id);
        }
    }

    /// Link call site relationships.
    fn link_call(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        call: &crate::block::BlockCall<'tcx>,
    ) {
        // Link call site to callee
        if let Some(callee_id) = self.resolve_callee(unit, call) {
            self.add_relation(block_id, BlockRelation::Calls, callee_id);
            self.add_relation(callee_id, BlockRelation::CalledBy, block_id);
        }
    }

    /// Link field relationships.
    fn link_field(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        field: &crate::block::BlockField<'tcx>,
    ) {
        // Type reference (TypeOf relationship)
        if let Some(type_id) = field.base.get_type_ref() {
            self.add_relation(block_id, BlockRelation::TypeOf, type_id);
            self.add_relation(type_id, BlockRelation::TypeFor, block_id);
        } else {
            // Try to resolve from HirNode symbol
            if let Some(type_id) = self.resolve_field_type(unit, field) {
                self.add_relation(block_id, BlockRelation::TypeOf, type_id);
                self.add_relation(type_id, BlockRelation::TypeFor, block_id);
            }
        }
    }

    /// Resolve the type of a field.
    fn resolve_field_type(
        &self,
        unit: &CompileUnit<'tcx>,
        field: &crate::block::BlockField<'tcx>,
    ) -> Option<BlockId> {
        // The field's HirNode should have a type identifier we can resolve
        field.base.node.ident_symbol(unit)?.block_id()
    }

}
