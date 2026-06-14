//! Query helpers for project block graphs.
//!
//! `ProjectGraph` owns graph construction and relation storage. `GraphQuery`
//! owns read-only traversal, block lookup, and type-resolution conveniences used
//! by downstream analysis and rendering crates.

use std::collections::HashSet;

use crate::block::{BasicBlock, BlockField, BlockKind, BlockRelation};
use crate::context::CompileCtxt;
use crate::graph::ProjectGraph;
use crate::id::{BlockId, SymId};
use crate::symbol::Symbol;

#[derive(Clone, Copy)]
pub struct GraphQuery<'graph, 'tcx> {
    graph: &'graph ProjectGraph<'tcx>,
}

/// A field block and all type blocks reachable from that field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldTypes {
    /// Field block id.
    pub field_id: BlockId,
    /// Direct and nested type ids referenced by the field.
    pub type_ids: Vec<BlockId>,
}

/// Generic type arguments resolved from a field declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldArgs {
    /// Declared field type id.
    pub type_id: BlockId,
    /// Resolved nested type argument ids.
    pub arg_ids: Vec<BlockId>,
    /// True when `type_id` is also present in `arg_ids`.
    pub includes_type: bool,
}

impl<'graph, 'tcx> GraphQuery<'graph, 'tcx> {
    pub fn new(graph: &'graph ProjectGraph<'tcx>) -> Self {
        Self { graph }
    }

    pub fn graph(self) -> &'graph ProjectGraph<'tcx> {
        self.graph
    }

    pub fn context(self) -> &'tcx CompileCtxt<'tcx> {
        self.graph.context()
    }

    /// Return a block by id, if it exists in the project context.
    pub fn try_block(self, block_id: BlockId) -> Option<BasicBlock<'tcx>> {
        self.context().try_block(block_id)
    }

    /// Return a block's kind, if the block exists.
    pub fn block_kind(self, block_id: BlockId) -> Option<BlockKind> {
        self.try_block(block_id).map(|block| block.kind())
    }

    /// Return true when the block exists and has `kind`.
    pub fn is_block_kind(self, block_id: BlockId, kind: BlockKind) -> bool {
        self.block_kind(block_id) == Some(kind)
    }

    /// Return true when the block exists and has any of the given kinds.
    pub fn is_any_block_kind(
        self,
        block_id: BlockId,
        kinds: impl IntoIterator<Item = BlockKind>,
    ) -> bool {
        let Some(kind) = self.block_kind(block_id) else {
            return false;
        };
        kinds.into_iter().any(|candidate| candidate == kind)
    }

    /// Return true when the block is eligible for the collected graph node set.
    pub fn is_collected_graph_node(self, block_id: BlockId) -> bool {
        let Some(block) = self.try_block(block_id) else {
            return false;
        };

        if !block.kind().is_architecture_node_kind() {
            return false;
        }

        !block.as_func().is_some_and(|func| func.is_method())
    }

    /// Return true for nominal type blocks that can be used as concrete type dependencies.
    pub fn is_nominal_type(self, block_id: BlockId) -> bool {
        self.is_any_block_kind(block_id, [BlockKind::Class, BlockKind::Enum])
    }

    /// Return blocks related to `from` by `relation`.
    pub fn related(self, from: BlockId, relation: BlockRelation) -> Vec<BlockId> {
        self.context().block_relations().related(from, relation)
    }

    /// Return sources that point to `to` with `relation`.
    pub fn reverse_related(self, to: BlockId, relation: BlockRelation) -> Vec<BlockId> {
        self.context()
            .block_relations()
            .reverse_related(to, relation)
    }

    /// Return related blocks that satisfy a caller-provided predicate.
    pub fn related_matching(
        self,
        from: BlockId,
        relation: BlockRelation,
        mut keep: impl FnMut(BlockId) -> bool,
    ) -> Vec<BlockId> {
        self.related(from, relation)
            .into_iter()
            .filter(|&block_id| keep(block_id))
            .collect()
    }

    /// Return related blocks whose kind matches `kind`.
    pub fn related_of_kind(
        self,
        from: BlockId,
        relation: BlockRelation,
        kind: BlockKind,
    ) -> Vec<BlockId> {
        self.related_matching(from, relation, |block_id| {
            self.is_block_kind(block_id, kind)
        })
    }

    /// Return whether one specific relation exists.
    pub fn contains(self, from: BlockId, relation: BlockRelation, to: BlockId) -> bool {
        self.context()
            .block_relations()
            .contains(from, relation, to)
    }

    /// Return whether any outgoing relation of this kind exists for `from`.
    pub fn has_related(self, from: BlockId, relation: BlockRelation) -> bool {
        self.context()
            .block_relations()
            .contains_relation(from, relation)
    }

    /// Return this block as a field block, if it exists and has field shape.
    pub fn try_field(self, block_id: BlockId) -> Option<&'tcx BlockField<'tcx>> {
        self.try_block(block_id).and_then(|block| block.as_field())
    }

    /// Return each field owned by `owner_id` with its direct and nested type references.
    pub fn field_types(self, owner_id: BlockId) -> Vec<FieldTypes> {
        self.related(owner_id, BlockRelation::HasField)
            .into_iter()
            .map(|field_id| FieldTypes {
                field_id,
                type_ids: self.field_type_closure(field_id),
            })
            .collect()
    }

    /// Return generic type arguments attached to a field declaration.
    pub fn field_args(self, field_id: BlockId) -> Option<FieldArgs> {
        let field = self.try_field(field_id)?;
        let type_id = field.type_ref()?;
        let nested_types = field.base.symbol?.nested_types()?;
        let arg_ids = self.nested_type_blocks(&nested_types);

        Some(FieldArgs {
            type_id,
            includes_type: arg_ids.contains(&type_id),
            arg_ids,
        })
    }

    fn field_type_closure(self, field_id: BlockId) -> Vec<BlockId> {
        let mut types = Vec::new();
        let mut visited = HashSet::new();
        self.collect_field_type_closure(field_id, &mut types, &mut visited);
        types
    }

    fn collect_field_type_closure(
        self,
        field_id: BlockId,
        types: &mut Vec<BlockId>,
        visited: &mut HashSet<BlockId>,
    ) {
        if !visited.insert(field_id) {
            tracing::debug!(
                field_id = field_id.as_u32(),
                "skipping cyclic field traversal"
            );
            return;
        }

        types.extend(self.related(field_id, BlockRelation::TypeOf));

        for nested_field_id in self.related(field_id, BlockRelation::HasField) {
            self.collect_field_type_closure(nested_field_id, types, visited);
        }
    }

    /// Return a symbol by id, if it exists.
    pub fn try_symbol(self, symbol_id: SymId) -> Option<&'tcx Symbol> {
        self.context().try_symbol(symbol_id)
    }

    /// Follow a type alias once and return the effective type symbol.
    pub fn actual_type_symbol(self, symbol: &'tcx Symbol) -> &'tcx Symbol {
        let Some(type_of_id) = symbol.type_of() else {
            return symbol;
        };

        self.try_symbol(type_of_id).unwrap_or_else(|| {
            tracing::debug!(
                symbol_id = %symbol.id(),
                target_id = %type_of_id,
                "type alias target missing while querying graph"
            );
            symbol
        })
    }

    /// Return the effective type block for a symbol id, following one type alias link.
    pub fn actual_type_block(self, symbol_id: SymId) -> Option<BlockId> {
        let symbol = self.try_symbol(symbol_id)?;
        self.actual_type_symbol(symbol).block_id()
    }

    /// Resolve nested type symbols to effective graph block ids.
    pub fn nested_type_blocks(self, nested_types: &[SymId]) -> Vec<BlockId> {
        nested_types
            .iter()
            .filter_map(|&symbol_id| self.actual_type_block(symbol_id))
            .collect()
    }

    /// Return call targets invoked by `block_id`.
    pub fn callees(self, block_id: BlockId) -> Vec<BlockId> {
        self.related(block_id, BlockRelation::Calls)
    }

    /// Return rendered type ids referenced by parameters on `block_id`.
    pub fn param_types(self, block_id: BlockId) -> Vec<BlockId> {
        self.related(block_id, BlockRelation::HasParameters)
            .into_iter()
            .flat_map(|param_id| self.related(param_id, BlockRelation::TypeOf))
            .collect()
    }

    /// Return rendered type ids referenced by returns on `block_id`.
    pub fn return_types(self, block_id: BlockId) -> Vec<BlockId> {
        self.related(block_id, BlockRelation::HasReturn)
            .into_iter()
            .flat_map(|return_id| self.related(return_id, BlockRelation::TypeOf))
            .collect()
    }

    /// Return contracts directly or indirectly implemented by `block_id`.
    pub fn contracts(self, block_id: BlockId) -> Vec<BlockId> {
        self.related(block_id, BlockRelation::HasImpl)
            .into_iter()
            .flat_map(|impl_id| self.related(impl_id, BlockRelation::Implements))
            .chain(self.related(block_id, BlockRelation::Implements))
            .collect()
    }

    /// Return base types or contracts extended by `block_id`.
    pub fn bases(self, block_id: BlockId) -> Vec<BlockId> {
        self.related(block_id, BlockRelation::Extends)
    }

    /// Return `Uses` targets that should be rendered as type dependency edges.
    pub fn type_deps(self, block_id: BlockId) -> Vec<BlockId> {
        self.related(block_id, BlockRelation::Uses)
            .into_iter()
            .filter(|&type_id| self.is_type_dep(block_id, type_id))
            .collect()
    }

    /// Return true when a `Uses` edge should be kept as a type dependency edge.
    fn is_type_dep(self, source_id: BlockId, type_id: BlockId) -> bool {
        match self.block_kind(type_id) {
            Some(BlockKind::Class | BlockKind::Enum) => true,
            Some(BlockKind::Trait) => !self
                .related(type_id, BlockRelation::UsedBy)
                .contains(&source_id),
            _ => false,
        }
    }

    /// Return generic type arguments attached to an implementation target.
    pub fn impl_args(self, block_id: BlockId) -> Vec<BlockId> {
        if !self.is_nominal_type(block_id) {
            return Vec::new();
        }

        let Some(block) = self.try_block(block_id) else {
            tracing::debug!(
                block_id = block_id.as_u32(),
                "block missing while querying implementation type arguments"
            );
            return Vec::new();
        };

        block
            .base()
            .type_deps
            .read()
            .iter()
            .copied()
            .filter(|&type_id| self.is_nominal_type(type_id))
            .collect()
    }

    /// Return annotation or decorator callable targets attached to a declaration.
    pub fn annotations(self, block_id: BlockId) -> Vec<BlockId> {
        if !self.is_nominal_type(block_id) {
            return Vec::new();
        }

        self.related_matching(block_id, BlockRelation::Uses, |target_id| {
            self.is_block_kind(target_id, BlockKind::Func)
        })
    }
}
