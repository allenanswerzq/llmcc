use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::marker::PhantomData;

use crate::DynError;
pub use crate::block::{BasicBlock, BlockId, BlockKind, BlockRelation};
use crate::block::{
    BlockAlias, BlockCall, BlockClass, BlockConst, BlockEnum, BlockField, BlockFunc, BlockImpl,
    BlockModule, BlockParameter, BlockReturn, BlockRoot, BlockTrait,
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
    /// Stack of parent kinds - tracks what kind of block we're currently inside
    parent_kind_stack: Vec<BlockKind>,
    _config: GraphBuildConfig,
    _marker: PhantomData<Language>,
}

impl<'tcx, Language: LanguageTrait> GraphBuilder<'tcx, Language> {
    fn new(unit: CompileUnit<'tcx>, config: GraphBuildConfig) -> Self {
        Self {
            unit,
            root: None,
            children_stack: Vec::new(),
            parent_kind_stack: Vec::new(),
            _config: config,
            _marker: PhantomData,
        }
    }

    fn next_id(&self) -> BlockId {
        self.unit.reserve_block_id()
    }

    /// Resolve type info from a symbol, following the type_of chain.
    /// Returns (type_name, type_block_id) tuple.
    fn resolve_type_info(
        &self,
        symbol: Option<&'tcx crate::symbol::Symbol>,
    ) -> (String, Option<BlockId>) {
        let sym = match symbol {
            Some(s) => s,
            None => return (String::new(), None),
        };

        // Special case: EnumVariant symbols should use their own name, not follow type_of
        // This preserves the variant name (e.g., "None", "Some") rather than the parent enum
        if sym.kind() == crate::symbol::SymKind::EnumVariant {
            let type_name = self
                .unit
                .resolve_interned_owned(sym.name)
                .unwrap_or_default();
            // No block_id for enum variants - they don't define a type
            return (type_name, None);
        }

        // First try type_of (for symbols that point to a type)
        if let Some(type_sym_id) = sym.type_of()
            && let Some(type_sym) = self.unit.opt_get_symbol(type_sym_id)
        {
            // Check if type_sym is a TypeParameter with a bound - use the bound type
            let effective_type = if type_sym.kind() == crate::symbol::SymKind::TypeParameter
                && let Some(bound_id) = type_sym.type_of()
            {
                self.unit.opt_get_symbol(bound_id).unwrap_or(type_sym)
            } else {
                type_sym
            };

            let type_name = self
                .unit
                .resolve_interned_owned(effective_type.name)
                .unwrap_or_default();
            let type_block_id = effective_type.block_id();
            return (type_name, type_block_id);
        }

        // Fallback: use the symbol directly (for cases where symbol IS the type)
        let type_name = self
            .unit
            .resolve_interned_owned(sym.name)
            .unwrap_or_default();
        let type_block_id = sym.block_id();
        (type_name, type_block_id)
    }

    /// Extract the defining symbol from a HIR node.
    /// For scoped nodes (class, func, etc.): gets symbol from scope
    /// For identifier nodes: gets the resolved symbol
    /// Returns None for nodes without an associated symbol
    fn extract_symbol(
        &self,
        node: HirNode<'tcx>,
        kind: BlockKind,
    ) -> Option<&'tcx crate::symbol::Symbol> {
        // Impl blocks reference existing type symbols, not their own
        // Don't extract symbol for impl - it will be set via relation linking
        if kind == BlockKind::Impl {
            return None;
        }

        // Try scope first (for class/func/enum etc.)
        if let Some(scope) = node.as_scope()
            && let Some(sym) = scope.opt_symbol()
        {
            return Some(sym);
        }

        // Try identifier (for fields, parameters, etc.)
        if let Some(ident) = node.find_ident(&self.unit)
            && let Some(sym) = ident.opt_symbol()
        {
            return Some(sym);
        }

        // Try children's identifiers (for self_parameter where the identifier is a child)
        for child in node.children(&self.unit) {
            if let Some(ident) = child.as_ident()
                && let Some(sym) = ident.opt_symbol()
            {
                return Some(sym);
            }
        }

        None
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
                let block = BlockRoot::new_with_symbol(
                    id,
                    node,
                    parent,
                    children,
                    file_name.clone(),
                    symbol,
                );

                // Populate crate_name and module_path from scope chain
                // Populate crate_name and module_path from scope chain.
                // The binding phase sets up proper parent scopes with Module/Crate symbols.
                if let Some(scope_node) = node.as_scope()
                    && let Some(scope) = scope_node.opt_scope()
                {
                    use crate::symbol::SymKind;

                    if let Some(crate_sym) = scope.find_parent_by_kind(SymKind::Crate)
                        && let Some(name) = self.unit.cc.interner.resolve_owned(crate_sym.name)
                    {
                        block.set_crate_name(name);
                    }

                    if let Some(module_sym) = scope.find_parent_by_kind(SymKind::Module)
                        && let Some(name) = self.unit.cc.interner.resolve_owned(module_sym.name)
                    {
                        block.set_module_path(name);
                    }
                }

                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, block);
                BasicBlock::Root(block_ref)
            }
            BlockKind::Func | BlockKind::Method => {
                let block = BlockFunc::new_with_symbol(id, node, kind, parent, children, symbol);
                if kind == BlockKind::Method {
                    block.set_is_method(true);
                }
                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, block);
                BasicBlock::Func(block_ref)
            }
            BlockKind::Class => {
                let block = BlockClass::new_with_symbol(id, node, parent, children, symbol);
                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, block);
                BasicBlock::Class(block_ref)
            }
            BlockKind::Trait => {
                let block = BlockTrait::new_with_symbol(id, node, parent, children, symbol);
                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, block);
                BasicBlock::Trait(block_ref)
            }
            BlockKind::Call => {
                // For call blocks, symbol is the callee (if resolved)
                let stmt = BlockCall::new_with_symbol(id, node, parent, children, symbol);
                // Set callee from resolved symbol
                if let Some(callee_sym) = node.ident_symbol(&self.unit)
                    && let Some(callee_block_id) = callee_sym.block_id()
                {
                    stmt.set_callee(callee_block_id);
                }
                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, stmt);
                BasicBlock::Call(block_ref)
            }
            BlockKind::Enum => {
                let enum_ty = BlockEnum::new_with_symbol(id, node, parent, children, symbol);
                let block_ref = self
                    .unit
                    .cc
                    .block_arena
                    .alloc_with_id(id.0 as usize, enum_ty);
                BasicBlock::Enum(block_ref)
            }
            BlockKind::Const => {
                let mut stmt = BlockConst::new_with_symbol(id, node, parent, children, symbol);
                // Find identifier name from children
                if let Some(ident) = node.find_ident(&self.unit) {
                    stmt.name = ident.name.to_string();
                }
                // Resolve and set type info
                let (type_name, type_ref) = self.resolve_type_info(symbol);
                stmt.set_type_info(type_name, type_ref);
                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, stmt);
                BasicBlock::Const(block_ref)
            }
            BlockKind::Impl => {
                // Impl blocks: resolve target and trait references using field-based access
                let mut block = BlockImpl::new(id, node, parent, children);

                // Get target type from the "type" field (e.g., `impl Foo` or `impl Trait for Foo`)
                if let Some(target_ident) = node.ident_by_field(&self.unit, Language::type_field())
                    && let Some(sym) = target_ident.opt_symbol()
                {
                    // Follow type_of chain to get the actual type symbol for block_id
                    let resolved = sym
                        .type_of()
                        .and_then(|id| self.unit.opt_get_symbol(id))
                        .unwrap_or(sym);
                    // Store original sym (which has nested_types from impl type args) not resolved
                    block.set_target_info(resolved.block_id(), Some(sym));
                }

                // Get trait from the "trait" field (e.g., `impl Trait for Foo`)
                if let Some(trait_ident) = node.ident_by_field(&self.unit, Language::trait_field())
                    && let Some(sym) = trait_ident.opt_symbol()
                {
                    // Follow type_of chain to get the actual trait symbol
                    let resolved = sym
                        .type_of()
                        .and_then(|id| self.unit.opt_get_symbol(id))
                        .unwrap_or(sym);
                    block.set_trait_info(resolved.block_id(), Some(resolved));
                }

                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, block);
                BasicBlock::Impl(block_ref)
            }
            BlockKind::Field => {
                let mut block = BlockField::new_with_symbol(id, node, parent, children, symbol);
                // Find identifier name from children using ir.rs find_ident
                if let Some(ident) = node.find_ident(&self.unit) {
                    block.name = ident.name.to_string();
                }
                // Resolve and set type info
                let (type_name, type_ref) = self.resolve_type_info(symbol);
                block.set_type_info(type_name, type_ref);
                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, block);
                BasicBlock::Field(block_ref)
            }
            BlockKind::Parameter => {
                let mut block = BlockParameter::new_with_symbol(id, node, parent, children, symbol);
                // Find identifier name from children using ir.rs find_ident
                if let Some(ident) = node.find_ident(&self.unit) {
                    block.name = ident.name.to_string();
                } else if let Some(text) = node.find_text(&self.unit) {
                    // Fallback: look for text nodes like "self" keyword
                    block.name = text.to_string();
                }
                let (type_name, type_ref) = self.resolve_type_info(symbol);
                block.set_type_info(type_name, type_ref);
                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, block);
                BasicBlock::Parameter(block_ref)
            }
            BlockKind::Return => {
                // Return blocks: symbol should already have type_of set during binding
                let mut block = BlockReturn::new_with_symbol(id, node, parent, children, symbol);
                let (type_name, type_ref) = self.resolve_type_info(symbol);
                block.set_type_info(type_name, type_ref);
                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, block);
                BasicBlock::Return(block_ref)
            }
            BlockKind::Alias => {
                let mut block = BlockAlias::new_with_symbol(id, node, parent, children, symbol);
                // Find identifier name from children
                if let Some(ident) = node.find_ident(&self.unit) {
                    block.name = ident.name.to_string();
                }
                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, block);
                BasicBlock::Alias(block_ref)
            }
            BlockKind::Module => {
                // Get module name from identifier
                let name = node
                    .find_ident(&self.unit)
                    .map(|ident| ident.name.to_string())
                    .unwrap_or_default();
                // Inline modules have children (the module body), file modules don't
                let is_inline = !children.is_empty();
                let block = BlockModule::new_with_symbol(
                    id, node, parent, children, name, is_inline, symbol,
                );
                let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, block);
                BasicBlock::Module(block_ref)
            }
            _ => {
                panic!("unknown block kind: {kind}")
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
        let mut block_kind = if field_kind != BlockKind::Undefined {
            field_kind
        } else {
            Language::block_kind(node.kind_id())
        };
        assert_ne!(block_kind, BlockKind::Undefined);

        // Override Func -> Method if:
        // 1. Symbol kind is Method (set during collection phase), OR
        // 2. Parent block is an Impl (handles field/method name collision case)
        if block_kind == BlockKind::Func {
            let is_method_by_symbol = node
                .opt_symbol()
                .is_some_and(|sym| sym.kind() == crate::symbol::SymKind::Method);

            // Check if parent is an impl block using parent_kind_stack
            let is_in_impl = self
                .parent_kind_stack
                .last()
                .is_some_and(|&k| k == BlockKind::Impl);

            if is_method_by_symbol || is_in_impl {
                block_kind = BlockKind::Method;
            }
        }

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
            self.parent_kind_stack.push(block_kind);
            self.visit_children(self.unit, node, id);
            self.parent_kind_stack.pop();
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

        // For tuple fields, the node is the type itself. Find the type symbol.
        // Strategy 1: Use find_ident to get the identifier and its symbol
        let mut type_symbol = node
            .find_ident(&self.unit)
            .and_then(|ident| ident.opt_symbol());

        // Strategy 2: Look at children for symbol
        if type_symbol.is_none() {
            for child in node.children(&self.unit) {
                if let Some(sym) = child.opt_symbol() {
                    type_symbol = Some(sym);
                    break;
                }
            }
        }
        // Strategy 3: Node's own scope/ident
        if type_symbol.is_none()
            && let Some(scope) = node.as_scope()
            && let Some(ident) = *scope.ident.read()
        {
            type_symbol = ident.opt_symbol();
        }
        // Strategy 4: Node's own symbol
        if type_symbol.is_none() {
            type_symbol = node.opt_symbol();
        }

        let mut block = BlockField::new_with_symbol(id, node, parent, children, type_symbol);
        block.name = index.to_string();
        // Resolve and set type info
        let (type_name, type_ref) = self.resolve_type_info(type_symbol);
        block.set_type_info(type_name, type_ref);
        let block_ref = self.unit.cc.block_arena.alloc_with_id(id.0 as usize, block);
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
                // Note: target and trait_ref are resolved in connect_blocks (graph.rs)
                // where all blocks are built and cross-file references work
            }
            _ => {}
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
        let children_vec: Vec<_> = children.iter().map(|id| unit.hir_node(*id)).collect();
        let mut tuple_field_index = 0usize;

        // Note: Test items (#[test] functions, #[cfg(test)] modules) are already filtered out
        // at the HIR building stage in ir_builder.rs, so they won't appear in children_vec.

        for child in children_vec.iter() {
            // Check for context-dependent blocks (like tuple struct fields)
            // Only intercept if the parent context changes the block kind
            let base_kind = Self::effective_block_kind(*child);
            let context_kind =
                Language::block_kind_with_parent(child.kind_id(), child.field_id(), parent_kind_id);

            if context_kind != base_kind && Self::is_block_kind(context_kind) {
                // Parent context creates a block that wouldn't exist otherwise
                // For tuple struct fields, pass the index as the name
                self.build_block_with_kind_and_index(
                    unit,
                    *child,
                    parent,
                    context_kind,
                    tuple_field_index,
                );
                tuple_field_index += 1;
            } else if context_kind == BlockKind::Undefined && Self::is_block_kind(base_kind) {
                // Parent context suppresses block creation (e.g., return_type inside function_type)
                // Just visit children without creating a block
                self.visit_children(unit, *child, parent);
            } else {
                // Normal path - let visit_node handle it
                self.visit_node(unit, *child, parent);
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
) -> Result<Option<UnitGraph>, DynError> {
    let root_hir = unit.file_root_id().ok_or("missing file start HIR id")?;
    let mut builder = GraphBuilder::<L>::new(unit, config);
    let root_node = unit.hir_node(root_hir);
    builder.visit_node(unit, root_node, BlockId::ROOT_PARENT);

    // Empty files or files with no blocks produce no root - this is OK, just skip them
    match builder.root {
        Some(root_block) => Ok(Some(UnitGraph::new(unit_index, root_block))),
        None => Ok(None),
    }
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
            .into_iter()
            .flatten()
            .collect()
    } else {
        (0..cc.get_files().len())
            .into_par_iter()
            .map(|index| {
                let unit = cc.compile_unit(index);
                build_unit_graph::<L>(unit, index, GraphBuildConfig)
            })
            .collect::<Result<Vec<_>, DynError>>()?
            .into_iter()
            .flatten()
            .collect()
    };

    // No sorting needed: DashMap provides O(1) lookup by ID

    Ok(unit_graphs)
}
