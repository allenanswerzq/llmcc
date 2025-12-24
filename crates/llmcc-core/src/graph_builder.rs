use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::marker::PhantomData;

use crate::DynError;
pub use crate::block::{BasicBlock, BlockId, BlockKind, BlockRelation};
use crate::block::{
    BlockAlias, BlockCall, BlockClass, BlockConst, BlockEnum, BlockField, BlockFunc, BlockImpl,
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
        // NOTE: block_id is set on the node's symbol in build_block() BEFORE visiting children
        // This allows children to resolve their parent's type (e.g., enum variants -> enum)
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
                let mut stmt = BlockConst::new(id, node, parent, children);
                // Find identifier name from children
                if let Some(ident) = node.find_ident(&self.unit) {
                    stmt.name = ident.name.clone();
                }
                // Populate type info from symbol
                self.populate_type_info(&stmt.base, node);
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
            BlockKind::Alias => {
                let mut block = BlockAlias::new(id, node, parent, children);
                // Find identifier name from children
                if let Some(ident) = node.find_ident(&self.unit) {
                    block.name = ident.name.clone();
                }
                let block_ref = self.unit.cc.block_arena.alloc(block);
                BasicBlock::Alias(block_ref)
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

        // Set block_id on the node's symbol BEFORE visiting children
        // This allows children to resolve their parent's type (e.g., enum variants -> enum)
        // Don't set for impl blocks - they reference existing type symbols
        // Don't set for return blocks - the return type node's symbol belongs to the type definition
        if block_kind != BlockKind::Impl && block_kind != BlockKind::Return {
            node.set_block_id(id);
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

    /// Build a block with a pre-determined kind (used for context-dependent block creation)
    /// For tuple struct fields, the index is used as the field name.
    fn build_block_with_kind_and_index(
        &mut self,
        _unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        parent: BlockId,
        block_kind: BlockKind,
        index: usize,
    ) {
        let id = self.next_id();

        if self.root.is_none() {
            self.root = Some(id);
        }

        // For context-dependent blocks (like tuple struct fields), don't recurse
        let child_ids = Vec::new();

        // Create the block - for tuple struct fields, use index as name
        let block = if block_kind == BlockKind::Field {
            self.create_tuple_field_block(id, node, Some(parent), child_ids, index)
        } else {
            self.create_block(id, node, block_kind, Some(parent), child_ids)
        };

        self.unit.insert_block(id, block, parent);

        if let Some(children) = self.children_stack.last_mut() {
            children.push((id, block_kind));
        }
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
        let mut block = BlockField::new(id, node, parent, children);
        block.name = index.to_string();
        // Populate type info from the type node itself
        self.populate_type_info(&block.base, node);
        let block_ref = self.unit.cc.block_arena.alloc(block);
        BasicBlock::Field(block_ref)
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
                        BlockKind::Return => func.set_returns(child_id),
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

    /// Resolve a symbol by following `type_of` chains to find a concrete type with a block.
    ///
    /// This handles language-agnostic type indirection patterns:
    /// - **Type Parameters**: Generic/template parameters (Rust `T`, Java `T`, C++ `typename T`)
    /// - **Type Aliases**: Type synonyms (Rust `type`, C `typedef`, TypeScript type alias)
    ///
    /// Returns the first symbol in the chain that has a `block_id`, allowing the block
    /// graph to reference the actual definition rather than intermediate aliases.
    fn resolve_to_block_type(&self, sym: &'tcx crate::symbol::Symbol) -> &'tcx crate::symbol::Symbol {
        let mut current = sym;
        // Follow type_of chains until we find something with a block
        // or can't follow further (max 10 iterations to prevent infinite loops)
        for _ in 0..10 {
            // If current has a block, return it
            if current.block_id().is_some() {
                return current;
            }
            // Only follow type_of for indirect type references
            match current.kind() {
                crate::symbol::SymKind::TypeParameter | crate::symbol::SymKind::TypeAlias => {
                    if let Some(type_of_id) = current.type_of() {
                        if let Some(resolved) = self.unit.cc.opt_get_symbol(type_of_id) {
                            current = resolved;
                            continue;
                        }
                    }
                    return current;
                }
                _ => return current,
            }
        }
        current
    }

    /// Set type info on BlockBase from a symbol.
    ///
    /// Follows `type_of` chains to find concrete types with blocks, then sets
    /// both the display name and block reference on the target block.
    fn set_type_from_symbol(&self, base: &crate::block::BlockBase<'tcx>, sym: &'tcx crate::symbol::Symbol) -> bool {
        let display_sym = self.resolve_to_block_type(sym);

        if let Some(name) = self.unit.cc.interner.resolve_owned(display_sym.name) {
            base.set_type_name(name);
        }
        if let Some(block_id) = display_sym.block_id() {
            if block_id != base.id {
                base.set_type_ref(block_id);
            }
        }
        true
    }

    /// Populate type info on BlockBase from the node.
    ///
    /// Extracts type information by finding the resolved type symbol from the node's
    /// structure. The binding phase has already resolved type references to symbols;
    /// this method finds and uses that resolved information.
    ///
    /// Two main patterns are handled:
    /// 1. **Value declarations** (params, fields, consts): The identifier has `type_of`
    ///    pointing to its type symbol.
    /// 2. **Type references** (return types): The node contains a type identifier
    ///    that IS the resolved type symbol.
    fn populate_type_info(&self, base: &crate::block::BlockBase<'tcx>, node: HirNode<'tcx>) {
        // Pattern 1: Scoped paths (e.g., Self::Summary, pkg::Type)
        // The "name" field contains the semantically significant final component
        if let Some(name_child) = node.child_by_field(&self.unit, Language::name_field()) {
            if let Some(ident) = name_child.find_ident(&self.unit)
                && let Some(sym) = ident.opt_symbol()
            {
                self.set_type_from_symbol(base, sym);
                return;
            }
        }

        // Pattern 2: Value declarations with type annotations (x: Type, var x Type)
        // The identifier's symbol has type_of pointing to the type symbol
        for child in node.children(&self.unit) {
            if let Some(child_sym) = child.opt_symbol() {
                if let Some(type_of_id) = child_sym.type_of() {
                    if let Some(type_sym) = self.unit.cc.opt_get_symbol(type_of_id) {
                        self.set_type_from_symbol(base, type_sym);
                        return;
                    }
                }
            }
        }

        // Pattern 3: Explicit type field (: Type suffix in field declarations)
        if let Some(type_child) = node.child_by_field(&self.unit, Language::type_field()) {
            if let Some(ident) = type_child.find_ident(&self.unit)
                && let Some(sym) = ident.opt_symbol()
            {
                self.set_type_from_symbol(base, sym);
                return;
            }
        }

        // Pattern 4: Node's own identifier (for scope nodes with attached ident)
        if let Some(scope) = node.as_scope() {
            if let Some(ident) = *scope.ident.read() {
                if let Some(sym) = ident.opt_symbol() {
                    self.set_type_from_symbol(base, sym);
                    return;
                }
            }
        }

        // Pattern 5: Type reference (the node contains the type itself)
        // Find any type identifier - its symbol IS the type
        if let Some(ident) = node.find_ident(&self.unit) {
            if let Some(sym) = ident.opt_symbol() {
                self.set_type_from_symbol(base, sym);
            } else {
                // Unresolved type - just set the name for display
                base.set_type_name(ident.name.clone());
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
                | BlockKind::Alias
        )
    }
}

impl<'tcx, Language: LanguageTrait> HirVisitor<'tcx> for GraphBuilder<'tcx, Language> {
    fn visit_children(&mut self, unit: CompileUnit<'tcx>, node: HirNode<'tcx>, parent: BlockId) {
        let parent_kind_id = node.kind_id();
        let children = node.child_ids();
        let mut tuple_field_index = 0usize;
        for child_id in children {
            let child = unit.hir_node(*child_id);
            // Check for context-dependent blocks (like tuple struct fields)
            // Only intercept if the parent context changes the block kind
            let base_kind = Self::effective_block_kind(child);
            let context_kind = Language::block_kind_with_parent(child.kind_id(), child.field_id(), parent_kind_id);

            if context_kind != base_kind && Self::is_block_kind(context_kind) {
                // Parent context creates a block that wouldn't exist otherwise
                // For tuple struct fields, pass the index as the name
                self.build_block_with_kind_and_index(unit, child, parent, context_kind, tuple_field_index);
                tuple_field_index += 1;
            } else if context_kind == BlockKind::Undefined && Self::is_block_kind(base_kind) {
                // Parent context suppresses block creation (e.g., return_type inside function_type)
                // Just visit children without creating a block
                self.visit_children(unit, child, parent);
            } else {
                // Normal path - let visit_node handle it
                self.visit_node(unit, child, parent);
            }
        }
    }

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
