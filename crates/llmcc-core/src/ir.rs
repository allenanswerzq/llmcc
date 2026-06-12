//! High-level IR node definitions.
//!
//! The HIR is the language-neutral tree that later phases consume. It keeps
//! tree-sitter identity (`kind_id`, `field_id`, byte offsets) next to llmcc's
//! own coarse node kind (`HirKind`) and symbol/scope links attached during
//! collection and binding.

use parking_lot::RwLock;
use smallvec::SmallVec;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicPtr, Ordering};
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::CompileUnit;
use crate::declare_arena;
pub use crate::id::HirId;
use crate::ir_query::HirQuery;
use crate::scope::Scope;
use crate::symbol::Symbol;

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
    /// Missing or intentionally absent node.
    #[default]
    Undefined,
    /// Parse or lowering error placeholder.
    Error,
    /// Source file root.
    File,
    /// Node that introduces a lexical or semantic scope.
    Scope,
    /// Source text leaf such as punctuation, keywords, or literals.
    Text,
    /// Structural node used to preserve parse shape without introducing a symbol.
    Internal,
    /// Comment text leaf.
    Comment,
    /// Identifier leaf that may be linked to a symbol.
    Identifier,
}

impl HirKind {
    /// True when the builder should not descend into parse children for this HIR kind.
    pub fn is_leaf(self) -> bool {
        matches!(self, HirKind::Text | HirKind::Comment | HirKind::Identifier)
    }

    /// True for trivia nodes that usually do not participate in semantic analysis.
    pub fn is_trivia(self) -> bool {
        matches!(self, HirKind::Text | HirKind::Comment)
    }
}

/// Lightweight reference to an arena-allocated HIR node.
///
/// Variants carry references to concrete node records. `Undefined` is the only
/// variant without `HirBase` metadata; methods returning `Option` treat it as
/// absent, while required metadata accessors panic with a clear message.
#[derive(Debug, Clone, Copy, Default)]
pub enum HirNode<'hir> {
    #[default]
    Undefined,
    /// Topmost node for a compilation unit.
    Root(&'hir HirRoot),
    /// Text or comment leaf.
    Text(&'hir HirText<'hir>),
    /// Structural internal node.
    Internal(&'hir HirInternal),
    /// Scope-introducing node.
    Scope(&'hir HirScope<'hir>),
    /// Source file node.
    File(&'hir HirFile),
    /// Identifier node.
    Ident(&'hir HirIdent<'hir>),
}

impl<'hir> HirNode<'hir> {
    #[inline]
    fn expect_base(&self, method: &'static str) -> &HirBase {
        self.base()
            .unwrap_or_else(|| panic!("HirNode::{method} called on Undefined"))
    }

    pub fn label(&self) -> String {
        self.base()
            .map(|base| format!("{}:{}", base.kind, base.id))
            .unwrap_or_else(|| "undefined".to_string())
    }

    /// Build a query helper for traversal and symbol lookup from this node.
    pub fn query<'unit>(self, unit: &'unit CompileUnit<'hir>) -> HirQuery<'hir, 'unit> {
        HirQuery::new(self, unit)
    }

    /// Shared metadata for this node, if it is not `Undefined`.
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

    /// Coarse HIR kind used by llmcc visitors and builders.
    pub fn kind(&self) -> HirKind {
        self.base().map_or(HirKind::Undefined, |base| base.kind)
    }

    /// Returns true when this node has the given coarse HIR kind.
    pub fn is_kind(&self, kind: HirKind) -> bool {
        self.kind() == kind
    }

    /// HIR id if this node is not `Undefined`.
    pub fn try_id(&self) -> Option<HirId> {
        self.base().map(|base| base.id)
    }

    /// Tree-sitter field id if this node is not `Undefined`.
    pub fn try_field_id(&self) -> Option<u16> {
        self.base().map(|base| base.field_id)
    }

    /// Tree-sitter kind id if this node is not `Undefined`.
    pub fn try_kind_id(&self) -> Option<u16> {
        self.base().map(|base| base.kind_id)
    }

    /// Tree-sitter field id assigned by the parent cursor.
    pub fn field_id(&self) -> u16 {
        self.expect_base("field_id").field_id
    }

    /// Child node ids in source order.
    pub fn child_ids(&self) -> &[HirId] {
        self.base().map_or(&[], |base| &base.children)
    }

    /// Child nodes in source order.
    pub fn children(&self, unit: &CompileUnit<'hir>) -> SmallVec<[HirNode<'hir>; 8]> {
        self.base().map_or(SmallVec::new(), |base| {
            base.children.iter().map(|id| unit.hir_node(*id)).collect()
        })
    }

    /// Tree-sitter kind id for this node.
    pub fn kind_id(&self) -> u16 {
        self.expect_base("kind_id").kind_id
    }

    /// Unique HIR id within the compile context.
    pub fn id(&self) -> HirId {
        self.expect_base("id").id
    }

    /// Start byte offset in the source file.
    pub fn start_byte(&self) -> usize {
        self.expect_base("start_byte").start_byte
    }

    /// End byte offset in the source file, exclusive.
    pub fn end_byte(&self) -> usize {
        self.expect_base("end_byte").end_byte
    }

    /// One-indexed start line.
    pub fn start_line(&self) -> usize {
        self.expect_base("start_line").start_line
    }

    /// Number of direct children.
    pub fn child_count(&self) -> usize {
        self.child_ids().len()
    }

    /// Parent node id, if this is not the root.
    pub fn parent(&self) -> Option<HirId> {
        self.base().and_then(|base| base.parent)
    }

    /// Direct child with the given tree-sitter field id.
    pub fn child_by_field(&self, unit: &CompileUnit<'hir>, field_id: u16) -> Option<HirNode<'hir>> {
        self.base()?.child_by_field(unit, field_id)
    }

    /// Direct child with the given tree-sitter kind id.
    pub fn child_by_kind(&self, unit: &CompileUnit<'hir>, kind_id: u16) -> Option<HirNode<'hir>> {
        self.children(unit)
            .into_iter()
            .find(|child| child.base().is_some_and(|base| base.kind_id == kind_id))
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

    /// True for trivia nodes that usually do not participate in semantic analysis.
    pub fn is_trivia(&self) -> bool {
        self.kind().is_trivia()
    }
}

/// Metadata shared by every concrete HIR node.
///
/// `kind` is llmcc's coarse language-neutral category. `kind_id` and
/// `field_id` preserve tree-sitter-specific structure for language handlers.
#[derive(Debug, Clone, Default)]
pub struct HirBase {
    /// Identity: unique id assigned during HIR building.
    pub id: HirId,
    /// Tree: parent HIR node id, absent only for the root.
    pub parent: Option<HirId>,
    /// Tree-sitter: raw node kind id from the parser grammar.
    pub kind_id: u16,
    /// Source: start byte offset in the file.
    pub start_byte: usize,
    /// Source: end byte offset in the file, exclusive.
    pub end_byte: usize,
    /// Source: one-indexed starting line.
    pub start_line: usize,
    /// Classification: language-neutral HIR kind.
    pub kind: HirKind,
    /// Tree-sitter: field id assigned by the parent cursor.
    pub field_id: u16,
    /// Tree: direct child ids in source order.
    pub children: SmallVec<[HirId; 4]>,
}

impl HirBase {
    /// Direct child with the given field id.
    pub fn child_by_field<'hir>(
        &self,
        unit: &CompileUnit<'hir>,
        field_id: u16,
    ) -> Option<HirNode<'hir>> {
        self.children
            .iter()
            .map(|id| unit.hir_node(*id))
            .find(|child| child.base().is_some_and(|base| base.field_id == field_id))
    }
}

/// Root node as topmost parent for all nodes in compilation unit's HIR.
#[derive(Debug, Clone)]
pub struct HirRoot {
    /// Shared node metadata.
    pub base: HirBase,
    /// Display file name or stored source path, if available.
    pub file_name: Option<String>,
}

impl HirRoot {
    /// Create a root node with an optional display file name.
    pub fn new(base: HirBase, file_name: Option<String>) -> Self {
        Self { base, file_name }
    }
}

/// Leaf node containing textual content (strings, comments, etc.)
#[derive(Debug, Clone)]
pub struct HirText<'hir> {
    /// Shared node metadata.
    pub base: HirBase,
    /// Arena-backed source slice or decoded text.
    pub text: &'hir str,
}

impl<'hir> HirText<'hir> {
    /// Create a text or comment node with arena-backed content.
    pub fn new(base: HirBase, text: &'hir str) -> Self {
        Self { base, text }
    }

    /// Text content for this node.
    pub fn text(&self) -> &str {
        self.text
    }
}

/// Synthetic node created during parsing/transformation, not directly from source.
#[derive(Debug, Clone)]
pub struct HirInternal {
    /// Shared node metadata.
    pub base: HirBase,
}

impl HirInternal {
    /// Create an internal structural node.
    pub fn new(base: HirBase) -> Self {
        Self { base }
    }
}

/// Node representing a named scope (functions, classes, modules, blocks, etc.).
///
/// The HIR scope node is created during IR building; the semantic `Scope` is
/// attached later during symbol collection.
#[derive(Debug)]
pub struct HirScope<'hir> {
    /// Shared node metadata.
    pub base: HirBase,
    /// Identifier that names this scope, if the syntax has one.
    pub ident: RwLock<Option<&'hir HirIdent<'hir>>>,
    /// Semantic scope attached during symbol collection.
    pub scope: RwLock<Option<&'hir Scope<'hir>>>,
}

impl<'hir> HirScope<'hir> {
    /// Create a scope node with an optional name identifier.
    pub fn new(base: HirBase, ident: Option<&'hir HirIdent<'hir>>) -> Self {
        Self {
            base,
            ident: RwLock::new(ident),
            scope: RwLock::new(None),
        }
    }

    /// Human-readable owner name for diagnostics.
    pub fn owner_name(&self) -> String {
        if let Some(id) = *self.ident.read() {
            id.name.to_string()
        } else {
            "unnamed_scope".to_string()
        }
    }

    /// Attach the semantic scope created during symbol collection.
    pub fn set_scope(&self, scope: &'hir Scope<'hir>) {
        *self.scope.write() = Some(scope);
    }

    /// Semantic scope, panicking if symbol collection has not attached one yet.
    pub fn scope(&self) -> &'hir Scope<'hir> {
        self.scope
            .read()
            .unwrap_or_else(|| panic!("scope must be set for HirScope {}", self.base.id))
    }

    /// Semantic scope if it has been attached.
    pub fn opt_scope(&self) -> Option<&'hir Scope<'hir>> {
        *self.scope.read()
    }

    /// Attach or replace the identifier that names this scope.
    pub fn set_ident(&self, ident: &'hir HirIdent<'hir>) {
        *self.ident.write() = Some(ident);
    }

    /// Identifier that names this scope, if present.
    pub fn opt_ident(&self) -> Option<&'hir HirIdent<'hir>> {
        *self.ident.read()
    }

    /// Identifier that names this scope, panicking if absent.
    pub fn ident(&self) -> &'hir HirIdent<'hir> {
        self.ident
            .read()
            .unwrap_or_else(|| panic!("ident must be set for HirScope {}", self.base.id))
    }

    /// Symbol associated with this scope through its semantic `Scope`.
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

/// Node representing a named identifier/reference (variables, functions, types, etc.).
///
/// Identifiers are the main bridge from HIR syntax to resolved symbols. Symbol
/// binding stores a raw pointer here so graph construction and inference can
/// cheaply find the resolved target.
#[derive(Debug)]
pub struct HirIdent<'hir> {
    /// Shared node metadata.
    pub base: HirBase,
    /// Arena-backed identifier text.
    pub name: &'hir str,
    /// Resolved symbol pointer set by collection or binding.
    pub symbol: AtomicPtr<Symbol>,
    _phantom: PhantomData<&'hir ()>,
}

impl<'hir> HirIdent<'hir> {
    /// Create an identifier node with arena-backed name text.
    pub fn new(base: HirBase, name: &'hir str) -> Self {
        Self {
            base,
            name,
            symbol: AtomicPtr::new(std::ptr::null_mut()),
            _phantom: PhantomData,
        }
    }

    /// HIR id for this identifier.
    pub fn id(&self) -> HirId {
        self.base.id
    }

    /// Attach the resolved symbol for this identifier.
    pub fn set_symbol(&self, symbol: &'hir Symbol) {
        self.symbol
            .store(symbol as *const _ as *mut _, Ordering::Release);
    }

    /// Resolved symbol if one has been attached.
    #[inline]
    pub fn opt_symbol(&self) -> Option<&'hir Symbol> {
        let ptr = self.symbol.load(Ordering::Acquire);
        if ptr.is_null() {
            None
        } else {
            // SAFETY: `set_symbol` only stores pointers to arena-allocated
            // symbols. The arena outlives every HIR identifier that references
            // it, and symbols are shared immutably after allocation.
            unsafe { Some(&*ptr) }
        }
    }
}

/// Node representing a source file. Provides entry point for language-specific analysis.
#[derive(Debug, Clone)]
pub struct HirFile {
    /// Shared node metadata.
    pub base: HirBase,
    /// Stored source path used for display and metadata.
    pub file_path: String,
}

impl HirFile {
    /// Create a source file node.
    pub fn new(base: HirBase, file_path: String) -> Self {
        Self { base, file_path }
    }
}
