use parking_lot::RwLock;
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::CompileUnit;
use crate::declare_arena;
use crate::scope::Scope;
use crate::symbol::Symbol;

// Declare the arena with all HIR types
declare_arena!([
    symbol: Symbol,
] @vec [
    hir_root: HirRoot,
    hir_text: HirText,
    hir_internal: HirInternal,
    hir_scope: HirScope<'tcx>,
    hir_file: HirFile,
    hir_ident: HirIdent<'tcx>,
    scope: Scope<'tcx>,
]);

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
    Text(&'hir HirText),
    Internal(&'hir HirInternal),
    Scope(&'hir HirScope<'hir>),
    File(&'hir HirFile),
    Ident(&'hir HirIdent<'hir>),
}

impl<'hir> HirNode<'hir> {
    /// Format node as "kind:id [s:scope_id]" for debugging
    pub fn format_node(&self, _unit: CompileUnit<'hir>) -> String {
        let id = self.id();
        let kind = self.kind();
        let mut f = format!("{}:{}", kind, id);
        // Only Scope nodes have an associated Scope; get it if available
        if let HirNode::Scope(scope_node) = self {
            if let Some(scope) = *scope_node.scope.read() {
                f.push_str(&format!("   s:{}", scope.format_compact()));
            }
        }
        f
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

    /// Get children of this node
    pub fn children(&self) -> &[HirId] {
        self.base().map_or(&[], |base| &base.children)
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

    /// Get count of direct children
    pub fn child_count(&self) -> usize {
        self.children().len()
    }

    /// Get parent HirId if it exists
    pub fn parent(&self) -> Option<HirId> {
        self.base().and_then(|base| base.parent)
    }

    /// Find optional child with matching field ID
    pub fn child_by_field(&self, unit: CompileUnit<'hir>, field_id: u16) -> Option<HirNode<'hir>> {
        self.base().unwrap().child_by_field(unit, field_id)
    }

    /// Find an optional child with a specific tree-sitter kind ID.
    pub fn child_by_kind(&self, unit: CompileUnit<'hir>, kind_id: u16) -> Option<HirNode<'hir>> {
        self.children()
            .iter()
            .map(|id| unit.hir_node(*id))
            .find(|child| child.kind_id() == kind_id)
    }

    /// Find the identifier for the first child node that is an identifier or interior node.
    /// Recursively searches for identifiers within interior nodes.
    pub fn find_identifier(&self, unit: CompileUnit<'hir>) -> Option<HirId> {
        if self.is_kind(HirKind::Identifier) {
            return Some(self.id());
        }
        for child_id in self.children() {
            let child = unit.hir_node(*child_id);
            if child.is_kind(HirKind::Identifier) {
                return Some(child.id());
            }
            if child.is_kind(HirKind::Internal) {
                if let Some(id) = child.find_identifier(unit) {
                    return Some(id);
                }
            }
        }
        None
    }

    /// Find identifier for the first child with a matching field ID.
    pub fn find_identifier_for_field(
        &self,
        unit: CompileUnit<'hir>,
        field_id: u16,
    ) -> Option<HirId> {
        debug_assert!(!self.is_kind(HirKind::Identifier));
        for child_id in self.children() {
            let child = unit.hir_node(*child_id);
            if child.field_id() == field_id {
                return child.find_identifier(unit);
            }
        }
        None
    }

    /// Find identifier for the first child with a matching kind ID.
    pub fn find_identifier_for_kind(&self, unit: CompileUnit<'hir>, kind_id: u16) -> Option<HirId> {
        debug_assert!(!self.is_kind(HirKind::Identifier));
        for child_id in self.children() {
            let child = unit.hir_node(*child_id);
            if child.kind_id() == kind_id {
                return child.find_identifier(unit);
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
    pub fn as_text(&self) -> Option<&'hir HirText> {
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
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, Default)]
/// Unique identifier for a HIR node within a compilation unit. IDs are stable,
/// sequential, and used for parent-child relationships and symbol references.
pub struct HirId(pub u32);

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
    pub kind: HirKind,
    pub field_id: u16,
    pub children: Vec<HirId>,
}

impl HirBase {
    /// Find child with matching field ID (linear search, O(n))
    pub fn child_by_field<'hir>(
        &self,
        unit: CompileUnit<'hir>,
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
pub struct HirText {
    pub base: HirBase,
    pub text: String,
}

impl HirText {
    /// Create new text node with given content
    pub fn new(base: HirBase, text: String) -> Self {
        Self { base, text }
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
            id.name.clone()
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
        self.scope.read().expect("scope must be set")
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
    pub name: String,
    pub symbol: RwLock<Option<&'hir Symbol>>,
}

impl<'hir> HirIdent<'hir> {
    /// Create new identifier node with name
    pub fn new(base: HirBase, name: String) -> Self {
        Self {
            base,
            name,
            symbol: RwLock::new(None),
        }
    }

    pub fn set_symbol(&self, symbol: &'hir Symbol) {
        *self.symbol.write() = Some(symbol);
    }

    pub fn symbol(&self) -> &'hir Symbol {
        self.symbol.read().expect("symbol must be set")
    }
}

#[derive(Debug, Clone)]
/// Node representing a source file. Provides entry point for language-specific analysis.
pub struct HirFile {
    pub base: HirBase,
    pub file_path: String,
}

impl<'hir> HirFile {
    /// Create new file node with path
    pub fn new(base: HirBase, file_path: String) -> Self {
        Self { base, file_path }
    }
}
