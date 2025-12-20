use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::collections::HashSet;
use std::marker::PhantomData;

use crate::DynError;
pub use crate::block::{BasicBlock, BlockId, BlockKind, BlockRelation};
use crate::block::{
    BlockCall, BlockClass, BlockConst, BlockEnum, BlockField, BlockFunc, BlockImpl, BlockMethod,
    BlockParameters, BlockReturn, BlockRoot, BlockStmt, BlockTrait,
};
use crate::block_rel::BlockRelationMap;
use crate::context::{CompileCtxt, CompileUnit};
use crate::graph::UnitGraph;
use crate::ir::HirNode;
use crate::lang_def::LanguageTrait;
use crate::symbol::{SymId, SymKind, Symbol};
use crate::visit::HirVisitor;

#[derive(Debug, Clone, Copy, Default)]
pub struct GraphBuildConfig;

#[derive(Debug, Clone, Copy)]
pub struct GraphBuildOption {
    pub sequential: bool,
    /// Component grouping depth from name for graph visualization:
    /// - 0: No grouping (flat graph, no clusters)
    /// - 1: Crate level only
    /// - 2: Top-level modules (data, service, api)
    /// - 3+: Deeper sub-modules
    /// - usize::MAX: Full depth (each file/module gets own cluster)
    pub component_depth: usize,
}

impl Default for GraphBuildOption {
    fn default() -> Self {
        Self {
            sequential: false,
            component_depth: 2, // Default to top-level modules
        }
    }
}

impl GraphBuildOption {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sequential(mut self, sequential: bool) -> Self {
        self.sequential = sequential;
        self
    }

    pub fn with_component_depth(mut self, depth: usize) -> Self {
        self.component_depth = depth;
        self
    }
}

#[derive(Debug)]
struct GraphBuilder<'tcx, Language> {
    unit: CompileUnit<'tcx>,
    root: Option<BlockId>,
    children_stack: Vec<Vec<BlockId>>,
    config: GraphBuildConfig,
    _marker: PhantomData<Language>,
}

impl<'tcx, Language: LanguageTrait> GraphBuilder<'tcx, Language> {
    fn new(unit: CompileUnit<'tcx>, config: GraphBuildConfig) -> Self {
        Self {
            unit,
            root: None,
            children_stack: Vec::new(),
            config,
            _marker: PhantomData,
        }
    }

    fn next_id(&self) -> BlockId {
        self.unit.reserve_block_id()
    }

    fn create_block(
        &self,
        id: BlockId,
        node: HirNode<'tcx>,
        kind: BlockKind,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> BasicBlock<'tcx> {
        match kind {
            BlockKind::Root => {
                let file_name = node.as_file().map(|file| file.file_path.clone());
                let block = BlockRoot::new(id, node, parent, children, file_name);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Root(block_ref)
            }
            BlockKind::Func => {
                let block = BlockFunc::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Func(block_ref)
            }
            BlockKind::Method => {
                let block = BlockMethod::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Method(block_ref)
            }
            BlockKind::Class => {
                let block = BlockClass::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Class(block_ref)
            }
            BlockKind::Trait => {
                let block = BlockTrait::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Trait(block_ref)
            }
            BlockKind::Stmt => {
                let stmt = BlockStmt::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(stmt);
                BasicBlock::Stmt(block_ref)
            }
            BlockKind::Call => {
                let stmt = BlockCall::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(stmt);
                BasicBlock::Call(block_ref)
            }
            BlockKind::Enum => {
                let enum_ty = BlockEnum::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(enum_ty);
                BasicBlock::Enum(block_ref)
            }
            BlockKind::Const => {
                let stmt = BlockConst::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(stmt);
                BasicBlock::Const(block_ref)
            }
            BlockKind::Impl => {
                let block = BlockImpl::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Impl(block_ref)
            }
            BlockKind::Field => {
                let block = BlockField::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Field(block_ref)
            }
            BlockKind::Parameters => {
                let block = BlockParameters::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Parameters(block_ref)
            }
            BlockKind::Return => {
                let block = BlockReturn::new(id, node, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Return(block_ref)
            }
            _ => {
                panic!("unknown block kind: {}", kind)
            }
        }
    }

    fn build_edges(&self, node: HirNode<'tcx>) -> BlockRelationMap {
        let edges = BlockRelationMap::default();
        let mut visited = HashSet::new();
        self.collect_edges(node, &edges, &mut visited);
        edges
    }

    fn collect_edges(
        &self,
        node: HirNode<'tcx>,
        edges: &BlockRelationMap,
        visited: &mut HashSet<SymId>,
    ) {
        // Process symbol and build edges based on symbol relationships
        if let Some(scope_node) = node.as_scope()
            && let Some(symbol) = scope_node.opt_symbol()
        {
            self.process_symbol(symbol, edges, visited);
        }

        // Recurse into children
        for &child_id in node.child_ids() {
            let child = self.unit.hir_node(child_id);
            self.collect_edges(child, edges, visited);
        }
    }

    fn process_symbol(
        &self,
        symbol: &'tcx Symbol,
        edges: &BlockRelationMap,
        visited: &mut HashSet<SymId>,
    ) {
        let symbol_id = symbol.id;

        // Avoid processing the same symbol twice
        if !visited.insert(symbol_id) {
            return;
        }

        // Get the block ID for this symbol
        let Some(block_id) = symbol.block_id() else {
            return;
        };

        let sym_kind = symbol.kind();

        // Build edges based on symbol kind and relationships
        match sym_kind {
            // Function/Method: add HasParameters, HasReturn edges
            SymKind::Function | SymKind::Method | SymKind::Closure => {
                // If this function has a type_of (return type), add HasReturn edge
                if let Some(return_type_sym_id) = symbol.type_of() {
                    if let Some(return_sym) = self.unit.cc.opt_get_symbol(return_type_sym_id) {
                        if let Some(return_block_id) = return_sym.block_id() {
                            edges.add_relation_impl(block_id, BlockRelation::HasReturn, return_block_id);
                        }
                    }
                }
            }

            // Struct: add HasField edges
            SymKind::Struct => {
                // nested_types contains field type symbols
                if let Some(nested) = symbol.nested_types() {
                    for field_sym_id in nested {
                        if let Some(field_sym) = self.unit.cc.opt_get_symbol(field_sym_id) {
                            if let Some(field_block_id) = field_sym.block_id() {
                                edges.add_relation_impl(block_id, BlockRelation::HasField, field_block_id);
                                edges.add_relation_impl(field_block_id, BlockRelation::FieldOf, block_id);
                            }
                        }
                    }
                }
            }

            // Enum: add HasField edges for variants
            SymKind::Enum => {
                if let Some(nested) = symbol.nested_types() {
                    for variant_sym_id in nested {
                        if let Some(variant_sym) = self.unit.cc.opt_get_symbol(variant_sym_id) {
                            if let Some(variant_block_id) = variant_sym.block_id() {
                                edges.add_relation_impl(block_id, BlockRelation::HasField, variant_block_id);
                                edges.add_relation_impl(variant_block_id, BlockRelation::FieldOf, block_id);
                            }
                        }
                    }
                }
            }

            // Trait: add HasMethod edges
            SymKind::Trait => {
                if let Some(nested) = symbol.nested_types() {
                    for method_sym_id in nested {
                        if let Some(method_sym) = self.unit.cc.opt_get_symbol(method_sym_id) {
                            if let Some(method_block_id) = method_sym.block_id() {
                                edges.add_relation_impl(block_id, BlockRelation::HasMethod, method_block_id);
                                edges.add_relation_impl(method_block_id, BlockRelation::MethodOf, block_id);
                            }
                        }
                    }
                }
            }

            // Field: add FieldOf edge to parent
            SymKind::Field | SymKind::EnumVariant => {
                if let Some(parent_sym_id) = symbol.field_of() {
                    if let Some(parent_sym) = self.unit.cc.opt_get_symbol(parent_sym_id) {
                        if let Some(parent_block_id) = parent_sym.block_id() {
                            edges.add_relation_impl(block_id, BlockRelation::FieldOf, parent_block_id);
                            edges.add_relation_impl(parent_block_id, BlockRelation::HasField, block_id);
                        }
                    }
                }
            }

            // For other kinds, we might add Uses/UsedBy based on type_of
            _ => {
                if let Some(type_sym_id) = symbol.type_of() {
                    if let Some(type_sym) = self.unit.cc.opt_get_symbol(type_sym_id) {
                        if let Some(type_block_id) = type_sym.block_id() {
                            edges.add_relation_impl(block_id, BlockRelation::Uses, type_block_id);
                            edges.add_relation_impl(type_block_id, BlockRelation::UsedBy, block_id);
                        }
                    }
                }
            }
        }
    }

    fn build_block(
        &mut self,
        unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        parent: BlockId,
        recursive: bool,
    ) {
        let id = self.next_id();
        let block_kind = Language::block_kind(node.kind_id());
        assert_ne!(block_kind, BlockKind::Undefined);

        if self.root.is_none() {
            self.root = Some(id);
        }

        let children = if recursive {
            self.children_stack.push(Vec::new());
            self.visit_children(self.unit, node, id);
            self.children_stack.pop().unwrap()
        } else {
            Vec::new()
        };

        let block = self.create_block(id, node, block_kind, Some(parent), children);
        self.unit.insert_block(id, block, parent);

        if let Some(children) = self.children_stack.last_mut() {
            children.push(id);
        }
    }
}

impl<'tcx, Language: LanguageTrait> HirVisitor<'tcx> for GraphBuilder<'tcx, Language> {
    fn visit_file(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        self.children_stack.push(Vec::new());
        self.build_block(unit, node, parent, true);
    }

    fn visit_internal(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        let kind = Language::block_kind(node.kind_id());
        match kind {
            BlockKind::Func
            | BlockKind::Method
            | BlockKind::Class
            | BlockKind::Trait
            | BlockKind::Enum
            | BlockKind::Const
            | BlockKind::Impl
            | BlockKind::Field
            | BlockKind::Call => self.build_block(unit, node, parent, false),
            _ => self.visit_children(unit, node, parent),
        }
    }

    fn visit_scope(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        let kind = Language::block_kind(node.kind_id());
        match kind {
            BlockKind::Func
            | BlockKind::Method
            | BlockKind::Class
            | BlockKind::Trait
            | BlockKind::Enum
            | BlockKind::Const
            | BlockKind::Impl
            | BlockKind::Field
            | BlockKind::Root => self.build_block(unit, node, parent, true),
            _ => self.visit_children(unit, node, parent),
        }
    }
}

pub fn build_unit_graph<L: LanguageTrait>(
    unit: CompileUnit<'_>,
    unit_index: usize,
    config: GraphBuildConfig,
) -> Result<UnitGraph, DynError> {
    let root_hir = unit.file_root_id().ok_or("missing file start HIR id")?;
    let mut builder = GraphBuilder::<L>::new(unit, config);
    let root_node = unit.hir_node(root_hir);
    builder.visit_node(unit, root_node, BlockId::ROOT_PARENT);

    let root_block = builder.root.ok_or("graph builder produced no root block")?;
    let edges = builder.build_edges(root_node);
    Ok(UnitGraph::new(unit_index, root_block, edges))
}

/// Build unit graphs for all compilation units in parallel.
///
/// This function processes all compilation units in the context in parallel,
/// building a UnitGraph for each one. It follows the same pattern as
/// collect_symbols_with for unified API design.
///
/// # Arguments
/// * `cc` - The compilation context containing all compilation units
/// * `config` - Build configuration (for future extensibility)
///
/// # Returns
/// A vector of UnitGraph objects, one per compilation unit, indexed by unit index
pub fn build_llmcc_graph<'tcx, L: LanguageTrait>(
    cc: &'tcx CompileCtxt<'tcx>,
    config: GraphBuildOption,
) -> Result<Vec<UnitGraph>, DynError> {
    let unit_graphs = if config.sequential {
        (0..cc.get_files().len())
            .map(|index| {
                let unit = cc.compile_unit(index);
                build_unit_graph::<L>(unit, index, GraphBuildConfig)
            })
            .collect::<Result<Vec<UnitGraph>, DynError>>()?
    } else {
        (0..cc.get_files().len())
            .into_par_iter()
            .map(|index| {
                let unit = cc.compile_unit(index);
                build_unit_graph::<L>(unit, index, GraphBuildConfig)
            })
            .collect::<Result<Vec<UnitGraph>, DynError>>()?
    };

    cc.block_arena.bb_sort_by(|block| block.id());

    Ok(unit_graphs)
}
