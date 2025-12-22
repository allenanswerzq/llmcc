use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::marker::PhantomData;

use crate::DynError;
pub use crate::block::{BasicBlock, BlockId, BlockKind, BlockRelation};
use crate::block::{
    BlockCall, BlockClass, BlockConst, BlockEnum, BlockField, BlockFunc, BlockImpl,
    BlockParameter, BlockReturn, BlockRoot, BlockTrait,
};
use crate::context::{CompileCtxt, CompileUnit};
use crate::graph::UnitGraph;
use crate::ir::HirNode;
use crate::lang_def::LanguageTrait;
use crate::visit::HirVisitor;

#[derive(Debug, Clone, Copy, Default)]
pub struct GraphBuildConfig;

#[derive(Debug, Clone, Copy, Default)]
pub struct GraphBuildOption {
    pub sequential: bool,
}

impl GraphBuildOption {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sequential(mut self, sequential: bool) -> Self {
        self.sequential = sequential;
        self
    }
}

#[derive(Debug)]
struct GraphBuilder<'tcx, Language> {
    unit: CompileUnit<'tcx>,
    root: Option<BlockId>,
    /// Stack of children being collected. Each entry is (BlockId, BlockKind) pairs.
    children_stack: Vec<Vec<(BlockId, BlockKind)>>,
    _config: GraphBuildConfig,
    _marker: PhantomData<Language>,
}

impl<'tcx, Language: LanguageTrait> GraphBuilder<'tcx, Language> {
    fn new(unit: CompileUnit<'tcx>, config: GraphBuildConfig) -> Self {
        Self {
            unit,
            root: None,
            children_stack: Vec::new(),
            _config: config,
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
        node.set_block_id(id);
        match kind {
            BlockKind::Root => {
                let file_name = node.as_file().map(|file| file.file_path.clone());
                let block = BlockRoot::new(id, node, parent, children, file_name);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Root(block_ref)
            }
            BlockKind::Func | BlockKind::Method => {
                let block = BlockFunc::new(id, node, kind, parent, children);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Func(block_ref)
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
                let mut block = BlockField::new(id, node, parent, children);
                // Find identifier name from children using ir.rs find_ident
                if let Some(ident) = node.find_ident(&self.unit) {
                    block.name = ident.name.clone();
                }
                // Populate type info from symbol
                self.populate_type_info(&block.base, node);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Field(block_ref)
            }
            BlockKind::Parameter => {
                let mut block = BlockParameter::new(id, node, parent, children);
                // Find identifier name from children using ir.rs find_ident
                if let Some(ident) = node.find_ident(&self.unit) {
                    block.name = ident.name.clone();
                } else if let Some(text) = node.find_text(&self.unit) {
                    // Fallback: look for text nodes like "self" keyword
                    block.name = text.to_string();
                }
                // Populate type info from symbol
                self.populate_type_info(&block.base, node);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Parameter(block_ref)
            }
            BlockKind::Return => {
                let block = BlockReturn::new(id, node, parent, children);
                // Populate type info from symbol
                self.populate_type_info(&block.base, node);
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Return(block_ref)
            }
            _ => {
                panic!("unknown block kind: {}", kind)
            }
        }
    }

    fn build_block(
        &mut self,
        _unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        parent: BlockId,
        recursive: bool,
    ) {
        let id = self.next_id();
        // Try field-based block_kind first, then fall back to node-based
        let field_kind = Language::block_kind(node.field_id());
        let block_kind = if field_kind != BlockKind::Undefined {
            field_kind
        } else {
            Language::block_kind(node.kind_id())
        };
        assert_ne!(block_kind, BlockKind::Undefined);

        if self.root.is_none() {
            self.root = Some(id);
        }

        let children_with_kinds = if recursive {
            self.children_stack.push(Vec::new());
            self.visit_children(self.unit, node, id);
            self.children_stack.pop().unwrap()
        } else {
            Vec::new()
        };

        let child_ids: Vec<BlockId> = children_with_kinds.iter().map(|(id, _)| *id).collect();
        let block = self.create_block(id, node, block_kind, Some(parent), child_ids);
        self.populate_block_fields(node, &block, &children_with_kinds);
        self.unit.insert_block(id, block, parent);

        if let Some(children) = self.children_stack.last_mut() {
            children.push((id, block_kind));
        }
    }

    /// Populate block-specific fields
    fn populate_block_fields(
        &self,
        _node: HirNode<'tcx>,
        block: &BasicBlock<'tcx>,
        children: &[(BlockId, BlockKind)],
    ) {
        match block {
            BasicBlock::Func(func) => {
                for &(child_id, child_kind) in children {
                    match child_kind {
                        BlockKind::Parameter => func.add_parameter(child_id),
                        BlockKind::Call => func.add_call(child_id),
                        _ => {}
                    }
                }
            }
            BasicBlock::Class(class) => {
                for &(child_id, child_kind) in children {
                    match child_kind {
                        BlockKind::Field => class.add_field(child_id),
                        BlockKind::Func | BlockKind::Method => class.add_method(child_id),
                        _ => {}
                    }
                }
            }
            BasicBlock::Enum(enum_block) => {
                for &(child_id, child_kind) in children {
                    if child_kind == BlockKind::Field {
                        enum_block.add_variant(child_id);
                    }
                }
            }
            BasicBlock::Trait(trait_block) => {
                for &(child_id, child_kind) in children {
                    if matches!(child_kind, BlockKind::Func | BlockKind::Method) {
                        trait_block.add_method(child_id);
                    }
                }
            }
            BasicBlock::Impl(impl_block) => {
                // Add methods to impl
                for &(child_id, child_kind) in children {
                    if matches!(child_kind, BlockKind::Func | BlockKind::Method) {
                        impl_block.add_method(child_id);
                    }
                }
            }
            _ => {}
        }
    }

    /// Populate type info on BlockBase from the node.
    fn populate_type_info(&self, base: &crate::block::BlockBase<'tcx>, node: HirNode<'tcx>) {
        // Strategy 1: Look at children for identifier with type_of symbol
        // Works for fields and parameters where the name identifier has type_of set
        for child in node.children(&self.unit) {
            if let Some(child_sym) = child.opt_symbol() {
                if let Some(type_sym_id) = child_sym.type_of() {
                    if let Some(type_sym) = self.unit.cc.opt_get_symbol(type_sym_id) {
                        if let Some(type_name) = self.unit.cc.interner.resolve_owned(type_sym.name) {
                            base.set_type_name(type_name);
                        }
                        if let Some(type_block_id) = type_sym.block_id() {
                            base.set_type_ref(type_block_id);
                        }
                        return;
                    }
                }
            }
        }

        // Strategy 2: Node's own scope/ident - for return types where node IS the type
        if let Some(scope) = node.as_scope() {
            if let Some(ident) = *scope.ident.read() {
                base.set_type_name(ident.name.clone());
                if let Some(sym) = ident.opt_symbol() {
                    // First try type_of (for Self/self which point to the struct)
                    if let Some(type_sym_id) = sym.type_of() {
                        if let Some(type_sym) = self.unit.cc.opt_get_symbol(type_sym_id) {
                            if let Some(type_block_id) = type_sym.block_id() {
                                base.set_type_ref(type_block_id);
                                return;
                            }
                        }
                    }
                    // Otherwise use direct block_id
                    if let Some(type_block_id) = sym.block_id() {
                        if type_block_id != base.id {
                            base.set_type_ref(type_block_id);
                        }
                    }
                }
                return;
            }
        }

        // Strategy 3: Direct symbol on the node - fallback
        if let Some(type_sym) = node.opt_symbol() {
            if let Some(type_name) = self.unit.cc.interner.resolve_owned(type_sym.name) {
                base.set_type_name(type_name);
            }
            // First try type_of (for Self/self which point to the struct)
            if let Some(type_of_id) = type_sym.type_of() {
                if let Some(type_of_sym) = self.unit.cc.opt_get_symbol(type_of_id) {
                    if let Some(type_block_id) = type_of_sym.block_id() {
                        base.set_type_ref(type_block_id);
                        return;
                    }
                }
            }
            // Otherwise use direct block_id
            if let Some(type_block_id) = type_sym.block_id() {
                if type_block_id != base.id {
                    base.set_type_ref(type_block_id);
                }
            }
        }
    }

    /// Get the effective block kind for a node, checking field first then node type.
    fn effective_block_kind(node: HirNode<'tcx>) -> BlockKind {
        let field_kind = Language::block_kind(node.field_id());
        if field_kind != BlockKind::Undefined {
            field_kind
        } else {
            Language::block_kind(node.kind_id())
        }
    }

    /// Check if a block kind should trigger block creation.
    fn is_block_kind(kind: BlockKind) -> bool {
        matches!(
            kind,
            BlockKind::Func
                | BlockKind::Method
                | BlockKind::Class
                | BlockKind::Trait
                | BlockKind::Enum
                | BlockKind::Const
                | BlockKind::Impl
                | BlockKind::Field
                | BlockKind::Parameter
                | BlockKind::Return
                | BlockKind::Call
                | BlockKind::Root
        )
    }
}

impl<'tcx, Language: LanguageTrait> HirVisitor<'tcx> for GraphBuilder<'tcx, Language> {
    fn visit_file(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        self.children_stack.push(Vec::new());
        self.build_block(unit, node, parent, true);
    }

    fn visit_internal(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        let kind = Self::effective_block_kind(node);
        if Self::is_block_kind(kind) && kind != BlockKind::Root {
            self.build_block(unit, node, parent, false);
        } else {
            self.visit_children(unit, node, parent);
        }
    }

    fn visit_scope(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        let kind = Self::effective_block_kind(node);
        if Self::is_block_kind(kind) {
            self.build_block(unit, node, parent, true);
        } else {
            self.visit_children(unit, node, parent);
        }
    }

    fn visit_ident(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        let kind = Self::effective_block_kind(node);
        if Self::is_block_kind(kind) {
            self.build_block(unit, node, parent, false);
        } else {
            self.visit_children(unit, node, parent);
        }
    }
}

pub fn build_unit_graph<'tcx, L: LanguageTrait>(
    unit: CompileUnit<'tcx>,
    unit_index: usize,
    config: GraphBuildConfig,
) -> Result<UnitGraph, DynError> {
    let root_hir = unit.file_root_id().ok_or("missing file start HIR id")?;
    let mut builder = GraphBuilder::<L>::new(unit, config);
    let root_node = unit.hir_node(root_hir);
    builder.visit_node(unit, root_node, BlockId::ROOT_PARENT);

    let root_block = builder
        .root
        .ok_or("graph builder produced no root block")?;
    Ok(UnitGraph::new(
        unit_index,
        root_block,
    ))
}

/// Build unit graphs for all compilation units in parallel.
pub fn build_llmcc_graph<'tcx, L: LanguageTrait>(
    cc: &'tcx CompileCtxt<'tcx>,
    config: GraphBuildOption,
) -> Result<Vec<UnitGraph>, DynError> {
    let unit_graphs: Vec<UnitGraph> = if config.sequential {
        (0..cc.get_files().len())
            .map(|index| {
                let unit = cc.compile_unit(index);
                build_unit_graph::<L>(unit, index, GraphBuildConfig)
            })
            .collect::<Result<Vec<_>, DynError>>()?
    } else {
        (0..cc.get_files().len())
            .into_par_iter()
            .map(|index| {
                let unit = cc.compile_unit(index);
                build_unit_graph::<L>(unit, index, GraphBuildConfig)
            })
            .collect::<Result<Vec<_>, DynError>>()?
    };

    // Sort blocks by ID for consistent lookup
    cc.block_arena.bb_sort_by(|block| block.id());

    Ok(unit_graphs)
}
