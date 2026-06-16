//! HIR graph builder from IR nodes.

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::marker::PhantomData;

use crate::Result;
use crate::block::{
    BasicBlock, BlockAlias, BlockCall, BlockClass, BlockConst, BlockEnum, BlockField, BlockFunc,
    BlockImpl, BlockInterface, BlockKind, BlockModule, BlockParameter, BlockReturn, BlockRoot,
    BlockTrait,
};
use crate::context::{CompileCtxt, CompileUnit};
use crate::graph::UnitGraph;
use crate::id::BlockId;
use crate::ir::HirNode;
use crate::lang_def::Language;
use crate::visit::HirVisitor;

/// Options for building block graphs from HIR.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GraphBuildOptions {
    sequential: bool,
}

impl GraphBuildOptions {
    /// Create options that build unit graphs in parallel.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create options that build unit graphs sequentially.
    pub fn sequential() -> Self {
        Self { sequential: true }
    }

    /// Choose whether unit graphs are built sequentially.
    pub fn with_sequential(mut self, sequential: bool) -> Self {
        self.sequential = sequential;
        self
    }

    /// Return true when unit graphs should be built one at a time.
    pub fn is_sequential(self) -> bool {
        self.sequential
    }
}

type ParentBlockKind = BlockKind;
type ChildBlock = (BlockId, BlockKind);
type ChildBlocks = Vec<ChildBlock>;
type BlockStackFrame = (Option<ParentBlockKind>, ChildBlocks);

#[derive(Debug)]
struct BlockStack {
    frames: Vec<BlockStackFrame>,
}

impl BlockStack {
    fn new() -> Self {
        Self {
            frames: vec![(None, Vec::new())],
        }
    }

    fn current_parent_kind(&self) -> Option<ParentBlockKind> {
        self.frames.last().and_then(|(parent_kind, _)| *parent_kind)
    }

    fn push_parent_frame(&mut self, kind: ParentBlockKind) {
        self.frames.push((Some(kind), Vec::new()));
    }

    fn pop_child_blocks(&mut self) -> ChildBlocks {
        if self.frames.len() <= 1 {
            return Vec::new();
        }

        self.frames
            .pop()
            .map(|(_, children)| children)
            .unwrap_or_default()
    }

    fn push_child_block(&mut self, id: BlockId, kind: BlockKind) {
        if self.frames.is_empty() {
            self.frames.push((None, Vec::new()));
        }

        if let Some((_, children)) = self.frames.last_mut() {
            children.push((id, kind));
        }
    }
}

#[derive(Debug)]
struct GraphBuilder<'tcx, L> {
    unit: CompileUnit<'tcx>,
    root: Option<BlockId>,
    stack: BlockStack,
    _marker: PhantomData<L>,
}

impl<'tcx, L: Language> GraphBuilder<'tcx, L> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            unit,
            root: None,
            stack: BlockStack::new(),
            _marker: PhantomData,
        }
    }

    fn create_block(
        &self,
        id: BlockId,
        node: HirNode<'tcx>,
        kind: BlockKind,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
    ) -> BasicBlock<'tcx> {
        let query = node.query(&self.unit);
        let symbol = query.try_block_symbol(kind, L::name_field());

        match kind {
            BlockKind::Root => {
                let block = BlockRoot::new_with(id, node, parent, children, symbol);
                block.set_meta(self.unit);
                BasicBlock::Root(self.unit.alloc_block(id, block))
            }
            BlockKind::Func | BlockKind::Method => {
                let block = BlockFunc::new_with(id, node, kind, parent, children, symbol);
                BasicBlock::Func(self.unit.alloc_block(id, block))
            }
            BlockKind::Class => {
                let block = BlockClass::new_with(id, node, parent, children, symbol);
                BasicBlock::Class(self.unit.alloc_block(id, block))
            }
            BlockKind::Trait => {
                let block = BlockTrait::new_with(id, node, parent, children, symbol);
                BasicBlock::Trait(self.unit.alloc_block(id, block))
            }
            BlockKind::Interface => {
                let block = BlockInterface::new_with(id, node, parent, children, symbol);
                BasicBlock::Interface(self.unit.alloc_block(id, block))
            }
            BlockKind::Call => {
                // For call blocks, symbol is the callee (if resolved)
                let stmt = BlockCall::new_with(id, node, parent, children, symbol);
                // Set callee from resolved symbol
                if let Some(callee_sym) = query.try_resolved()
                    && let Some(callee_block_id) = callee_sym.block_id()
                {
                    stmt.set_callee(callee_block_id);
                }
                BasicBlock::Call(self.unit.alloc_block(id, stmt))
            }
            BlockKind::Enum => {
                let enum_ty = BlockEnum::new_with(id, node, parent, children, symbol);
                BasicBlock::Enum(self.unit.alloc_block(id, enum_ty))
            }
            BlockKind::Const => {
                let mut stmt = BlockConst::new_with_name(
                    id,
                    node,
                    parent,
                    children,
                    query.try_first_ident_name(),
                    symbol,
                );
                if let Some(type_sym) = self.unit.try_actual_type(symbol) {
                    stmt.set_type(self.unit, type_sym);
                }
                BasicBlock::Const(self.unit.alloc_block(id, stmt))
            }
            BlockKind::Impl => {
                let mut block = BlockImpl::new(id, node, parent, children);

                if let Some(sym) = query.try_ident_symbol_with_field(L::type_field()) {
                    block.set_target(self.unit, sym);
                }

                if let Some(sym) = query.try_ident_symbol_with_field(L::trait_field()) {
                    block.set_trait(self.unit, sym);
                }

                BasicBlock::Impl(self.unit.alloc_block(id, block))
            }
            BlockKind::Field => {
                let mut block = BlockField::new_with_name(
                    id,
                    node,
                    parent,
                    children,
                    query.try_field_name(L::name_field(), L::type_field()),
                    symbol,
                );
                if let Some(type_sym) = self.unit.try_actual_type(symbol) {
                    block.set_type(self.unit, type_sym);
                }
                BasicBlock::Field(self.unit.alloc_block(id, block))
            }
            BlockKind::Parameter => {
                let mut block = BlockParameter::new_with_name(
                    id,
                    node,
                    parent,
                    children,
                    query.try_parameter_name(),
                    symbol,
                );
                if let Some(type_sym) = self.unit.try_actual_type(symbol) {
                    block.set_type(self.unit, type_sym);
                }
                BasicBlock::Parameter(self.unit.alloc_block(id, block))
            }
            BlockKind::Return => {
                // Return blocks: symbol should already have type_of set during binding
                let mut block = BlockReturn::new_with(id, node, parent, children, symbol);
                if let Some(type_sym) = self.unit.try_actual_type(symbol) {
                    block.set_type(self.unit, type_sym);
                }
                BasicBlock::Return(self.unit.alloc_block(id, block))
            }
            BlockKind::Alias => {
                let name = symbol
                    .map(|symbol| self.unit.resolve_name(symbol.name))
                    .or_else(|| query.try_first_ident_name());
                let block = BlockAlias::new_with_name(id, node, parent, children, name, symbol);
                BasicBlock::Alias(self.unit.alloc_block(id, block))
            }
            BlockKind::Module => {
                let name = query.try_first_ident_name().unwrap_or_default();
                // Inline modules have children (the module body), file modules don't
                let is_inline = !children.is_empty();
                let block =
                    BlockModule::new_with(id, node, parent, children, name, is_inline, symbol);
                BasicBlock::Module(self.unit.alloc_block(id, block))
            }
            _ => {
                unreachable!("non-materialized block kind reached create_block: {kind}")
            }
        }
    }

    fn build_block(
        &mut self,
        node: HirNode<'tcx>,
        parent: BlockId,
        block_kind: BlockKind,
        recursive: bool,
    ) {
        let id = self.unit.reserve_block_id();
        let query = node.query(&self.unit);

        let block_kind = query.resolve_block_kind(block_kind, self.stack.current_parent_kind());
        if block_kind == BlockKind::Root {
            self.root.get_or_insert(id);
        }

        // Attach before visiting children so nested symbols can resolve the parent block.
        if block_kind.owns_symbol_block_id() {
            query.attach_block_id(id, block_kind, L::type_field());
        }

        let children = if recursive {
            self.collect_child_blocks(node, id, block_kind)
        } else {
            Vec::new()
        };

        let child_ids: Vec<BlockId> = children.iter().map(|child| child.0).collect();
        let block = self.create_block(id, node, block_kind, Some(parent), child_ids);
        block.attach_child_blocks(&children);
        self.unit.insert_block(id, block);
        self.stack.push_child_block(id, block_kind);
    }

    fn collect_child_blocks(
        &mut self,
        node: HirNode<'tcx>,
        parent: BlockId,
        parent_kind: ParentBlockKind,
    ) -> ChildBlocks {
        self.stack.push_parent_frame(parent_kind);
        self.visit_children(self.unit, node, parent);
        self.stack.pop_child_blocks()
    }
}

impl<'tcx, L: Language> HirVisitor<'tcx> for GraphBuilder<'tcx, L> {
    fn visit_children(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        for child in node.children(&unit) {
            let default_kind = L::try_block_kind_for_node(child);
            let contextual_kind = L::try_block_kind_in_parent(child, node);

            if let Some(contextual_kind) =
                contextual_kind.filter(|kind| Some(*kind) != default_kind)
            {
                // Parent context reclassifies this child as a block.
                self.build_block(child, parent, contextual_kind, false);
            } else if contextual_kind.is_none() && default_kind.is_some() {
                // Parent context suppresses the child's default block, but descendants may
                // still contain materialized blocks that belong under the current parent.
                self.visit_children(unit, child, parent);
            } else {
                // No parent-context override; let the normal visitor dispatch handle the child.
                self.visit_node(unit, child, parent);
            }
        }
    }

    fn visit_file(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = L::try_block_kind_for_node(node) {
            self.build_block(node, parent, kind, true);
        } else {
            self.visit_children(unit, node, parent);
        }
    }

    fn visit_internal(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = L::try_block_kind_for_node(node).filter(|kind| *kind != BlockKind::Root)
        {
            self.build_block(node, parent, kind, false);
        } else {
            self.visit_children(unit, node, parent);
        }
    }

    fn visit_scope(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = L::try_block_kind_for_node(node) {
            if !node.query(&unit).can_materialize_scope(kind) {
                self.visit_children(unit, node, parent);
            } else {
                self.build_block(node, parent, kind, true);
            }
        } else {
            self.visit_children(unit, node, parent);
        }
    }

    fn visit_ident(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = L::try_block_kind_for_node(node) {
            self.build_block(node, parent, kind, false);
        } else {
            self.visit_children(unit, node, parent);
        }
    }
}

fn build_graph<'tcx, L: Language>(
    unit: CompileUnit<'tcx>,
    unit_index: usize,
) -> Result<Option<UnitGraph>> {
    let root_hir = unit.file_root_id()?;
    let mut builder = GraphBuilder::<L>::new(unit);
    let root_node = unit.hir_node(root_hir);
    builder.visit_node(unit, root_node, BlockId::ROOT_PARENT);

    // Empty files or files with no blocks produce no root - this is OK, just skip them
    match builder.root {
        Some(root_block) => Ok(Some(UnitGraph::new(unit_index, root_block))),
        None => Ok(None),
    }
}

/// Build unit block graphs for all compilation units.
pub fn build_graphs<'tcx, L: Language>(
    cc: &'tcx CompileCtxt<'tcx>,
    options: GraphBuildOptions,
) -> Result<Vec<UnitGraph>> {
    let mut unit_graphs: Vec<UnitGraph> = if options.is_sequential() {
        (0..cc.unit_count())
            .map(|index| {
                let unit = cc.compile_unit(index);
                build_graph::<L>(unit, index)
            })
            .filter_map(|r| r.transpose())
            .collect::<Result<Vec<_>>>()?
    } else {
        (0..cc.unit_count())
            .into_par_iter()
            .map(|index| {
                let unit = cc.compile_unit(index);
                build_graph::<L>(unit, index)
            })
            .filter_map(|r| r.transpose())
            .collect::<Result<Vec<_>>>()?
    };

    unit_graphs.sort_by_key(UnitGraph::unit_index);

    Ok(unit_graphs)
}
