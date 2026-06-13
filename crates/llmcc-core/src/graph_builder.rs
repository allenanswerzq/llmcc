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
use crate::symbol::SymKind;
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

#[derive(Debug)]
struct BlockStack {
    frames: Vec<(ParentBlockKind, Vec<(BlockId, BlockKind)>)>,
}

impl BlockStack {
    fn new() -> Self {
        Self {
            frames: vec![(BlockKind::Undefined, Vec::new())],
        }
    }

    fn current_parent_kind(&self) -> ParentBlockKind {
        self.frames
            .last()
            .map(|(kind, _)| *kind)
            .unwrap_or(BlockKind::Undefined)
    }

    fn push_parent_frame(&mut self, kind: ParentBlockKind) {
        self.frames.push((kind, Vec::new()));
    }

    fn pop_child_blocks(&mut self) -> Vec<(BlockId, BlockKind)> {
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
            self.frames.push((BlockKind::Undefined, Vec::new()));
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
    block_stack: BlockStack,
    _marker: PhantomData<L>,
}

impl<'tcx, L: Language> GraphBuilder<'tcx, L> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            unit,
            root: None,
            block_stack: BlockStack::new(),
            _marker: PhantomData,
        }
    }

    fn reserve_block_id(&mut self) -> BlockId {
        let id = self.unit.reserve_block_id();
        self.root.get_or_insert(id);
        id
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
                let file_name = query.try_file_path();
                let block = BlockRoot::new_with(id, node, parent, children, file_name, symbol);
                block.set_meta(self.unit, node.try_scope());
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
                if let Some(type_sym) = self.unit.try_effective_type(symbol) {
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
                    query.try_name_with_field_or_first(L::name_field()),
                    symbol,
                );
                if let Some(type_sym) = self.unit.try_effective_type(symbol) {
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
                if let Some(type_sym) = self.unit.try_effective_type(symbol) {
                    block.set_type(self.unit, type_sym);
                }
                BasicBlock::Parameter(self.unit.alloc_block(id, block))
            }
            BlockKind::Return => {
                // Return blocks: symbol should already have type_of set during binding
                let mut block = BlockReturn::new_with(id, node, parent, children, symbol);
                if let Some(type_sym) = self.unit.try_effective_type(symbol) {
                    block.set_type(self.unit, type_sym);
                }
                BasicBlock::Return(self.unit.alloc_block(id, block))
            }
            BlockKind::Alias => {
                let block = BlockAlias::new_with_name(
                    id,
                    node,
                    parent,
                    children,
                    query.try_first_ident_name(),
                    symbol,
                );
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
        let id = self.reserve_block_id();
        let block_kind = self.refine_block_kind(node, block_kind);

        // Attach before visiting children so nested symbols can resolve the parent block.
        if block_kind.owns_symbol_block_id() {
            node.query(&self.unit).attach_block_id(id);
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

        self.block_stack.push_child_block(id, block_kind);
    }

    fn refine_block_kind(&self, node: HirNode<'tcx>, kind: BlockKind) -> BlockKind {
        if kind != BlockKind::Func {
            return kind;
        }

        let is_method = node.query(&self.unit).is_symbol_kind(SymKind::Method);
        let is_in_impl = self.block_stack.current_parent_kind() == BlockKind::Impl;

        if is_method || is_in_impl {
            BlockKind::Method
        } else {
            BlockKind::Func
        }
    }

    fn collect_child_blocks(
        &mut self,
        node: HirNode<'tcx>,
        parent: BlockId,
        parent_kind: ParentBlockKind,
    ) -> Vec<(BlockId, BlockKind)> {
        self.block_stack.push_parent_frame(parent_kind);
        self.visit_children(self.unit, node, parent);
        self.block_stack.pop_child_blocks()
    }

    /// Build a block whose kind is determined by parent context.
    fn build_context_block(
        &mut self,
        node: HirNode<'tcx>,
        parent: BlockId,
        block_kind: BlockKind,
        index: usize,
    ) {
        let id = self.reserve_block_id();
        let block_kind = self.refine_block_kind(node, block_kind);

        // For context-dependent blocks (like tuple struct fields), don't recurse
        let child_ids = Vec::new();

        // Create the block - for tuple struct fields, use index as name
        let block = if block_kind == BlockKind::Field {
            self.create_tuple_field_block(id, node, Some(parent), child_ids, index)
        } else {
            self.create_block(id, node, block_kind, Some(parent), child_ids)
        };

        self.unit.insert_block(id, block);

        self.block_stack.push_child_block(id, block_kind);
    }

    /// Create a field block for tuple struct with index as name
    fn create_tuple_field_block(
        &self,
        id: BlockId,
        node: HirNode<'tcx>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        index: usize,
    ) -> BasicBlock<'tcx> {
        // NOTE: Don't call set_block_id here - the node is a type_identifier that's
        // bound to the struct symbol, and we don't want to overwrite the struct's block_id
        let field_type_symbol = node.query(&self.unit).try_type_expression();

        let mut block = BlockField::new_with_name(
            id,
            node,
            parent,
            children,
            Some(index.to_string()),
            field_type_symbol,
        );
        if let Some(type_sym) = self.unit.try_effective_type(field_type_symbol) {
            block.set_type(self.unit, type_sym);
        }
        BasicBlock::Field(self.unit.alloc_block(id, block))
    }

    /// Get the effective block kind for a node, checking field first then node type.
    fn effective_block_kind(node: HirNode<'tcx>) -> BlockKind {
        let field_kind = L::block_kind(node.field_id());
        if field_kind != BlockKind::Undefined {
            field_kind
        } else {
            L::block_kind(node.kind_id())
        }
    }

    fn try_graph_block_kind(node: HirNode<'tcx>) -> Option<BlockKind> {
        let kind = Self::effective_block_kind(node);
        kind.is_graph_block().then_some(kind)
    }
}

impl<'tcx, L: Language> HirVisitor<'tcx> for GraphBuilder<'tcx, L> {
    fn visit_children(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        let parent_kind_id = node.kind_id();
        let children = node.child_ids();
        let children_vec: Vec<_> = children.iter().map(|id| unit.hir_node(*id)).collect();
        let mut tuple_field_index = 0usize;

        for child in children_vec.iter() {
            let base_kind = Self::effective_block_kind(*child);
            let context_kind =
                L::block_kind_with_parent(child.kind_id(), child.field_id(), parent_kind_id);

            if context_kind != base_kind && context_kind.is_graph_block() {
                self.build_context_block(*child, parent, context_kind, tuple_field_index);
                tuple_field_index += 1;
            } else if context_kind == BlockKind::Undefined && base_kind.is_graph_block() {
                self.visit_children(unit, *child, parent);
            } else {
                self.visit_node(unit, *child, parent);
            }
        }
    }

    fn visit_file(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = Self::try_graph_block_kind(node) {
            self.build_block(node, parent, kind, true);
        } else {
            self.visit_children(unit, node, parent);
        }
    }

    fn visit_internal(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = Self::try_graph_block_kind(node).filter(|kind| *kind != BlockKind::Root)
        {
            self.build_block(node, parent, kind, false);
        } else {
            self.visit_children(unit, node, parent);
        }
    }

    fn visit_scope(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = Self::try_graph_block_kind(node) {
            // For function/method blocks, only create a block if the scope has a symbol.
            // This filters out function pointer variable declarations in C/C++ where the
            // function_declarator node is a Scope but doesn't represent an actual function.
            if kind.requires_scope_symbol() && node.try_scope_symbol().is_none() {
                self.visit_children(unit, node, parent);
                return;
            }
            self.build_block(node, parent, kind, true);
        } else {
            self.visit_children(unit, node, parent);
        }
    }

    fn visit_ident(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = Self::try_graph_block_kind(node) {
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
