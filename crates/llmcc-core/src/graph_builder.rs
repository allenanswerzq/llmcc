use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::collections::HashSet;
use std::marker::PhantomData;
use std::mem;

use crate::DynError;
use crate::block::Arena as BlockArena;
pub use crate::block::{BasicBlock, BlockId, BlockKind, BlockRelation};
use crate::block::{
    BlockCall, BlockClass, BlockConst, BlockEnum, BlockField, BlockFunc, BlockImpl, BlockMethod,
    BlockRoot, BlockStmt,
};
use crate::block_rel::BlockRelationMap;
use crate::context::{CompileCtxt, CompileUnit};
use crate::graph::UnitGraph;
use crate::ir::HirNode;
use crate::lang_def::LanguageTrait;
use crate::symbol::{SymId, Symbol};
use crate::visit::HirVisitor;

#[derive(Debug, Clone, Copy, Default)]
pub struct GraphBuildConfig;

#[derive(Default, Debug, Clone, Copy)]
pub struct GraphBuildOption {
    // Placeholder for future configuration options
}

impl GraphBuildOption {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug)]
struct GraphBuilder<'tcx, Language> {
    unit: CompileUnit<'tcx>,
    root: Option<BlockId>,
    children_stack: Vec<Vec<BlockId>>,
    _config: GraphBuildConfig,
    _marker: PhantomData<Language>,
}

impl<'tcx, Language: LanguageTrait> GraphBuilder<'tcx, Language> {
    fn new(unit: CompileUnit<'tcx>, _config: GraphBuildConfig) -> Self {
        Self {
            unit,
            root: None,
            children_stack: Vec::new(),
            _config,
            _marker: PhantomData,
        }
    }

    fn alloc_from_block_arena<T, F>(&self, alloc: F) -> &'tcx T
    where
        F: for<'a> FnOnce(&'a BlockArena<'tcx>) -> &'a mut T,
    {
        let arena = self.unit.cc.block_arena.lock();
        let ptr = alloc(&arena);
        let reference: &T = &*ptr;
        unsafe { mem::transmute::<&T, &'tcx T>(reference) }
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
                let block = BlockRoot::from_hir(id, node, parent, children, file_name);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_root.alloc(block));
                BasicBlock::Root(block_ref)
            }
            BlockKind::Func => {
                let block = BlockFunc::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_func.alloc(block));
                BasicBlock::Func(block_ref)
            }
            BlockKind::Method => {
                let block = BlockMethod::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_method.alloc(block));
                BasicBlock::Method(block_ref)
            }
            BlockKind::Class => {
                let block = BlockClass::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_class.alloc(block));
                BasicBlock::Class(block_ref)
            }
            BlockKind::Stmt => {
                let stmt = BlockStmt::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_stmt.alloc(stmt));
                BasicBlock::Stmt(block_ref)
            }
            BlockKind::Call => {
                let stmt = BlockCall::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_call.alloc(stmt));
                BasicBlock::Call(block_ref)
            }
            BlockKind::Enum => {
                let enum_ty = BlockEnum::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_enum.alloc(enum_ty));
                BasicBlock::Enum(block_ref)
            }
            BlockKind::Const => {
                let stmt = BlockConst::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_const.alloc(stmt));
                BasicBlock::Const(block_ref)
            }
            BlockKind::Impl => {
                let block = BlockImpl::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_impl.alloc(block));
                BasicBlock::Impl(block_ref)
            }
            BlockKind::Field => {
                let block = BlockField::from_hir(id, node, parent, children);
                let block_ref = self.alloc_from_block_arena(|arena| arena.blk_field.alloc(block));
                BasicBlock::Field(block_ref)
            }
            _ => {
                panic!("unknown block kind: {}", kind)
            }
        }
    }

    fn build_edges(&self, node: HirNode<'tcx>) -> BlockRelationMap {
        let edges = BlockRelationMap::default();
        let mut visited = HashSet::new();
        let mut unresolved = HashSet::new();
        self.collect_edges(node, &edges, &mut visited, &mut unresolved);
        edges
    }

    fn collect_edges(
        &self,
        node: HirNode<'tcx>,
        edges: &BlockRelationMap,
        visited: &mut HashSet<SymId>,
        unresolved: &mut HashSet<SymId>,
    ) {
        // Try to process symbol dependencies for this node
        // If this node is a Scope node, it has a direct reference to its Scope
        if let Some(scope_node) = node.as_scope()
            && let Some(scope) = *scope_node.scope.read()
            && let Some(symbol) = scope.symbol()
        {
            self.process_symbol(symbol, edges, visited, unresolved);
        }

        // Recurse into children
        for &child_id in node.children() {
            let child = self.unit.hir_node(child_id);
            self.collect_edges(child, edges, visited, unresolved);
        }
    }

    fn process_symbol(
        &self,
        symbol: &'tcx Symbol,
        edges: &BlockRelationMap,
        visited: &mut HashSet<SymId>,
        unresolved: &mut HashSet<SymId>,
    ) {
        let symbol_id = symbol.id;

        // Avoid processing the same symbol twice
        if !visited.insert(symbol_id) {
            return;
        }

        let Some(from_block) = symbol.block_id() else {
            return;
        };

        let dependencies = symbol.depends.read().clone();
        for dep_id in dependencies {
            self.link_dependency(dep_id, from_block, edges, unresolved);
        }
    }

    fn link_dependency(
        &self,
        dep_id: SymId,
        from_block: BlockId,
        edges: &BlockRelationMap,
        unresolved: &mut HashSet<SymId>,
    ) {
        // If target symbol exists and has a block, add the dependency edge
        if let Some(target_symbol) = self.unit.opt_get_symbol(dep_id) {
            if let Some(to_block) = target_symbol.block_id() {
                if !edges.has_relation(from_block, BlockRelation::DependsOn, to_block) {
                    edges.add_relation(from_block, to_block);
                }
                let target_unit = target_symbol.unit_index();
                if target_unit.is_some()
                    && target_unit != Some(self.unit.index)
                    && unresolved.insert(dep_id)
                {
                    self.unit.add_unresolved_symbol(target_symbol);
                }
                return;
            }

            // Target symbol exists but block not yet known
            if unresolved.insert(dep_id) {
                self.unit.add_unresolved_symbol(target_symbol);
            }
            return;
        }

        // Target symbol not found at all
        unresolved.insert(dep_id);
    }

    fn build_block(
        &mut self,
        unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        parent: BlockId,
        recursive: bool,
    ) {
        let id = self.next_id();
        let mut block_kind = Language::block_kind(node.kind_id());
        if block_kind == BlockKind::Func {
            let mut current_parent = node.parent();
            while let Some(parent_id) = current_parent {
                let parent_node = unit.hir_node(parent_id);
                let parent_kind = Language::block_kind(parent_node.kind_id());
                if matches!(parent_kind, BlockKind::Class | BlockKind::Impl) {
                    block_kind = BlockKind::Method;
                    break;
                }
                if parent_kind == BlockKind::Root {
                    break;
                }
                current_parent = parent_node.parent();
            }
        }

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
        if let Some(scope_node) = node.as_scope()
            && let Some(scope) = *scope_node.scope.read()
            && let Some(symbol) = scope.symbol()
        {
            // Only set the block ID if it hasn't been set before
            // This prevents impl blocks from overwriting struct block IDs
            if symbol.block_id().is_none() {
                symbol.set_block_id(id);
            }
        }
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
            | BlockKind::Enum
            | BlockKind::Const
            | BlockKind::Impl
            | BlockKind::Field => self.build_block(unit, node, parent, true),
            _ => self.visit_children(unit, node, parent),
        }
    }
}

pub fn build_unit_graph<L: LanguageTrait>(
    unit: CompileUnit<'_>,
    unit_index: usize,
    config: GraphBuildConfig,
) -> Result<UnitGraph, DynError> {
    let root_hir = unit
        .file_start_hir_id()
        .ok_or("missing file start HIR id")?;
    let mut builder = GraphBuilder::<L>::new(unit, config);
    let root_node = unit.hir_node(root_hir);
    builder.visit_node(unit, root_node, BlockId::ROOT_PARENT);

    let root_block = builder.root;
    let root_block = root_block.ok_or("graph builder produced no root")?;
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
/// * `_config` - Build configuration (for future extensibility)
///
/// # Returns
/// A vector of UnitGraph objects, one per compilation unit, indexed by unit index
pub fn build_llmcc_graph<'tcx, L: LanguageTrait>(
    cc: &'tcx CompileCtxt<'tcx>,
    _config: GraphBuildOption,
) -> Result<Vec<UnitGraph>, DynError> {
    let unit_graphs = (0..cc.get_files().len())
        .into_par_iter()
        .map(|index| {
            let unit = cc.compile_unit(index);
            build_unit_graph::<L>(unit, index, GraphBuildConfig)
        })
        .collect::<Result<Vec<UnitGraph>, DynError>>()?;

    Ok(unit_graphs)
}
