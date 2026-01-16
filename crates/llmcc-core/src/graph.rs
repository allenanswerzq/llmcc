//! Project and unit graph structures.

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
    fn dfs_connect(
        &self,
        unit: &CompileUnit<'tcx>,
        block: &BasicBlock<'tcx>,
        parent: Option<BlockId>,
    ) {
        let block_id = block.id();

        // 1. Link structural parent/child relationship
        if let Some(parent_id) = parent {
            self.add_relation(parent_id, BlockRelation::Contains, block_id);
            self.add_relation(block_id, BlockRelation::ContainedBy, parent_id);
        }

        // 2. Link kind-specific relationships
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
            // Root - no special linking needed
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

        // Type parameter bounds: for `function<T extends HasLength>`, create edge HasLength -> func
        // HasLength (bound) is used by func (this function)
        if let Some(func_sym) = func.base.symbol
            && let Some(scope_id) = func_sym.opt_scope()
        {
            let scope = unit.get_scope(scope_id);
            // Look for type parameters in the function's scope
            scope.for_each_symbol(|sym| {
                if sym.kind() == crate::symbol::SymKind::TypeParameter {
                    // Get the bound type from type_of
                    if let Some(bound_id) = sym.type_of()
                        && let Some(bound_sym) = unit.opt_get_symbol(bound_id)
                        && let Some(bound_block_id) = bound_sym.block_id()
                    {
                        // Create edge: bound --UsedBy--> this_func
                        // This means: bound is used by this_func (as a type parameter constraint)
                        self.add_relation(bound_block_id, BlockRelation::UsedBy, block_id);
                        self.add_relation(block_id, BlockRelation::Uses, bound_block_id);
                    }
                }
            });
        }

        // Populate type_deps from function symbol's nested_types (generic return type args)
        // e.g., for `fn get_user() -> Result<User, Error>`, nested_types contains User and Error
        if let Some(func_sym) = func.base.symbol
            && let Some(nested_types) = func_sym.nested_types()
        {
            for type_id in nested_types {
                // Follow type_of chain to get actual type symbol
                let type_sym = unit.opt_get_symbol(type_id).and_then(|sym| {
                    sym.type_of()
                        .and_then(|id| unit.opt_get_symbol(id))
                        .or(Some(sym))
                });
                if let Some(type_sym) = type_sym
                    && let Some(type_block_id) = type_sym.block_id()
                {
                    func.add_type_dep(type_block_id);
                }
            }
        }

        // Decorators (TypeScript/JavaScript `@decorator` on functions/methods)
        if let Some(func_sym) = func.base.symbol
            && let Some(decorators) = func_sym.decorators()
        {
            for decorator_id in decorators {
                if let Some(decorator_sym) = unit.opt_get_symbol(decorator_id)
                    && let Some(decorator_block_id) = decorator_sym.block_id()
                {
                    func.add_type_dep(decorator_block_id);
                }
            }
        }

        // Find calls within this function's children and link to callees
        // Also populate type_deps and func_deps
        for child_id in func.base.get_children() {
            self.find_calls_recursive(unit, block_id, func, child_id);
        }

        // Add Uses/UsedBy edges for type dependencies
        for type_id in func.get_type_deps() {
            self.add_relation(block_id, BlockRelation::Uses, type_id);
            self.add_relation(type_id, BlockRelation::UsedBy, block_id);
        }

        // Add Calls/CalledBy edges for type dependencies
        for type_id in func.get_func_deps() {
            self.add_relation(block_id, BlockRelation::Calls, type_id);
            self.add_relation(type_id, BlockRelation::CalledBy, block_id);
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

        // Check for Call blocks (explicit call blocks in Rust)
        if let BasicBlock::Call(call) = &block {
            // Get the callee symbol to check its kind
            if let Some(callee_sym) = call.base.node.ident_symbol(unit) {
                self.process_callee_symbol(unit, caller_func_id, caller_func, callee_sym);
            }
        }

        // For nodes that are call/new expressions but not Call blocks (TypeScript),
        // check if the node's symbol is a callable or constructable type.
        // This handles cases where we visit call_expression/new_expression nodes
        // that weren't converted to Call blocks.
        if let Some(base) = block.base() {
            let node = &base.node;
            // Check if this node has a resolved symbol that's a function or struct
            if let Some(callee_sym) = node.ident_symbol(unit) {
                let kind = callee_sym.kind();
                // Only process if it's not already a Call block and is callable
                if !matches!(&block, BasicBlock::Call(_))
                    && matches!(
                        kind,
                        crate::symbol::SymKind::Function
                            | crate::symbol::SymKind::Struct
                            | crate::symbol::SymKind::Enum
                    )
                {
                    self.process_callee_symbol(unit, caller_func_id, caller_func, callee_sym);
                }
            }
        }

        // Recurse into children
        for child_id in block.children() {
            self.find_calls_recursive(unit, caller_func_id, caller_func, child_id);
        }
    }

    /// Process a callee symbol to add func_deps/type_deps and establish relations
    fn process_callee_symbol(
        &self,
        _unit: &CompileUnit<'tcx>,
        caller_func_id: BlockId,
        caller_func: &crate::block::BlockFunc<'tcx>,
        callee_sym: &crate::symbol::Symbol,
    ) {
        let callee_kind = callee_sym.kind();
        let callee_block_id_opt = callee_sym.block_id();

        match callee_kind {
            crate::symbol::SymKind::Function => {
                // Free function call → add to func_deps
                if let Some(callee_block_id) = callee_block_id_opt {
                    caller_func.add_func_dep(callee_block_id);
                    // Also establish caller-callee relation
                    self.add_relation(caller_func_id, BlockRelation::Calls, callee_block_id);
                    self.add_relation(callee_block_id, BlockRelation::CalledBy, caller_func_id);
                }
            }
            crate::symbol::SymKind::Method => {
                // Method call → check if it has a type receiver (Foo::method)
                // The type is tracked via type_of on the callee symbol
                if let Some(type_sym_id) = callee_sym.type_of()
                    && let Some(type_sym) = self.cc.opt_get_symbol(type_sym_id)
                    && let Some(type_block_id) = type_sym.block_id()
                {
                    caller_func.add_type_dep(type_block_id);
                }
                // Establish caller-callee relation for methods too
                if let Some(callee_block_id) = callee_sym.block_id() {
                    self.add_relation(caller_func_id, BlockRelation::Calls, callee_block_id);
                    self.add_relation(callee_block_id, BlockRelation::CalledBy, caller_func_id);
                }
            }
            _ => {
                // Other kinds (e.g., Struct for associated functions like Foo::new or new Class())
                // Add type to type_deps and create Uses/UsedBy relations
                if let Some(callee_block_id) = callee_sym.block_id()
                    && (callee_kind == crate::symbol::SymKind::Struct
                        || callee_kind == crate::symbol::SymKind::Enum)
                {
                    caller_func.add_type_dep(callee_block_id);
                    // Add Uses relation for type_dep edges in arch-graph
                    self.add_relation(caller_func_id, BlockRelation::Uses, callee_block_id);
                    self.add_relation(callee_block_id, BlockRelation::UsedBy, caller_func_id);
                }
            }
        }
    }

    /// Link struct/class relationships.
    fn link_class(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        class: &crate::block::BlockClass<'tcx>,
    ) {
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

        if let Some(class_sym) = class.base.symbol {
            // Extended class (from extends_clause) - stored in type_of
            if let Some(extends_id) = class_sym.type_of()
                && let Some(extends_sym) = unit.opt_get_symbol(extends_id)
                && let Some(extends_block_id) = extends_sym.block_id()
            {
                // Set extends relation (don't add to type_dep since @extends already shows the edge)
                self.add_relation(block_id, BlockRelation::Extends, extends_block_id);
                self.add_relation(extends_block_id, BlockRelation::ExtendedBy, block_id);

                // Populate the extends field on the BlockClass for display
                let extends_name = unit.resolve_name(extends_sym.name);
                class.set_extends(extends_name, Some(extends_block_id));
            }

            // Implemented interfaces (from implements_clause) - stored in nested_types
            // For TypeScript: nested_types = implemented interfaces
            // For Rust: nested_types = field types (traits from dyn Trait)
            if let Some(nested) = class_sym.nested_types() {
                for type_id in nested {
                    if let Some(type_sym) = unit.opt_get_symbol(type_id)
                        && let Some(type_block_id) = type_sym.block_id()
                    {
                        // Add as type_dep
                        class.base.add_type_dep(type_block_id);

                        // Only create Implements relation for Trait/Interface, not regular types
                        // In Rust, nested_types may include field types which are not implementations
                        let is_trait = type_sym.kind() == crate::symbol::SymKind::Trait;
                        let is_interface = type_sym.kind() == crate::symbol::SymKind::Interface;

                        if is_trait {
                            // For Rust Traits (from dyn Trait): create Uses/UsedBy for bound edges
                            // Don't create Implements relation here - Rust impl blocks handle that
                            self.add_relation(block_id, BlockRelation::Uses, type_block_id);
                            self.add_relation(type_block_id, BlockRelation::UsedBy, block_id);
                        } else if is_interface {
                            // TypeScript Interfaces: create Implements relation for interface -> implements edges
                            self.add_relation(block_id, BlockRelation::Implements, type_block_id);
                            self.add_relation(
                                type_block_id,
                                BlockRelation::ImplementedBy,
                                block_id,
                            );
                        }
                    }
                }
            }

            // Decorators (from @decorator syntax in TypeScript/JavaScript)
            if let Some(decorators) = class_sym.decorators() {
                for decorator_id in decorators {
                    if let Some(decorator_sym) = unit.opt_get_symbol(decorator_id)
                        && let Some(decorator_block_id) = decorator_sym.block_id()
                    {
                        // Add as type_dep for decorators
                        class.base.add_type_dep(decorator_block_id);
                        self.add_relation(block_id, BlockRelation::Uses, decorator_block_id);
                        self.add_relation(decorator_block_id, BlockRelation::UsedBy, block_id);
                    }
                }
            }
        }
        // Note: Field type argument edges (e.g., User -> Triple for `data: Triple<User>`)
        // are created during graph_render edge collection from field.symbol.nested_types
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

        // Target type - resolve from symbol if block_id wasn't available during building
        let target_id = impl_block
            .get_target()
            .or_else(|| impl_block.target_sym.and_then(|sym| sym.block_id()));
        if let Some(target_id) = target_id {
            impl_block.set_target(target_id);
            self.add_relation(block_id, BlockRelation::ImplFor, target_id);
            self.add_relation(target_id, BlockRelation::HasImpl, block_id);

            // Populate type_deps from the target symbol's nested_types
            // These were set during binding from impl's trait type arguments (e.g., User from `impl Repository<User>`)
            // Note: target_sym may be a local symbol with type_of pointing to actual struct, so we get
            // nested_types from target_sym but add type_deps to the actual target block
            if let Some(target_sym) = impl_block.target_sym
                && let Some(nested_types) = target_sym.nested_types()
            {
                let target_block = unit.bb(target_id);
                if let Some(base) = target_block.base() {
                    for type_id in nested_types {
                        // Follow type_of chain to get actual type symbol
                        let type_sym = unit.opt_get_symbol(type_id).and_then(|sym| {
                            sym.type_of()
                                .and_then(|id| unit.opt_get_symbol(id))
                                .or(Some(sym))
                        });
                        if let Some(type_sym) = type_sym
                            && let Some(type_block_id) = type_sym.block_id()
                        {
                            base.type_deps.write().insert(type_block_id);
                        }
                    }
                }
            }
        }

        // Trait reference - resolve from symbol if block_id wasn't available during building
        let trait_id = impl_block
            .get_trait_ref()
            .or_else(|| impl_block.trait_sym.and_then(|sym| sym.block_id()));
        if let Some(trait_id) = trait_id {
            impl_block.set_trait_ref(trait_id);
            self.add_relation(block_id, BlockRelation::Implements, trait_id);
            self.add_relation(trait_id, BlockRelation::ImplementedBy, block_id);
        }
    }

    /// Link trait relationships (Rust traits).
    fn link_trait(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        trait_block: &crate::block::BlockTrait<'tcx>,
    ) {
        // Methods
        for method_id in trait_block.get_methods() {
            self.add_relation(block_id, BlockRelation::HasMethod, method_id);
            self.add_relation(method_id, BlockRelation::MethodOf, block_id);
        }

        // Type parameter bounds: for `trait Foo<T: Bar>`, create edge Bar -> Foo
        // Bar (bound) is used by Foo (this trait)
        if let Some(trait_sym) = trait_block.base.symbol
            && let Some(scope_id) = trait_sym.opt_scope()
        {
            let scope = unit.get_scope(scope_id);
            // Look for type parameters in the trait's scope
            scope.for_each_symbol(|sym| {
                if sym.kind() == crate::symbol::SymKind::TypeParameter {
                    // Get the bound trait from type_of
                    if let Some(bound_id) = sym.type_of()
                        && let Some(bound_sym) = unit.opt_get_symbol(bound_id)
                        && let Some(bound_block_id) = bound_sym.block_id()
                    {
                        // Create edge: bound --UsedBy--> this_trait
                        // This means: bound is used by this_trait (as a type parameter constraint)
                        self.add_relation(bound_block_id, BlockRelation::UsedBy, block_id);
                        self.add_relation(block_id, BlockRelation::Uses, bound_block_id);
                    }
                }
            });
        }
    }

    /// Link interface relationships (TypeScript interfaces).
    fn link_interface(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        iface_block: &crate::block::BlockInterface<'tcx>,
    ) {
        // Methods
        for method_id in iface_block.get_methods() {
            self.add_relation(block_id, BlockRelation::HasMethod, method_id);
            self.add_relation(method_id, BlockRelation::MethodOf, block_id);
        }

        // Fields
        for field_id in iface_block.get_fields() {
            self.add_relation(block_id, BlockRelation::HasField, field_id);
            self.add_relation(field_id, BlockRelation::FieldOf, block_id);
        }

        // Extended/inherited types: for `interface Foo extends Bar`, create edge Foo -> Bar
        if let Some(iface_sym) = iface_block.base.symbol
            && let Some(nested) = iface_sym.nested_types()
        {
            for base_type_id in nested {
                if let Some(base_sym) = unit.opt_get_symbol(base_type_id)
                    && let Some(base_block_id) = base_sym.block_id()
                {
                    // Create edge: this_interface --Extends--> base_interface
                    self.add_relation(block_id, BlockRelation::Extends, base_block_id);
                    self.add_relation(base_block_id, BlockRelation::ExtendedBy, block_id);

                    // Also populate the extends field on the BlockInterface for display
                    let base_name = unit.resolve_name(base_sym.name);
                    iface_block.add_extends(base_name, Some(base_block_id));
                }
            }
        }

        // Type parameter bounds: for `interface EventHandler<T extends Event>`, create edge Event -> EventHandler
        if let Some(iface_sym) = iface_block.base.symbol
            && let Some(scope_id) = iface_sym.opt_scope()
        {
            let scope = unit.get_scope(scope_id);
            // Look for type parameters in the interface's scope
            scope.for_each_symbol(|sym| {
                if sym.kind() == crate::symbol::SymKind::TypeParameter {
                    // Get the bound type from type_of
                    if let Some(bound_id) = sym.type_of()
                        && let Some(bound_sym) = unit.opt_get_symbol(bound_id)
                        && let Some(bound_block_id) = bound_sym.block_id()
                    {
                        // Create edge: bound --UsedBy--> this_interface
                        // This means: bound is used by this_interface (as a type parameter constraint)
                        self.add_relation(bound_block_id, BlockRelation::UsedBy, block_id);
                        self.add_relation(block_id, BlockRelation::Uses, bound_block_id);
                    }
                }
            });
        }
    }

    /// Link enum relationships.
    fn link_enum(
        &self,
        _unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        enum_block: &crate::block::BlockEnum<'tcx>,
    ) {
        // Variants are like fields
        for variant_id in enum_block.get_variants() {
            self.add_relation(block_id, BlockRelation::HasField, variant_id);
            self.add_relation(variant_id, BlockRelation::FieldOf, block_id);
        }
        // Note: Variant type argument edges are created during graph_render edge collection
    }

    /// Link call site relationships.
    fn link_call(
        &self,
        _unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        call: &crate::block::BlockCall<'tcx>,
    ) {
        // Link call site to callee
        // Already set by graph_builder when creating BlockCall
        if let Some(callee_id) = call.get_callee() {
            self.add_relation(block_id, BlockRelation::Calls, callee_id);
            self.add_relation(callee_id, BlockRelation::CalledBy, block_id);
        }
    }

    /// Link return type relationships.
    /// Uses symbol.type_of() chain for cross-file safe lookup.
    fn link_return(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        ret: &crate::block::BlockReturn<'tcx>,
    ) {
        // First try the block's type_ref directly (set during block building)
        if let Some(type_id) = ret.get_type_ref() {
            self.add_relation(block_id, BlockRelation::TypeOf, type_id);
            self.add_relation(type_id, BlockRelation::TypeFor, block_id);
            return;
        }

        // Fallback: resolve via symbol (handles cross-file references)
        let type_id = self.resolve_type_ref(unit, &ret.base);

        if let Some(type_id) = type_id {
            // Update the block's type_ref so rendering shows the correct reference
            ret.set_type_ref(type_id);
            self.add_relation(block_id, BlockRelation::TypeOf, type_id);
            self.add_relation(type_id, BlockRelation::TypeFor, block_id);
        }
    }

    /// Link parameter type relationships.
    /// Uses symbol.type_of() chain for cross-file safe lookup.
    fn link_parameter(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        param: &crate::block::BlockParameter<'tcx>,
    ) {
        // First try the block's type_ref directly (set during block building)
        if let Some(type_id) = param.get_type_ref() {
            self.add_relation(block_id, BlockRelation::TypeOf, type_id);
            self.add_relation(type_id, BlockRelation::TypeFor, block_id);
            return;
        }

        // Fallback: resolve via symbol (handles cross-file references)
        let type_id = self.resolve_type_ref(unit, &param.base);

        if let Some(type_id) = type_id {
            // Update the block's type_ref so rendering shows the correct reference
            param.set_type_ref(type_id);
            self.add_relation(block_id, BlockRelation::TypeOf, type_id);
            self.add_relation(type_id, BlockRelation::TypeFor, block_id);
        }
    }

    /// Link field relationships.
    /// Uses symbol.type_of() chain for cross-file safe lookup.
    fn link_field(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        field: &crate::block::BlockField<'tcx>,
    ) {
        // Link type reference
        // First try the block's type_ref directly (set during block building)
        if let Some(type_id) = field.get_type_ref() {
            self.add_relation(block_id, BlockRelation::TypeOf, type_id);
            self.add_relation(type_id, BlockRelation::TypeFor, block_id);
        } else {
            // Fallback: resolve via symbol (handles cross-file references)
            let type_id = self.resolve_type_ref(unit, &field.base);

            if let Some(type_id) = type_id {
                // Update the block's type_ref so rendering shows the correct reference
                field.set_type_ref(type_id);
                self.add_relation(block_id, BlockRelation::TypeOf, type_id);
                self.add_relation(type_id, BlockRelation::TypeFor, block_id);
            }
        }

        // Link nested fields (for enum variants with struct-like fields)
        for child_id in field.base.get_children() {
            self.add_relation(block_id, BlockRelation::HasField, child_id);
            self.add_relation(child_id, BlockRelation::FieldOf, block_id);
        }
    }

    /// Link const relationships (type annotation).
    /// Uses symbol.type_of() chain for cross-file safe lookup.
    fn link_const(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        const_block: &crate::block::BlockConst<'tcx>,
    ) {
        // First try the block's type_ref directly (set during block building)
        if let Some(type_id) = const_block.get_type_ref() {
            self.add_relation(block_id, BlockRelation::TypeOf, type_id);
            self.add_relation(type_id, BlockRelation::TypeFor, block_id);
            return;
        }

        // Fallback: resolve via symbol (handles cross-file references)
        let type_id = self.resolve_type_ref(unit, &const_block.base);

        if let Some(type_id) = type_id {
            // Update the block's type_ref so rendering shows the correct reference
            const_block.set_type_ref(type_id);
            self.add_relation(block_id, BlockRelation::TypeOf, type_id);
            self.add_relation(type_id, BlockRelation::TypeFor, block_id);
        }
    }

    /// Link type alias relationships.
    /// Uses symbol.type_of() chain for cross-file safe lookup.
    fn link_alias(
        &self,
        unit: &CompileUnit<'tcx>,
        block_id: BlockId,
        alias: &crate::block::BlockAlias<'tcx>,
    ) {
        // Try symbol.type_of() chain first (cross-file safe)
        let type_id = self.resolve_type_ref(unit, &alias.base);

        if let Some(type_id) = type_id {
            self.add_relation(block_id, BlockRelation::TypeOf, type_id);
            self.add_relation(type_id, BlockRelation::TypeFor, block_id);
        }
    }

    /// Resolve type reference from a block's symbol.type_of() chain.
    /// This is cross-file safe since binding already resolved the types.
    fn resolve_type_ref(
        &self,
        _unit: &CompileUnit<'tcx>,
        base: &crate::block::BlockBase<'tcx>,
    ) -> Option<BlockId> {
        // Use symbol.type_of() chain (cross-file safe)
        if let Some(sym) = base.symbol() {
            if let Some(type_of_id) = sym.type_of()
                && let Some(type_sym) = self.cc.opt_get_symbol(type_of_id)
            {
                return type_sym.block_id();
            }
            // If symbol IS the type (no type_of), use its own block_id
            // Type kinds include: Struct, Enum, Trait, TypeAlias, Primitive, etc.
            if crate::symbol::SYM_KIND_TYPES.contains(sym.kind()) {
                return sym.block_id();
            }
        }
        None
    }
}
