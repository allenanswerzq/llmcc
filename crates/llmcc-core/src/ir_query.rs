//! Query for HIR subtrees.
//!
//! `HirNode` owns the compact data-model API. `HirQuery` owns traversal and
//! semantic lookup policy that needs a `CompileUnit` to resolve child ids.

use crate::block::BlockKind;
use crate::context::CompileUnit;
use crate::id::BlockId;
use crate::ir::{HirIdent, HirKind, HirNode, HirScope};
use crate::symbol::{SymKind, Symbol};

#[derive(Clone, Copy)]
pub struct HirQuery<'hir, 'unit> {
    node: HirNode<'hir>,
    unit: &'unit CompileUnit<'hir>,
}

impl<'hir, 'unit> HirQuery<'hir, 'unit> {
    pub fn new(node: HirNode<'hir>, unit: &'unit CompileUnit<'hir>) -> Self {
        Self { node, unit }
    }

    pub fn node(self) -> HirNode<'hir> {
        self.node
    }

    /// Parent HIR node, if this node has one in the current compile unit.
    pub fn try_parent(self) -> Option<HirNode<'hir>> {
        self.node.parent().and_then(|id| self.unit.try_hir_node(id))
    }

    /// Symbol attached directly to this node.
    pub fn try_symbol(self) -> Option<&'hir Symbol> {
        if let Some(symbol) = self.node.try_scope_symbol() {
            return Some(symbol);
        }
        self.node.try_ident_symbol()
    }

    /// Symbol that should receive this node's materialized block id.
    pub fn try_block_owner_symbol(
        self,
        block_kind: BlockKind,
        positional_field: u16,
    ) -> Option<&'hir Symbol> {
        if block_kind == BlockKind::Field
            && self
                .try_position_in_parent_field(positional_field)
                .is_some()
        {
            return None;
        }

        self.node
            .try_scope_symbol()
            .or_else(|| self.node.try_scope_ident_symbol())
            .or_else(|| self.node.try_ident_symbol())
            .filter(|symbol| symbol.kind() != SymKind::Primitive)
    }

    /// Return true when this node's direct symbol has `kind`.
    pub fn is_symbol_kind(self, kind: SymKind) -> bool {
        self.try_symbol()
            .is_some_and(|symbol| symbol.kind() == kind)
    }

    /// Return the block kind after applying bound-symbol and parent-block semantics.
    pub fn resolve_block_kind(self, kind: BlockKind, parent_kind: Option<BlockKind>) -> BlockKind {
        if kind != BlockKind::Func {
            return kind;
        }

        if self.is_symbol_kind(SymKind::Method) || parent_kind == Some(BlockKind::Impl) {
            BlockKind::Method
        } else {
            BlockKind::Func
        }
    }

    /// Return true when `kind` can be materialized for this scope node.
    pub fn can_materialize_scope(self, kind: BlockKind) -> bool {
        !kind.requires_scope_symbol() || self.node.try_scope_symbol().is_some()
    }

    /// Symbol referenced by the identifier under a specific child field.
    pub fn try_resolved_by_field(self, field_id: u16) -> Option<&'hir Symbol> {
        let child = self.node.child_by_field(self.unit, field_id)?;
        child.query(self.unit).try_first_ident()?.try_symbol()
    }

    /// Best symbol associated with this subtree.
    ///
    /// Prefer the deepest/rightmost identifier that already has a symbol. This
    /// handles scoped paths where the resolved target is not the first token.
    pub fn try_resolved(self) -> Option<&'hir Symbol> {
        if let Some(ident) = self.try_resolved_ident() {
            return ident.try_symbol();
        }

        self.try_first_ident()?.try_symbol()
    }

    /// First descendant with the given tree-sitter field id.
    pub fn try_descendant_with_field(self, field_id: u16) -> Option<HirNode<'hir>> {
        if let Some(direct_child) = self.node.child_by_field(self.unit, field_id) {
            return Some(direct_child);
        }

        for child in self.node.children(self.unit) {
            if let Some(recursive_match) =
                child.query(self.unit).try_descendant_with_field(field_id)
            {
                return Some(recursive_match);
            }
        }

        None
    }

    /// First identifier in this subtree.
    ///
    /// This is intentionally shallow-first and is useful for declarations where
    /// the first identifier is usually the declared name.
    pub fn try_first_ident(self) -> Option<&'hir HirIdent<'hir>> {
        if self.node.is_kind(HirKind::Identifier) {
            return self.node.as_ident();
        }
        for child in self.node.children(self.unit) {
            if child.is_kind(HirKind::Identifier) {
                return child.as_ident();
            }
            if child.is_kind(HirKind::Internal)
                && let Some(id) = child.query(self.unit).try_first_ident()
            {
                return Some(id);
            }
        }
        None
    }

    /// Name of the first identifier in this subtree.
    pub fn try_first_ident_name(self) -> Option<String> {
        self.try_first_ident().map(|ident| ident.name.to_string())
    }

    /// File path represented by this node, falling back to the compile unit path.
    pub fn try_file_path(self) -> Option<String> {
        self.node
            .as_file()
            .map(|file| file.file_path.clone())
            .or_else(|| self.unit.file_path())
    }

    /// Position among siblings that share this node's parent field id.
    pub fn try_position_in_parent_field(self, field_id: u16) -> Option<usize> {
        if self.node.try_field_id()? != field_id {
            return None;
        }

        let node_id = self.node.try_id()?;
        let parent = self.try_parent()?;
        let mut index = 0usize;

        for child in parent.children(self.unit) {
            if child.try_field_id() == Some(field_id) {
                if child.try_id() == Some(node_id) {
                    return Some(index);
                }
                index += 1;
            }
        }

        None
    }

    /// Positional field display name, such as `0` or `1` for tuple fields.
    pub fn try_positional_field_name(self, field_id: u16) -> Option<String> {
        self.try_position_in_parent_field(field_id)
            .map(|index| index.to_string())
    }

    /// Symbol attached to the first identifier in this subtree.
    pub fn try_first_ident_symbol(self) -> Option<&'hir Symbol> {
        self.try_first_ident().and_then(|ident| ident.try_symbol())
    }

    /// Symbol attached to the first direct identifier child.
    pub fn try_first_child_ident_symbol(self) -> Option<&'hir Symbol> {
        self.node
            .children(self.unit)
            .into_iter()
            .find_map(|child| child.try_ident_symbol())
    }

    /// Identifier under `field_id`, returned as a display name.
    pub fn try_ident_name_with_field(self, field_id: u16) -> Option<String> {
        self.try_ident_with_field(field_id)
            .map(|ident| ident.name.to_string())
    }

    /// Identifier name under `field_id`, falling back to the first identifier.
    pub fn try_name_with_field_or_first(self, field_id: u16) -> Option<String> {
        self.try_ident_name_with_field(field_id)
            .or_else(|| self.try_first_ident_name())
    }

    /// Field display name from a declaration name or positional parent-field index.
    pub fn try_field_name(self, name_field: u16, positional_field: u16) -> Option<String> {
        self.try_positional_field_name(positional_field)
            .or_else(|| self.try_name_with_field_or_first(name_field))
    }

    /// Symbol attached to the identifier under `field_id`.
    pub fn try_ident_symbol_with_field(self, field_id: u16) -> Option<&'hir Symbol> {
        self.try_ident_with_field(field_id)
            .and_then(|ident| ident.try_symbol())
    }

    /// First identifier bound to a variable symbol in this subtree.
    pub fn try_variable_ident(self) -> Option<&'hir HirIdent<'hir>> {
        self.identifiers().into_iter().find(|ident| {
            ident
                .try_symbol()
                .is_some_and(|sym| sym.kind() == SymKind::Variable)
        })
    }

    /// Symbol attached to the first variable identifier in this subtree.
    pub fn try_variable_symbol(self) -> Option<&'hir Symbol> {
        self.try_variable_ident()
            .and_then(|ident| ident.try_symbol())
    }

    /// Symbol represented by this subtree when it is materialized as `block_kind`.
    pub fn try_block_symbol(self, block_kind: BlockKind, name_field: u16) -> Option<&'hir Symbol> {
        let scope_symbol = self.node.try_scope_symbol();

        match block_kind {
            BlockKind::Impl => None,
            BlockKind::Func | BlockKind::Method => scope_symbol,
            BlockKind::Field => scope_symbol
                .or_else(|| self.try_ident_symbol_with_field(name_field))
                .or_else(|| self.try_first_ident_symbol())
                .or_else(|| self.try_first_child_ident_symbol()),
            BlockKind::Parameter => scope_symbol
                .or_else(|| self.try_variable_symbol())
                .or_else(|| self.try_first_ident_symbol())
                .or_else(|| self.try_first_child_ident_symbol()),
            _ => scope_symbol
                .or_else(|| self.try_first_ident_symbol())
                .or_else(|| self.try_first_child_ident_symbol()),
        }
    }

    /// Parameter display name using variable identifier, first identifier, then text fallback.
    pub fn try_parameter_name(self) -> Option<String> {
        self.try_variable_ident()
            .map(|ident| ident.name.to_string())
            .or_else(|| self.try_first_ident_name())
            .or_else(|| self.try_text().map(|text| text.to_string()))
    }

    /// Symbol for a type-expression-like node.
    pub fn try_type_expression(self) -> Option<&'hir Symbol> {
        self.try_first_ident_symbol()
            .or_else(|| {
                self.node
                    .children(self.unit)
                    .into_iter()
                    .find_map(|child| child.query(self.unit).try_symbol())
            })
            .or_else(|| self.node.try_scope_ident_symbol())
            .or_else(|| self.try_symbol())
    }

    /// Deepest/rightmost identifier in this subtree that already has a symbol.
    ///
    /// This is useful for call expressions where `crate::module::func` should
    /// resolve to `func`, not the first path segment.
    pub fn try_resolved_ident(self) -> Option<&'hir HirIdent<'hir>> {
        let mut result: Option<&'hir HirIdent<'hir>> = None;
        self.find_resolved_ident(&mut result);
        result
    }

    fn find_resolved_ident(self, result: &mut Option<&'hir HirIdent<'hir>>) {
        if self.node.is_kind(HirKind::Identifier) {
            if let Some(ident) = self.node.as_ident()
                && ident.try_symbol().is_some()
            {
                *result = Some(ident);
            }
            return;
        }
        for child in self.node.children(self.unit) {
            if child.is_kind(HirKind::Identifier) {
                if let Some(ident) = child.as_ident()
                    && ident.try_symbol().is_some()
                {
                    *result = Some(ident);
                }
            } else if child.is_kind(HirKind::Internal) {
                child.query(self.unit).find_resolved_ident(result);
            }
        }
    }

    /// First text child content, useful for keywords such as `self`.
    pub fn try_text(self) -> Option<&'hir str> {
        for child in self.node.children(self.unit) {
            if child.is_kind(HirKind::Text)
                && let Some(text) = child.as_text()
            {
                return Some(text.text());
            }
        }
        None
    }

    /// Identifier under the first child with the given field id.
    ///
    /// Scoped types return their direct type identifier; generic types recurse
    /// through the type child to return the generic callee (`Repository` in
    /// `Repository<User>`).
    pub fn try_ident_with_field(self, field_id: u16) -> Option<&'hir HirIdent<'hir>> {
        debug_assert!(!self.node.is_kind(HirKind::Identifier));
        for child in self.node.children(self.unit) {
            if child
                .try_base()
                .is_some_and(|base| base.field_id == field_id)
            {
                return Self::try_type_ident(child, self.unit);
            }
        }
        None
    }

    fn try_type_ident(
        node: HirNode<'hir>,
        unit: &CompileUnit<'hir>,
    ) -> Option<&'hir HirIdent<'hir>> {
        if node.is_kind(HirKind::Identifier) {
            return node.as_ident();
        }

        for child in node.children(unit) {
            if child.is_kind(HirKind::Identifier) {
                return child.as_ident();
            }
        }

        for child in node.children(unit) {
            if child.is_kind(HirKind::Internal) {
                return Self::try_type_ident(child, unit);
            }
        }
        None
    }

    /// Scope node paired with an identifier under the given field id.
    pub fn try_scope_and_ident_with_field(
        self,
        field_id: u16,
    ) -> Option<(&'hir HirScope<'hir>, &'hir HirIdent<'hir>)> {
        let scope = self.node.as_scope()?;
        let ident = self.try_ident_with_field(field_id)?;
        Some((scope, ident))
    }

    /// All identifier descendants whose field id matches `field_id`.
    pub fn identifiers_with_field(self, field_id: u16) -> Vec<&'hir HirIdent<'hir>> {
        let mut identifiers = Vec::new();
        self.collect_identifiers_with_field(field_id, &mut identifiers);
        identifiers
    }

    fn collect_identifiers_with_field(
        self,
        field_id: u16,
        identifiers: &mut Vec<&'hir HirIdent<'hir>>,
    ) {
        if self
            .node
            .try_base()
            .is_some_and(|base| base.field_id == field_id)
            && let Some(ident) = self.node.as_ident()
        {
            identifiers.push(ident);
        }

        for child in self.node.children(self.unit) {
            child
                .query(self.unit)
                .collect_identifiers_with_field(field_id, identifiers);
        }
    }

    /// All identifier descendants in source order.
    pub fn identifiers(self) -> Vec<&'hir HirIdent<'hir>> {
        let mut identifiers = Vec::new();
        self.collect_identifiers(&mut identifiers);
        identifiers
    }

    fn collect_identifiers(self, identifiers: &mut Vec<&'hir HirIdent<'hir>>) {
        if let Some(ident) = self.node.as_ident() {
            identifiers.push(ident);
        }

        for child in self.node.children(self.unit) {
            child.query(self.unit).collect_identifiers(identifiers);
        }
    }

    /// Attach a block id to this node's non-primitive block-owning symbol.
    pub fn attach_block_id(self, block_id: BlockId, block_kind: BlockKind, positional_field: u16) {
        if let Some(symbol) = self.try_block_owner_symbol(block_kind, positional_field) {
            symbol.set_block_id(block_id);
        }
    }
}
