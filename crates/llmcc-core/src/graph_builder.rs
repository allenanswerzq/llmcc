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
use crate::ir::{HirIdent, HirNode};
use crate::lang_def::Language;
use crate::symbol::{SymKind, Symbol};
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

#[derive(Debug)]
struct ChildBlock {
    id: BlockId,
    kind: BlockKind,
}

impl ChildBlock {
    fn new(id: BlockId, kind: BlockKind) -> Self {
        Self { id, kind }
    }
}

#[derive(Debug)]
struct BlockFrame {
    kind: BlockKind,
    children: Vec<ChildBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChildTraversal {
    BuildContextBlock(BlockKind),
    TraverseChildren,
    VisitNode,
}

#[derive(Debug)]
struct BlockTypeInfo {
    name: String,
    type_ref: Option<BlockId>,
}

impl BlockTypeInfo {
    fn none() -> Self {
        Self {
            name: String::new(),
            type_ref: None,
        }
    }
}

#[derive(Debug)]
struct GraphBuilder<'tcx, L> {
    unit: CompileUnit<'tcx>,
    root: Option<BlockId>,
    block_stack: Vec<BlockFrame>,
    _marker: PhantomData<L>,
}

impl<'tcx, L: Language> GraphBuilder<'tcx, L> {
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            unit,
            root: None,
            block_stack: Vec::new(),
            _marker: PhantomData,
        }
    }

    fn reserve_block_id(&mut self) -> BlockId {
        let id = self.unit.reserve_block_id();
        self.root.get_or_insert(id);
        id
    }

    fn register_child_block(&mut self, id: BlockId, kind: BlockKind) {
        if let Some(frame) = self.block_stack.last_mut() {
            frame.children.push(ChildBlock::new(id, kind));
        }
    }

    /// Resolve type info from a symbol, following the type_of chain.
    fn resolve_type_info(&self, symbol: Option<&'tcx Symbol>) -> BlockTypeInfo {
        let sym = match symbol {
            Some(s) => s,
            None => return BlockTypeInfo::none(),
        };

        // Special case: EnumVariant symbols don't have a type - they ARE the enum's members
        // Don't show @type for enum variants
        if sym.kind() == SymKind::EnumVariant {
            return BlockTypeInfo::none();
        }

        // First try type_of (for symbols that point to a type)
        if let Some(type_sym_id) = sym.type_of()
            && let Some(type_sym) = self.unit.try_symbol(type_sym_id)
        {
            // Check if type_sym is a TypeParameter with a bound - use the bound type
            let effective_type = if type_sym.kind() == SymKind::TypeParameter
                && let Some(bound_id) = type_sym.type_of()
            {
                self.unit.try_symbol(bound_id).unwrap_or(type_sym)
            } else {
                type_sym
            };

            let type_name = self
                .unit
                .resolve_interned_owned(effective_type.name)
                .unwrap_or_default();
            let type_block_id = effective_type.block_id();
            return BlockTypeInfo {
                name: type_name,
                type_ref: type_block_id,
            };
        }

        // Fallback: use the symbol directly (for cases where symbol IS the type)
        let type_name = self
            .unit
            .resolve_interned_owned(sym.name)
            .unwrap_or_default();
        let type_block_id = sym.block_id();
        BlockTypeInfo {
            name: type_name,
            type_ref: type_block_id,
        }
    }

    fn first_ident_name(&self, node: HirNode<'tcx>) -> Option<String> {
        node.query(&self.unit)
            .first_ident()
            .map(|ident| ident.name.to_string())
    }

    fn field_name(&self, node: HirNode<'tcx>) -> Option<String> {
        node.query(&self.unit)
            .ident_with_field(L::name_field())
            .map(|ident| ident.name.to_string())
            .or_else(|| self.first_ident_name(node))
    }

    fn parameter_name(&self, node: HirNode<'tcx>) -> Option<String> {
        self.variable_ident(node)
            .map(|ident| ident.name.to_string())
            .or_else(|| self.first_ident_name(node))
            .or_else(|| node.query(&self.unit).text().map(|text| text.to_string()))
    }

    fn ident_symbol_with_field(&self, node: HirNode<'tcx>, field_id: u16) -> Option<&'tcx Symbol> {
        node.query(&self.unit)
            .ident_with_field(field_id)
            .and_then(|ident| ident.try_symbol())
    }

    fn variable_ident(&self, node: HirNode<'tcx>) -> Option<&'tcx HirIdent<'tcx>> {
        node.query(&self.unit)
            .identifiers()
            .into_iter()
            .find(|ident| {
                ident
                    .try_symbol()
                    .is_some_and(|sym| sym.kind() == SymKind::Variable)
            })
    }

    fn first_ident_symbol(&self, node: HirNode<'tcx>) -> Option<&'tcx Symbol> {
        node.query(&self.unit)
            .first_ident()
            .and_then(|ident| ident.try_symbol())
    }

    fn first_child_ident_symbol(&self, node: HirNode<'tcx>) -> Option<&'tcx Symbol> {
        node.children(&self.unit)
            .into_iter()
            .find_map(|child| child.try_ident_symbol())
    }

    /// Extract the symbol represented by a block node.
    fn extract_symbol(&self, node: HirNode<'tcx>, kind: BlockKind) -> Option<&'tcx Symbol> {
        if kind == BlockKind::Impl {
            return None;
        }

        let scope_symbol = node.try_scope_symbol();
        match kind {
            BlockKind::Func | BlockKind::Method => scope_symbol,
            BlockKind::Field => scope_symbol
                .or_else(|| self.ident_symbol_with_field(node, L::name_field()))
                .or_else(|| self.first_ident_symbol(node))
                .or_else(|| self.first_child_ident_symbol(node)),
            BlockKind::Parameter => scope_symbol
                .or_else(|| {
                    self.variable_ident(node)
                        .and_then(|ident| ident.try_symbol())
                })
                .or_else(|| self.first_ident_symbol(node))
                .or_else(|| self.first_child_ident_symbol(node)),
            _ => scope_symbol
                .or_else(|| self.first_ident_symbol(node))
                .or_else(|| self.first_child_ident_symbol(node)),
        }
    }

    fn populate_root_metadata(&self, node: HirNode<'tcx>, block: &BlockRoot<'tcx>) {
        let Some(scope) = node.try_scope() else {
            return;
        };

        let meta = self.unit.unit_meta();
        if let Some(ref pkg_name) = meta.package_name {
            block.set_crate_name(pkg_name.clone());
        }
        if let Some(ref pkg_root) = meta.package_root {
            block.set_crate_root(pkg_root.display().to_string());
        }
        if let Some(ref mod_name) = meta.module_name {
            block.set_module_path(mod_name.clone());
        }
        if let Some(ref mod_root) = meta.module_root {
            block.set_module_root(mod_root.display().to_string());
        }

        if meta.package_name.is_none()
            && let Some(crate_sym) = scope.try_parent_symbol(SymKind::Crate)
            && let Some(name) = self.unit.context().interner().resolve_owned(crate_sym.name)
        {
            block.set_crate_name(name);
        }
        if meta.module_name.is_none()
            && let Some(module_sym) = scope.try_parent_symbol(SymKind::Module)
            && let Some(name) = self
                .unit
                .context()
                .interner()
                .resolve_owned(module_sym.name)
        {
            block.set_module_path(name);
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
        // Extract symbol for this block (if applicable)
        let symbol = self.extract_symbol(node, kind);

        // NOTE: block_id is set on the node's symbol in build_block() BEFORE visiting children
        // This allows children to resolve their parent's type (e.g., enum variants -> enum)
        match kind {
            BlockKind::Root => {
                // Get file path from HirFile node or from compile unit
                let file_name = node
                    .as_file()
                    .map(|file| file.file_path.clone())
                    .or_else(|| self.unit.file_path().map(|s| s.to_string()));
                let block =
                    BlockRoot::new_with(id, node, parent, children, file_name.clone(), symbol);
                self.populate_root_metadata(node, &block);

                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, block);
                BasicBlock::Root(block_ref)
            }
            BlockKind::Func | BlockKind::Method => {
                let block = BlockFunc::new_with(id, node, kind, parent, children, symbol);
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, block);
                BasicBlock::Func(block_ref)
            }
            BlockKind::Class => {
                let block = BlockClass::new_with(id, node, parent, children, symbol);
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, block);
                BasicBlock::Class(block_ref)
            }
            BlockKind::Trait => {
                let block = BlockTrait::new_with(id, node, parent, children, symbol);
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, block);
                BasicBlock::Trait(block_ref)
            }
            BlockKind::Interface => {
                let block = BlockInterface::new_with(id, node, parent, children, symbol);
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, block);
                BasicBlock::Interface(block_ref)
            }
            BlockKind::Call => {
                // For call blocks, symbol is the callee (if resolved)
                let stmt = BlockCall::new_with(id, node, parent, children, symbol);
                // Set callee from resolved symbol
                if let Some(callee_sym) = node.query(&self.unit).resolved_symbol()
                    && let Some(callee_block_id) = callee_sym.block_id()
                {
                    stmt.set_callee(callee_block_id);
                }
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, stmt);
                BasicBlock::Call(block_ref)
            }
            BlockKind::Enum => {
                let enum_ty = BlockEnum::new_with(id, node, parent, children, symbol);
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, enum_ty);
                BasicBlock::Enum(block_ref)
            }
            BlockKind::Const => {
                let mut stmt = BlockConst::new_with_name(
                    id,
                    node,
                    parent,
                    children,
                    self.first_ident_name(node),
                    symbol,
                );
                let type_info = self.resolve_type_info(symbol);
                stmt.set_type_info(type_info.name, type_info.type_ref);
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, stmt);
                BasicBlock::Const(block_ref)
            }
            BlockKind::Impl => {
                // Impl blocks: resolve target and trait references using field-based access
                let mut block = BlockImpl::new(id, node, parent, children);

                // Get target type from the "type" field (e.g., `impl Foo` or `impl Trait for Foo`)
                if let Some(sym) = self.ident_symbol_with_field(node, L::type_field()) {
                    // Follow type_of chain to get the actual type symbol for block_id
                    let resolved = sym
                        .type_of()
                        .and_then(|id| self.unit.try_symbol(id))
                        .unwrap_or(sym);
                    // Store original sym (which has nested_types from impl type args) not resolved
                    block.set_target_info(resolved.block_id(), Some(sym));
                }

                // Get trait from the "trait" field (e.g., `impl Trait for Foo`)
                if let Some(sym) = self.ident_symbol_with_field(node, L::trait_field()) {
                    // Follow type_of chain to get the actual trait symbol
                    let resolved = sym
                        .type_of()
                        .and_then(|id| self.unit.try_symbol(id))
                        .unwrap_or(sym);
                    block.set_trait_info(resolved.block_id(), Some(resolved));
                }

                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, block);
                BasicBlock::Impl(block_ref)
            }
            BlockKind::Field => {
                let mut block = BlockField::new_with_name(
                    id,
                    node,
                    parent,
                    children,
                    self.field_name(node),
                    symbol,
                );
                let type_info = self.resolve_type_info(symbol);
                block.set_type_info(type_info.name, type_info.type_ref);
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, block);
                BasicBlock::Field(block_ref)
            }
            BlockKind::Parameter => {
                let mut block = BlockParameter::new_with_name(
                    id,
                    node,
                    parent,
                    children,
                    self.parameter_name(node),
                    symbol,
                );
                let type_info = self.resolve_type_info(symbol);
                block.set_type_info(type_info.name, type_info.type_ref);
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, block);
                BasicBlock::Parameter(block_ref)
            }
            BlockKind::Return => {
                // Return blocks: symbol should already have type_of set during binding
                let mut block = BlockReturn::new_with(id, node, parent, children, symbol);
                let type_info = self.resolve_type_info(symbol);
                block.set_type_info(type_info.name, type_info.type_ref);
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, block);
                BasicBlock::Return(block_ref)
            }
            BlockKind::Alias => {
                let block = BlockAlias::new_with_name(
                    id,
                    node,
                    parent,
                    children,
                    self.first_ident_name(node),
                    symbol,
                );
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, block);
                BasicBlock::Alias(block_ref)
            }
            BlockKind::Module => {
                let name = self.first_ident_name(node).unwrap_or_default();
                // Inline modules have children (the module body), file modules don't
                let is_inline = !children.is_empty();
                let block =
                    BlockModule::new_with(id, node, parent, children, name, is_inline, symbol);
                let block_ref = self
                    .unit
                    .context()
                    .block_arena
                    .alloc_with_id(id.0 as usize, block);
                BasicBlock::Module(block_ref)
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

        // Set block_id on the node's symbol BEFORE visiting children
        // This allows children to resolve their parent's type (e.g., enum variants -> enum)
        // Don't set for impl blocks - they reference existing type symbols
        // Don't set for return blocks - the return type node's symbol belongs to the type definition
        if block_kind != BlockKind::Impl && block_kind != BlockKind::Return {
            node.query(&self.unit).attach_block_id(id);
        }

        let children_with_kinds = if recursive {
            self.collect_child_blocks(node, id, block_kind)
        } else {
            Vec::new()
        };

        let child_ids: Vec<BlockId> = children_with_kinds.iter().map(|child| child.id).collect();
        let block = self.create_block(id, node, block_kind, Some(parent), child_ids);
        self.populate_block_fields(node, &block, &children_with_kinds);
        self.unit.insert_block(id, block, parent);

        self.register_child_block(id, block_kind);
    }

    fn refine_block_kind(&self, node: HirNode<'tcx>, kind: BlockKind) -> BlockKind {
        if kind != BlockKind::Func {
            return kind;
        }

        let is_method_by_symbol = node
            .query(&self.unit)
            .symbol()
            .is_some_and(|sym| sym.kind() == SymKind::Method);
        let is_in_impl = self
            .block_stack
            .last()
            .is_some_and(|frame| frame.kind == BlockKind::Impl);

        if is_method_by_symbol || is_in_impl {
            BlockKind::Method
        } else {
            BlockKind::Func
        }
    }

    fn collect_child_blocks(
        &mut self,
        node: HirNode<'tcx>,
        parent: BlockId,
        parent_kind: BlockKind,
    ) -> Vec<ChildBlock> {
        self.block_stack.push(BlockFrame {
            kind: parent_kind,
            children: Vec::new(),
        });
        self.visit_children(self.unit, node, parent);
        self.block_stack
            .pop()
            .expect("block frame stack must be balanced")
            .children
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

        self.unit.insert_block(id, block, parent);

        self.register_child_block(id, block_kind);
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
        let type_symbol = self.tuple_field_type_symbol(node);

        let mut block = BlockField::new_with_name(
            id,
            node,
            parent,
            children,
            Some(index.to_string()),
            type_symbol,
        );
        let type_info = self.resolve_type_info(type_symbol);
        block.set_type_info(type_info.name, type_info.type_ref);
        let block_ref = self
            .unit
            .context()
            .block_arena
            .alloc_with_id(id.0 as usize, block);
        BasicBlock::Field(block_ref)
    }

    fn tuple_field_type_symbol(&self, node: HirNode<'tcx>) -> Option<&'tcx Symbol> {
        node.query(&self.unit)
            .first_ident()
            .and_then(|ident| ident.try_symbol())
            .or_else(|| {
                node.children(&self.unit)
                    .into_iter()
                    .find_map(|child| child.query(&self.unit).symbol())
            })
            .or_else(|| node.try_scope_ident_symbol())
            .or_else(|| node.query(&self.unit).symbol())
    }

    /// Populate block-specific fields
    fn populate_block_fields(
        &self,
        _node: HirNode<'tcx>,
        block: &BasicBlock<'tcx>,
        children: &[ChildBlock],
    ) {
        match block {
            BasicBlock::Func(func) => {
                for child in children {
                    match child.kind {
                        BlockKind::Parameter => func.add_parameter(child.id),
                        BlockKind::Return => func.set_return(child.id),
                        _ => {}
                    }
                }
            }
            BasicBlock::Class(class) => {
                for child in children {
                    match child.kind {
                        BlockKind::Field => class.add_field(child.id),
                        BlockKind::Func | BlockKind::Method => class.add_method(child.id),
                        _ => {}
                    }
                }
            }
            BasicBlock::Enum(enum_block) => {
                for child in children {
                    if child.kind == BlockKind::Field {
                        enum_block.add_variant(child.id);
                    }
                }
            }
            BasicBlock::Trait(trait_block) => {
                for child in children {
                    if matches!(child.kind, BlockKind::Func | BlockKind::Method) {
                        trait_block.add_method(child.id);
                    }
                }
            }
            BasicBlock::Interface(iface_block) => {
                for child in children {
                    match child.kind {
                        BlockKind::Field => iface_block.add_field(child.id),
                        BlockKind::Func | BlockKind::Method => iface_block.add_method(child.id),
                        _ => {}
                    }
                }
            }
            BasicBlock::Impl(impl_block) => {
                // Add methods to impl
                for child in children {
                    if matches!(child.kind, BlockKind::Func | BlockKind::Method) {
                        impl_block.add_method(child.id);
                    }
                }
                // Note: target and trait_ref are resolved in connect_blocks (graph.rs)
                // where all blocks are built and cross-file references work
            }
            _ => {}
        }
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

    fn graph_block_kind(node: HirNode<'tcx>) -> Option<BlockKind> {
        let kind = Self::effective_block_kind(node);
        kind.is_graph_block().then_some(kind)
    }

    fn requires_scope_symbol(kind: BlockKind) -> bool {
        matches!(kind, BlockKind::Func | BlockKind::Method)
    }

    fn child_traversal(node: HirNode<'tcx>, parent_kind_id: u16) -> ChildTraversal {
        let base_kind = Self::effective_block_kind(node);
        let context_kind =
            L::block_kind_with_parent(node.kind_id(), node.field_id(), parent_kind_id);

        if context_kind != base_kind && context_kind.is_graph_block() {
            ChildTraversal::BuildContextBlock(context_kind)
        } else if context_kind == BlockKind::Undefined && base_kind.is_graph_block() {
            ChildTraversal::TraverseChildren
        } else {
            ChildTraversal::VisitNode
        }
    }
}

impl<'tcx, L: Language> HirVisitor<'tcx> for GraphBuilder<'tcx, L> {
    fn visit_children(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        let parent_kind_id = node.kind_id();
        let children = node.child_ids();
        let children_vec: Vec<_> = children.iter().map(|id| unit.hir_node(*id)).collect();
        let mut tuple_field_index = 0usize;

        for child in children_vec.iter() {
            match Self::child_traversal(*child, parent_kind_id) {
                ChildTraversal::BuildContextBlock(kind) => {
                    self.build_context_block(*child, parent, kind, tuple_field_index);
                    tuple_field_index += 1;
                }
                ChildTraversal::TraverseChildren => self.visit_children(unit, *child, parent),
                ChildTraversal::VisitNode => self.visit_node(unit, *child, parent),
            }
        }
    }

    fn visit_file(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = Self::graph_block_kind(node) {
            self.build_block(node, parent, kind, true);
        } else {
            self.visit_children(unit, node, parent);
        }
    }

    fn visit_internal(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = Self::graph_block_kind(node).filter(|kind| *kind != BlockKind::Root) {
            self.build_block(node, parent, kind, false);
        } else {
            self.visit_children(unit, node, parent);
        }
    }

    fn visit_scope(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = Self::graph_block_kind(node) {
            // For function/method blocks, only create a block if the scope has a symbol.
            // This filters out function pointer variable declarations in C/C++ where the
            // function_declarator node is a Scope but doesn't represent an actual function.
            if Self::requires_scope_symbol(kind) && node.try_scope_symbol().is_none() {
                self.visit_children(unit, node, parent);
                return;
            }
            self.build_block(node, parent, kind, true);
        } else {
            self.visit_children(unit, node, parent);
        }
    }

    fn visit_ident(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        if let Some(kind) = Self::graph_block_kind(node) {
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
