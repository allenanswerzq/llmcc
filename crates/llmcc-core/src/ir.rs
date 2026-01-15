use parking_lot::RwLock;
use smallvec::SmallVec;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicPtr, Ordering};
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::CompileUnit;
use crate::declare_arena;
use crate::scope::Scope;
use crate::symbol::Symbol;

// Declare the arena with all HIR types
// Using DashMap-based arena for concurrent O(1) lookup
declare_arena!(Arena {
    hir_node: HirNode<'a>,
    hir_file: HirFile,
    hir_text: HirText<'a>,
    hir_internal: HirInternal,
    hir_scope: HirScope<'a>,
    hir_ident: HirIdent<'a>,
    scope: Scope<'a>,
    symbol: Symbol,
});

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, FromRepr, Display, Default,
)]
#[strum(serialize_all = "snake_case")]
pub enum HirKind {
    #[default]
    Undefined,
    Error,
    File,
    Scope,
    Text,
    Internal,
    Comment,
    Identifier,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum HirNode<'hir> {
    #[default]
    Undefined,
    Root(&'hir HirRoot),
    Text(&'hir HirText<'hir>),
    Internal(&'hir HirInternal),
    Scope(&'hir HirScope<'hir>),
    File(&'hir HirFile),
    Ident(&'hir HirIdent<'hir>),
}

impl<'hir> HirNode<'hir> {
    pub fn format(&self, _unit: CompileUnit<'hir>) -> String {
        let id = self.id();
        let kind = self.kind();
        format!("{kind}:{id}")
    }

    /// Get the base information for any HIR node
    pub fn base(&self) -> Option<&HirBase> {
        match self {
            HirNode::Undefined => None,
            HirNode::Root(node) => Some(&node.base),
            HirNode::Text(node) => Some(&node.base),
            HirNode::Internal(node) => Some(&node.base),
            HirNode::Scope(node) => Some(&node.base),
            HirNode::File(node) => Some(&node.base),
            HirNode::Ident(node) => Some(&node.base),
        }
    }

    /// Get the kind of this HIR node
    pub fn kind(&self) -> HirKind {
        self.base().map_or(HirKind::Undefined, |base| base.kind)
    }

    /// Check if this node is of a specific kind
    pub fn is_kind(&self, kind: HirKind) -> bool {
        self.kind() == kind
    }

    /// Get the field ID of this node (used in structured tree navigation)
    ///
    /// For example, in a function declaration, the name field might have field_id=1
    /// and the body field_id=2. Panics on Undefined node.
    pub fn field_id(&self) -> u16 {
        self.base().unwrap().field_id
    }

    /// Get child IDs of this node
    pub fn child_ids(&self) -> &[HirId] {
        self.base().map_or(&[], |base| &base.children)
    }

    /// Get children nodes of this node - uses SmallVec to avoid heap allocation for small child counts
    pub fn children(&self, unit: &CompileUnit<'hir>) -> SmallVec<[HirNode<'hir>; 8]> {
        self.base().map_or(SmallVec::new(), |base| {
            base.children.iter().map(|id| unit.hir_node(*id)).collect()
        })
    }

    /// Get tree-sitter kind ID for this node (distinct from HirKind)
    pub fn kind_id(&self) -> u16 {
        self.base().unwrap().kind_id
    }

    /// Get unique HirId for this node within its compilation unit. Panics on Undefined.
    pub fn id(&self) -> HirId {
        self.base().unwrap().id
    }

    /// Get byte offset where this node starts in source. Panics on Undefined.
    pub fn start_byte(&self) -> usize {
        self.base().unwrap().start_byte
    }

    /// Get byte offset where this node ends (exclusive). Panics on Undefined.
    pub fn end_byte(&self) -> usize {
        self.base().unwrap().end_byte
    }

    /// Get 1-indexed line number where this node starts. Panics on Undefined.
    pub fn start_line(&self) -> usize {
        self.base().unwrap().start_line
    }

    /// Get count of direct children
    pub fn child_count(&self) -> usize {
        self.child_ids().len()
    }

    /// Get parent HirId if it exists
    pub fn parent(&self) -> Option<HirId> {
        self.base().and_then(|base| base.parent)
    }

    /// Find optional child with matching field ID
    pub fn child_by_field(&self, unit: &CompileUnit<'hir>, field_id: u16) -> Option<HirNode<'hir>> {
        self.base().unwrap().child_by_field(unit, field_id)
    }

    pub fn child_by_kind(&self, unit: &CompileUnit<'hir>, kind_id: u16) -> Option<HirNode<'hir>> {
        self.children(unit)
            .into_iter()
            .find(|&child| child.kind_id() == kind_id)
    }

    /// Returns the symbol referenced by the identifier within a specific child field.
    pub fn ident_symbol_by_field(
        &self,
        unit: &CompileUnit<'hir>,
        field_id: u16,
    ) -> Option<&'hir Symbol> {
        let child = self.child_by_field(unit, field_id)?;
        let ident = child.find_ident(unit)?;
        ident.opt_symbol()
    }

    /// Returns the ident symbol if any.
    /// Prefers finding an identifier that has a symbol set (useful for scoped paths
    /// where the target identifier has the resolved symbol).
    pub fn ident_symbol(&self, unit: &CompileUnit<'hir>) -> Option<&'hir Symbol> {
        // First try to find an identifier that already has a symbol set
        if let Some(ident) = self.find_symboled_ident(unit) {
            return ident.opt_symbol();
        }
        // Fall back to finding any identifier
        let ident = self.find_ident(unit)?;
        ident.opt_symbol()
    }

    /// Recursively search down the tree for a child with matching field ID.
    /// Keeps going deeper until it finds a match or reaches a leaf node.
    pub fn child_by_field_recursive(
        &self,
        unit: &CompileUnit<'hir>,
        field_id: u16,
    ) -> Option<HirNode<'hir>> {
        // First check immediate children
        if let Some(direct_child) = self.child_by_field(unit, field_id) {
            return Some(direct_child);
        }

        // If no direct child with this field, recurse into all children
        for child in self.children(unit) {
            if let Some(recursive_match) = child.child_by_field_recursive(unit, field_id) {
                return Some(recursive_match);
            }
        }

        None
    }

    /// Find the identifier for the first child node that is an identifier or interior node.
    /// Recursively searches for identifiers within interior nodes.
    pub fn find_ident(&self, unit: &CompileUnit<'hir>) -> Option<&'hir HirIdent<'hir>> {
        if self.is_kind(HirKind::Identifier) {
            return self.as_ident();
        }
        for child in self.children(unit) {
            if child.is_kind(HirKind::Identifier) {
                return child.as_ident();
            }
            if child.is_kind(HirKind::Internal)
                && let Some(id) = child.find_ident(unit)
            {
                return Some(id);
            }
        }
        None
    }

    /// Find the deepest/rightmost identifier that has a symbol set.
    /// This is useful for call expressions where we want the resolved callee,
    /// not just the first identifier in a scoped path like `crate::module::func`.
    pub fn find_symboled_ident(&self, unit: &CompileUnit<'hir>) -> Option<&'hir HirIdent<'hir>> {
        let mut result: Option<&'hir HirIdent<'hir>> = None;
        self.find_symboled_ident_recursive(unit, &mut result);
        result
    }

    fn find_symboled_ident_recursive(
        &self,
        unit: &CompileUnit<'hir>,
        result: &mut Option<&'hir HirIdent<'hir>>,
    ) {
        if self.is_kind(HirKind::Identifier) {
            if let Some(ident) = self.as_ident()
                && ident.opt_symbol().is_some()
            {
                *result = Some(ident);
            }
            return;
        }
        for child in self.children(unit) {
            if child.is_kind(HirKind::Identifier) {
                if let Some(ident) = child.as_ident()
                    && ident.opt_symbol().is_some()
                {
                    *result = Some(ident);
                }
            } else if child.is_kind(HirKind::Internal) {
                child.find_symboled_ident_recursive(unit, result);
            }
        }
    }

    /// Find the first text node's content in children (for keywords like "self").
    pub fn find_text(&self, unit: &CompileUnit<'hir>) -> Option<&str> {
        for child in self.children(unit) {
            if child.is_kind(HirKind::Text)
                && let Some(text) = child.as_text()
            {
                return Some(text.text());
            }
        }
        None
    }

    /// Find identifier for the first child with a matching field ID.
    /// For scoped types like `crate::module::Type`, returns `Type` (the direct type_identifier child).
    /// For generic types like `Repository<User>`, recurses into the type child to get `Repository`.
    pub fn ident_by_field(
        &self,
        unit: &CompileUnit<'hir>,
        field_id: u16,
    ) -> Option<&'hir HirIdent<'hir>> {
        debug_assert!(!self.is_kind(HirKind::Identifier));
        for child in self.children(unit) {
            if child.field_id() == field_id {
                return Self::find_type_ident(&child, unit);
            }
        }
        None
    }

    /// Find the type identifier from a node, handling scoped and generic types correctly.
    /// Looks for direct identifier children first, then recurses into the first internal child.
    fn find_type_ident(
        node: &HirNode<'hir>,
        unit: &CompileUnit<'hir>,
    ) -> Option<&'hir HirIdent<'hir>> {
        if node.is_kind(HirKind::Identifier) {
            return node.as_ident();
        }
        // First pass: look for direct identifier children
        for child in node.children(unit) {
            if child.is_kind(HirKind::Identifier) {
                return child.as_ident();
            }
        }
        // Second pass: recurse into the FIRST internal child only (e.g., generic_type → type child)
        // This avoids recursing into type_arguments which would give wrong results
        for child in node.children(unit) {
            if child.is_kind(HirKind::Internal) {
                return Self::find_type_ident(&child, unit);
            }
        }
        None
    }

    #[inline]
    pub fn as_root(&self) -> Option<&'hir HirRoot> {
        match self {
            HirNode::Root(r) => Some(r),
            _ => None,
        }
    }

    #[inline]
    pub fn as_text(&self) -> Option<&'hir HirText<'hir>> {
        match self {
            HirNode::Text(r) => Some(r),
            _ => None,
        }
    }

    #[inline]
    pub fn as_scope(&self) -> Option<&'hir HirScope<'hir>> {
        match self {
            HirNode::Scope(r) => Some(r),
            _ => None,
        }
    }

    /// Get scope and child identifier by field - convenience method combining as_scope() and ident_by_field()
    #[inline]
    pub fn scope_and_ident_by_field(
        &self,
        unit: &CompileUnit<'hir>,
        field_id: u16,
    ) -> Option<(&'hir HirScope<'hir>, &'hir HirIdent<'hir>)> {
        let scope = self.as_scope()?;
        let ident = self.ident_by_field(unit, field_id)?;
        Some((scope, ident))
    }

    /// Collect identifiers by field kind matching a specific field ID
    pub fn collect_by_field_kind(
        &self,
        unit: &CompileUnit<'hir>,
        field_id: u16,
    ) -> Vec<&'hir HirIdent<'hir>> {
        let mut idents = Vec::new();
        self.collect_by_field_kind_impl(unit, field_id, &mut idents);
        idents
    }

    /// Helper for recursively collecting identifiers by field kind
    fn collect_by_field_kind_impl(
        &self,
        unit: &CompileUnit<'hir>,
        field_id: u16,
        idents: &mut Vec<&'hir HirIdent<'hir>>,
    ) {
        // If this node has matching field ID and is an identifier, collect it
        if self.field_id() == field_id
            && let Some(ident) = self.as_ident()
        {
            idents.push(ident);
        }

        // Recursively collect from all children
        for child in self.children(unit) {
            child.collect_by_field_kind_impl(unit, field_id, idents);
        }
    }

    #[inline]
    pub fn as_file(&self) -> Option<&'hir HirFile> {
        match self {
            HirNode::File(r) => Some(r),
            _ => None,
        }
    }

    #[inline]
    pub fn as_internal(&self) -> Option<&'hir HirInternal> {
        match self {
            HirNode::Internal(r) => Some(r),
            _ => None,
        }
    }

    #[inline]
    pub fn as_ident(&self) -> Option<&'hir HirIdent<'hir>> {
        match self {
            HirNode::Ident(r) => Some(r),
            _ => None,
        }
    }

    /// Recursively collect all identifier nodes under this node
    pub fn collect_idents(&self, unit: &CompileUnit<'hir>) -> Vec<&'hir HirIdent<'hir>> {
        let mut idents = Vec::new();
        self.collect_idents_impl(unit, &mut idents);
        idents
    }

    /// Helper function for recursively collecting identifier nodes
    fn collect_idents_impl(
        &self,
        unit: &CompileUnit<'hir>,
        idents: &mut Vec<&'hir HirIdent<'hir>>,
    ) {
        // If this node is an identifier, collect it
        if let Some(ident) = self.as_ident() {
            idents.push(ident);
        }

        // Recursively collect from all children
        for child in self.children(unit) {
            child.collect_idents_impl(unit, idents);
        }
    }

    /// Check if node is trivia (whitespace, comment, etc.)
    pub fn is_trivia(&self) -> bool {
        matches!(self.kind(), HirKind::Text | HirKind::Comment)
    }

    /// Set the block ID on the symbol associated with this node.
    /// Works for both HirScope (gets symbol from scope) and HirIdent (has direct symbol).
    /// Does nothing if no symbol is associated or if the symbol is a primitive (shared globally).
    pub fn set_block_id(&self, block_id: crate::block::BlockId) {
        use crate::symbol::SymKind;
        // Try HirScope first
        if let Some(scope) = self.as_scope() {
            // First try scope's symbol
            if let Some(symbol) = scope.opt_symbol() {
                // Don't set block_id on primitives - they are shared globally
                if symbol.kind() != SymKind::Primitive {
                    symbol.set_block_id(block_id);
                }
                return;
            }
            // If no scope symbol, try the scope's ident (for type aliases, etc.)
            if let Some(ident) = scope.opt_ident()
                && let Some(symbol) = ident.opt_symbol()
            {
                if symbol.kind() != SymKind::Primitive {
                    symbol.set_block_id(block_id);
                }
                return;
            }
        }
        // Try HirIdent
        if let Some(ident) = self.as_ident()
            && let Some(symbol) = ident.opt_symbol()
        {
            // Don't set block_id on primitives - they are shared globally
            if symbol.kind() != SymKind::Primitive {
                symbol.set_block_id(block_id);
            }
        }
    }

    /// Get the symbol associated with this node if any.
    /// Works for both HirScope and HirIdent nodes.
    pub fn opt_symbol(&self) -> Option<&'hir Symbol> {
        if let Some(scope) = self.as_scope() {
            return scope.opt_symbol();
        }
        if let Some(ident) = self.as_ident() {
            return ident.opt_symbol();
        }
        None
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, Default)]
/// Unique identifier for a HIR node within a compilation unit. IDs are stable,
/// sequential, and used for parent-child relationships and symbol references.
pub struct HirId(pub usize);

/// Global counter for allocating unique HIR IDs
static HIR_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

impl HirId {
    /// Allocate a new unique HIR ID
    pub fn new() -> Self {
        let id = HIR_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        HirId(id)
    }

    /// Get the next HIR ID that will be allocated (useful for diagnostics)
    pub fn next() -> Self {
        HirId(HIR_ID_COUNTER.load(Ordering::Relaxed))
    }
}

impl std::fmt::Display for HirId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Common metadata shared by all HIR node types. Provides identity, parent link,
/// tree-sitter connection, and child references for tree structure.
#[derive(Debug, Clone, Default)]
pub struct HirBase {
    pub id: HirId,
    pub parent: Option<HirId>,
    pub kind_id: u16,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub kind: HirKind,
    pub field_id: u16,
    pub children: SmallVec<[HirId; 4]>,
}

impl HirBase {
    /// Find child with matching field ID (linear search, O(n))
    pub fn child_by_field<'hir>(
        &self,
        unit: &CompileUnit<'hir>,
        field_id: u16,
    ) -> Option<HirNode<'hir>> {
        self.children
            .iter()
            .map(|id| unit.hir_node(*id))
            .find(|child| child.field_id() == field_id)
    }
}

#[derive(Debug, Clone)]
/// Root node as topmost parent for all nodes in compilation unit's HIR.
pub struct HirRoot {
    pub base: HirBase,
    pub file_name: Option<String>,
}

impl HirRoot {
    /// Create new root node with optional file name
    pub fn new(base: HirBase, file_name: Option<String>) -> Self {
        Self { base, file_name }
    }
}

#[derive(Debug, Clone)]
/// Leaf node containing textual content (strings, comments, etc.)
pub struct HirText<'hir> {
    pub base: HirBase,
    pub text: &'hir str,
}

impl<'hir> HirText<'hir> {
    /// Create new text node with given content
    pub fn new(base: HirBase, text: &'hir str) -> Self {
        Self { base, text }
    }

    pub fn text(&self) -> &str {
        self.text
    }
}

#[derive(Debug, Clone)]
/// Synthetic node created during parsing/transformation, not directly from source.
pub struct HirInternal {
    pub base: HirBase,
}

impl HirInternal {
    /// Create new internal node
    pub fn new(base: HirBase) -> Self {
        Self { base }
    }
}

#[derive(Debug)]
/// Node representing a named scope (functions, classes, modules, blocks, etc.).
/// Scopes are critical for symbol resolution - collected symbols are associated with scope lifetime.
pub struct HirScope<'hir> {
    pub base: HirBase,
    pub ident: RwLock<Option<&'hir HirIdent<'hir>>>,
    pub scope: RwLock<Option<&'hir Scope<'hir>>>,
}

impl<'hir> HirScope<'hir> {
    /// Create new scope node with optional identifier
    pub fn new(base: HirBase, ident: Option<&'hir HirIdent<'hir>>) -> Self {
        Self {
            base,
            ident: RwLock::new(ident),
            scope: RwLock::new(None),
        }
    }

    /// Get human-readable name (identifier name or "unamed_scope")
    pub fn owner_name(&self) -> String {
        if let Some(id) = *self.ident.read() {
            id.name.to_string()
        } else {
            "unamed_scope".to_string()
        }
    }

    /// Set the scope reference for this scope node
    pub fn set_scope(&self, scope: &'hir Scope<'hir>) {
        *self.scope.write() = Some(scope);
    }

    /// Get the scope reference if it has been set
    pub fn scope(&self) -> &'hir Scope<'hir> {
        self.scope
            .read()
            .unwrap_or_else(|| panic!("scope must be set for HirScope {}", self.base.id))
    }

    pub fn opt_scope(&self) -> Option<&'hir Scope<'hir>> {
        *self.scope.read()
    }

    pub fn set_ident(&self, ident: &'hir HirIdent<'hir>) {
        *self.ident.write() = Some(ident);
    }

    pub fn opt_ident(&self) -> Option<&'hir HirIdent<'hir>> {
        *self.ident.read()
    }

    pub fn ident(&self) -> &'hir HirIdent<'hir> {
        self.ident.read().expect("ident must be set")
    }

    pub fn opt_symbol(&self) -> Option<&'hir Symbol> {
        self.opt_scope().and_then(|scope| scope.opt_symbol())
    }
}

impl<'hir> Clone for HirScope<'hir> {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            ident: RwLock::new(*self.ident.read()),
            scope: RwLock::new(*self.scope.read()),
        }
    }
}

#[derive(Debug)]
/// Node representing a named identifier/reference (variables, functions, types, etc.).
/// Identifiers are primary targets for symbol collection and resolution.
pub struct HirIdent<'hir> {
    pub base: HirBase,
    pub name: &'hir str,
    pub symbol: AtomicPtr<Symbol>,
    _phantom: std::marker::PhantomData<&'hir ()>,
}

impl<'hir> HirIdent<'hir> {
    /// Create new identifier node with name
    pub fn new(base: HirBase, name: &'hir str) -> Self {
        Self {
            base,
            name,
            symbol: AtomicPtr::new(std::ptr::null_mut()),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn id(&self) -> HirId {
        self.base.id
    }

    pub fn set_symbol(&self, symbol: &'hir Symbol) {
        self.symbol
            .store(symbol as *const _ as *mut _, Ordering::Release);
    }

    #[inline]
    pub fn opt_symbol(&self) -> Option<&'hir Symbol> {
        let ptr = self.symbol.load(Ordering::Acquire);
        if ptr.is_null() {
            None
        } else {
            unsafe { Some(&*ptr) }
        }
    }
}

#[derive(Debug, Clone)]
/// Node representing a source file. Provides entry point for language-specific analysis.
pub struct HirFile {
    pub base: HirBase,
    pub file_path: String,
}

impl HirFile {
    /// Create new file node with path
    pub fn new(base: HirBase, file_path: String) -> Self {
        Self { base, file_path }
    }
}
