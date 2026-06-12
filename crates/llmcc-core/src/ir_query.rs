//! Query helpers for HIR subtrees.
//!
//! `HirNode` owns the compact data-model API. `HirQuery` owns traversal and
//! semantic lookup policy that needs a `CompileUnit` to resolve child ids.

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

    /// Symbol attached directly to this node.
    pub fn symbol(self) -> Option<&'hir Symbol> {
        if let Some(scope) = self.node.as_scope() {
            return scope.try_symbol();
        }
        if let Some(ident) = self.node.as_ident() {
            return ident.try_symbol();
        }
        None
    }

    /// Symbol referenced by the identifier under a specific child field.
    pub fn resolved_symbol_by_field(self, field_id: u16) -> Option<&'hir Symbol> {
        let child = self.node.child_by_field(self.unit, field_id)?;
        child.query(self.unit).first_ident()?.try_symbol()
    }

    /// Best symbol associated with this subtree.
    ///
    /// Prefer the deepest/rightmost identifier that already has a symbol. This
    /// handles scoped paths where the resolved target is not the first token.
    pub fn resolved_symbol(self) -> Option<&'hir Symbol> {
        if let Some(ident) = self.resolved_ident() {
            return ident.try_symbol();
        }

        self.first_ident()?.try_symbol()
    }

    /// First descendant with the given tree-sitter field id.
    pub fn descendant_with_field(self, field_id: u16) -> Option<HirNode<'hir>> {
        if let Some(direct_child) = self.node.child_by_field(self.unit, field_id) {
            return Some(direct_child);
        }

        for child in self.node.children(self.unit) {
            if let Some(recursive_match) = child.query(self.unit).descendant_with_field(field_id) {
                return Some(recursive_match);
            }
        }

        None
    }

    /// First identifier in this subtree.
    ///
    /// This is intentionally shallow-first and is useful for declarations where
    /// the first identifier is usually the declared name.
    pub fn first_ident(self) -> Option<&'hir HirIdent<'hir>> {
        if self.node.is_kind(HirKind::Identifier) {
            return self.node.as_ident();
        }
        for child in self.node.children(self.unit) {
            if child.is_kind(HirKind::Identifier) {
                return child.as_ident();
            }
            if child.is_kind(HirKind::Internal)
                && let Some(id) = child.query(self.unit).first_ident()
            {
                return Some(id);
            }
        }
        None
    }

    /// Deepest/rightmost identifier in this subtree that already has a symbol.
    ///
    /// This is useful for call expressions where `crate::module::func` should
    /// resolve to `func`, not the first path segment.
    pub fn resolved_ident(self) -> Option<&'hir HirIdent<'hir>> {
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
    pub fn text(self) -> Option<&'hir str> {
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
    pub fn ident_with_field(self, field_id: u16) -> Option<&'hir HirIdent<'hir>> {
        debug_assert!(!self.node.is_kind(HirKind::Identifier));
        for child in self.node.children(self.unit) {
            if child
                .try_base()
                .is_some_and(|base| base.field_id == field_id)
            {
                return Self::type_ident(child, self.unit);
            }
        }
        None
    }

    fn type_ident(node: HirNode<'hir>, unit: &CompileUnit<'hir>) -> Option<&'hir HirIdent<'hir>> {
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
                return Self::type_ident(child, unit);
            }
        }
        None
    }

    /// Scope node paired with an identifier under the given field id.
    pub fn scope_and_ident_with_field(
        self,
        field_id: u16,
    ) -> Option<(&'hir HirScope<'hir>, &'hir HirIdent<'hir>)> {
        let scope = self.node.as_scope()?;
        let ident = self.ident_with_field(field_id)?;
        Some((scope, ident))
    }

    /// All identifier descendants whose field id matches `field_id`.
    pub fn idents_with_field(self, field_id: u16) -> Vec<&'hir HirIdent<'hir>> {
        let mut idents = Vec::new();
        self.collect_idents_with_field(field_id, &mut idents);
        idents
    }

    fn collect_idents_with_field(self, field_id: u16, idents: &mut Vec<&'hir HirIdent<'hir>>) {
        if self
            .node
            .try_base()
            .is_some_and(|base| base.field_id == field_id)
            && let Some(ident) = self.node.as_ident()
        {
            idents.push(ident);
        }

        for child in self.node.children(self.unit) {
            child
                .query(self.unit)
                .collect_idents_with_field(field_id, idents);
        }
    }

    /// All identifier descendants in source order.
    pub fn identifiers(self) -> Vec<&'hir HirIdent<'hir>> {
        let mut idents = Vec::new();
        self.collect_identifiers(&mut idents);
        idents
    }

    fn collect_identifiers(self, idents: &mut Vec<&'hir HirIdent<'hir>>) {
        if let Some(ident) = self.node.as_ident() {
            idents.push(ident);
        }

        for child in self.node.children(self.unit) {
            child.query(self.unit).collect_identifiers(idents);
        }
    }

    /// Attach a block id to any non-primitive symbol associated with this node.
    pub fn attach_block_id(self, block_id: BlockId) {
        if let Some(scope) = self.node.as_scope() {
            if let Some(symbol) = scope.try_symbol() {
                if symbol.kind() != SymKind::Primitive {
                    symbol.set_block_id(block_id);
                }
                return;
            }

            if let Some(ident) = scope.try_ident()
                && let Some(symbol) = ident.try_symbol()
            {
                if symbol.kind() != SymKind::Primitive {
                    symbol.set_block_id(block_id);
                }
                return;
            }
        }

        if let Some(ident) = self.node.as_ident()
            && let Some(symbol) = ident.try_symbol()
            && symbol.kind() != SymKind::Primitive
        {
            symbol.set_block_id(block_id);
        }
    }
}
