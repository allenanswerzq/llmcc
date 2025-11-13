//! High-level Intermediate Representation (HIR) for the LLMCC compiler.
//!
//! This module defines the core HIR structures used throughout compilation. The HIR provides
//! a language-independent representation of source code that bridges between the tree-sitter
//! parse tree and the symbol resolution and analysis phases.
//!
//! # Architecture
//!
//! ## Node Types
//! The HIR represents different kinds of AST nodes:
//! - **File**: Root node representing an entire source file
//! - **Scope**: Named scopes (functions, classes, modules, blocks)
//! - **Identifier**: Named references and declarations
//! - **Text**: String literals, comments, and textual content
//! - **Internal**: Intermediate nodes created during transformation
//! - **Root**: Abstract root for the tree structure
//!
//! ## Memory Management
//! All HIR nodes are allocated in an arena (bump allocator) for efficient memory management
//! and to enable safe lifetime management across compilation phases. The arena is declared
//! via the `declare_arena!` macro and manages the lifetime 'hir.
//!
//! ## Node Access Patterns
//! Nodes provide three patterns for runtime type checking:
//! - `as_*`: Returns `Option<&T>` for safe downcasting
//! - `is_*`: Returns `bool` for type checking without extraction
//! - `expect_*`: Panics if type mismatch (use only when type is guaranteed)
//!
//! # Performance Characteristics
//! - Node access: O(1) via direct pointer from arena allocation
//! - Child lookup by field: O(children) linear search through field IDs
//! - Recursive identifier finding: O(n) traversal of subtree
//! - All operations are cache-friendly due to arena allocation
//!
//! # Thread Safety
//! HIR nodes are immutable once allocated and can be safely shared across threads.
//! The arena itself is not thread-safe; use thread-local arenas for parallel processing.

use strum_macros::{Display, EnumIter, EnumString, FromRepr};
use tree_sitter::Node;

use crate::context::CompileUnit;
use crate::declare_arena;
use crate::scope::Scope;
use crate::symbol::Symbol;

// Declare the arena with all HIR types
declare_arena!([
    hir_root: HirRoot<'tcx>,
    hir_text: HirText<'tcx>,
    hir_internal: HirInternal<'tcx>,
    hir_scope: HirScope<'tcx>,
    hir_file: HirFile<'tcx>,
    hir_ident: HirIdent<'tcx>,
    symbol: Symbol,
    scope: Scope<'tcx>,
]);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, FromRepr, Display, Default,
)]
#[strum(serialize_all = "snake_case")]
/// Enumeration of all possible HIR node kinds.
///
/// Each variant represents a category of AST node. The HIR uses a discriminated union pattern
/// to represent different node types while maintaining type safety.
///
/// # Variants
/// - `Undefined`: Unknown or uninitialized node (default)
/// - `Error`: Represents a parse error or malformed construct
/// - `File`: Root node for a source file
/// - `Scope`: Named scope container (functions, classes, modules, etc.)
/// - `Text`: Textual content (strings, comments, etc.)
/// - `Internal`: Intermediate node created during transformation
/// - `Comment`: Special node kind for comments (documentation, etc.)
/// - `Identifier`: Named reference or declaration
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
/// Discriminated union representing any HIR node.
///
/// This is the primary interface for accessing HIR nodes. It provides safe downcast methods
/// and common accessor methods that work on any node type without requiring runtime type checks.
///
/// # Lifetime
/// - `'hir`: The lifetime of the arena allocator containing this node
///
/// # Variants
/// - `Undefined`: Default variant for uninitialized or invalid nodes
/// - `Root`: Tree root node
/// - `Text`: Textual content node
/// - `Internal`: Intermediate transformation node
/// - `Scope`: Named scope container
/// - `File`: Source file root
/// - `Ident`: Named identifier
pub enum HirNode<'hir> {
    #[default]
    Undefined,
    Root(&'hir HirRoot<'hir>),
    Text(&'hir HirText<'hir>),
    Internal(&'hir HirInternal<'hir>),
    Scope(&'hir HirScope<'hir>),
    File(&'hir HirFile<'hir>),
    Ident(&'hir HirIdent<'hir>),
}

impl<'hir> HirNode<'hir> {
    /// Format this node for display and debugging.
    ///
    /// Produces a human-readable string representation including:
    /// - Node kind (e.g., "file", "scope", "ident")
    /// - Node ID
    /// - Associated scope information if available
    ///
    /// # Example Output
    /// - `file:42` - File node with ID 42
    /// - `ident:17   s:scope_123` - Identifier in scope 123
    ///
    /// # Arguments
    /// * `unit` - The compilation unit for scope lookup
    ///
    /// # Returns
    /// A formatted string useful for debugging and diagnostics
    pub fn format_node(&self, unit: CompileUnit<'hir>) -> String {
        let id = self.hir_id();
        let kind = self.kind();
        let mut f = format!("{}:{}", kind, id);

        if let Some(scope) = unit.opt_get_scope(id) {
            f.push_str(&format!("   s:{}", scope.format_compact()));
        }

        f
    }

    /// Get the base information for any HIR node
    pub fn base(&self) -> Option<&HirBase<'hir>> {
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

    /// Get the field ID of this node.
    ///
    /// The field ID is used in structured tree navigation to identify which
    /// named field this node occupies in its parent. For example, in a function
    /// declaration, the name field might have field_id=1 and the body field_id=2.
    ///
    /// # Returns
    /// The field ID as a u16
    ///
    /// # Panics
    /// Panics if called on an Undefined node
    ///
    /// # Example
    /// ```ignore
    /// let id = node.field_id();
    /// if base.opt_child_by_field(unit, id).is_some() {
    ///     println!("Found sibling with same field ID");
    /// }
    /// ```
    pub fn field_id(&self) -> u16 {
        self.base().unwrap().field_id
    }

    /// Get children of this node
    pub fn children(&self) -> &[HirId] {
        self.base().map_or(&[], |base| &base.children)
    }

    /// Get the tree-sitter kind ID for this node.
    ///
    /// The kind_id is the numeric identifier assigned by tree-sitter to categorize
    /// node types. This is distinct from HirKind and is used for tree-sitter-specific
    /// operations and queries.
    ///
    /// # Returns
    /// The tree-sitter kind ID as a u16
    ///
    /// # Panics
    /// Panics if called on an Undefined node
    ///
    /// # See Also
    /// - `kind()`: Returns the HIR-specific HirKind categorization
    /// - `opt_child_by_kind()`: Find children by their kind_id
    pub fn kind_id(&self) -> u16 {
        self.base().unwrap().node.kind_id()
    }

    /// Get the unique identifier for this node.
    ///
    /// Every HIR node has a unique HirId within its compilation unit. This ID is used
    /// for cross-referencing, parent-child relationships, and symbol associations.
    ///
    /// # Returns
    /// The HirId of this node
    ///
    /// # Panics
    /// Panics if called on an Undefined node
    ///
    /// # Example
    /// ```ignore
    /// let node_id = node.hir_id();
    /// let parent_node = unit.hir_node(node_id);
    /// ```
    pub fn hir_id(&self) -> HirId {
        self.base().unwrap().hir_id
    }

    /// Get the byte offset where this node starts in the source file.
    ///
    /// This is provided by the underlying tree-sitter parse tree and is useful
    /// for error reporting, source mapping, and precise location tracking.
    ///
    /// # Returns
    /// The byte offset (0-indexed) from the start of the file
    ///
    /// # Panics
    /// Panics if called on an Undefined node
    ///
    /// # Example
    /// ```ignore
    /// let start = node.start_byte();
    /// let end = node.end_byte();
    /// let source_slice = &source[start..end];
    /// ```
    pub fn start_byte(&self) -> usize {
        self.base().unwrap().node.start_byte()
    }

    /// Get the byte offset where this node ends in the source file.
    ///
    /// The end byte is exclusive (one past the last character), consistent with Rust's
    /// range semantics.
    ///
    /// # Returns
    /// The exclusive byte offset from the start of the file
    ///
    /// # Panics
    /// Panics if called on an Undefined node
    ///
    /// # See Also
    /// - `start_byte()`: Get the inclusive starting byte offset
    pub fn end_byte(&self) -> usize {
        self.base().unwrap().node.end_byte()
    }

    /// Get the number of direct children this node has.
    ///
    /// Returns the count of immediate children, not including grandchildren.
    ///
    /// # Returns
    /// The number of direct children
    ///
    /// # Example
    /// ```ignore
    /// for i in 0..node.child_count() {
    ///     let child = unit.hir_node(node.children()[i]);
    ///     process(child);
    /// }
    /// ```
    pub fn child_count(&self) -> usize {
        self.children().len()
    }

    /// Get the underlying tree-sitter Node.
    ///
    /// This provides direct access to the tree-sitter parse tree node, which can be
    /// used for advanced queries or accessing tree-sitter-specific features not exposed
    /// through the HIR abstraction.
    ///
    /// # Returns
    /// The tree-sitter Node<'hir> object
    ///
    /// # Panics
    /// Panics if called on an Undefined node
    ///
    /// # Example
    /// ```ignore
    /// let ts_node = node.inner_ts_node();
    /// let text = ts_node.utf8_byte_range();
    /// ```
    pub fn inner_ts_node(&self) -> Node<'hir> {
        self.base().unwrap().node
    }

    /// Get the parent node ID if this node has a parent.
    ///
    /// Returns the HirId of the parent node, or None if this is a root node.
    ///
    /// # Returns
    /// `Some(parent_id)` if a parent exists, `None` otherwise
    ///
    /// # Example
    /// ```ignore
    /// if let Some(parent_id) = node.parent() {
    ///     let parent = unit.hir_node(parent_id);
    ///     println!("Parent: {}", parent.kind());
    /// }
    /// ```
    pub fn parent(&self) -> Option<HirId> {
        self.base().and_then(|base| base.parent)
    }

    /// Find an optional child with a specific field ID (delegated to base).
    ///
    /// Searches direct children for one with the matching field_id.
    ///
    /// # Arguments
    /// * `unit` - The compilation unit for node lookup
    /// * `field_id` - The field ID to search for
    ///
    /// # Returns
    /// `Some(child)` if a matching child exists, `None` otherwise
    ///
    /// # See Also
    /// - `child_by_field()`: Panicking variant
    pub fn opt_child_by_field(
        &self,
        unit: CompileUnit<'hir>,
        field_id: u16,
    ) -> Option<HirNode<'hir>> {
        self.base().unwrap().opt_child_by_field(unit, field_id)
    }

    /// Find a required child with a specific field ID.
    ///
    /// Like `opt_child_by_field()` but panics if the child is not found.
    /// Use only when the field is required by the node structure.
    ///
    /// # Arguments
    /// * `unit` - The compilation unit for node lookup
    /// * `field_id` - The field ID to search for
    ///
    /// # Returns
    /// The child node with the matching field_id
    ///
    /// # Panics
    /// Panics if no child with the given field_id exists
    ///
    /// # Example
    /// ```ignore
    /// let name_field = node.child_by_field(unit, FIELD_NAME);
    /// ```
    pub fn child_by_field(&self, unit: CompileUnit<'hir>, field_id: u16) -> HirNode<'hir> {
        self.opt_child_by_field(unit, field_id)
            .unwrap_or_else(|| panic!("no child with field_id {}", field_id))
    }

    /// Find an identifier child by field ID, panicking if not found.
    ///
    /// Convenience method combining field lookup with identifier downcast.
    /// Panics if either the field doesn't exist or the child is not an identifier.
    ///
    /// # Arguments
    /// * `unit` - The compilation unit for node lookup
    /// * `field_id` - The field ID of the identifier
    ///
    /// # Returns
    /// Reference to the HirIdent if found
    ///
    /// # Panics
    /// Panics if no child exists with the given field_id or if it's not an identifier
    ///
    /// # Example
    /// ```ignore
    /// let func_name = node.expect_ident_child_by_field(unit, FIELD_NAME);
    /// println!("Function: {}", func_name.name);
    /// ```
    pub fn expect_ident_child_by_field(
        &self,
        unit: CompileUnit<'hir>,
        field_id: u16,
    ) -> &'hir HirIdent<'hir> {
        self.opt_child_by_field(unit, field_id)
            .map(|child| child.expect_ident())
            .unwrap_or_else(|| panic!("no child with field_id {}", field_id))
    }

    /// Find an optional child with a specific tree-sitter kind ID.
    ///
    /// Searches direct children for one with the matching kind_id (tree-sitter specific).
    /// Note: This searches by tree-sitter kind_id, not HIR HirKind.
    ///
    /// # Arguments
    /// * `unit` - The compilation unit for node lookup
    /// * `kind_id` - The tree-sitter kind ID to search for
    ///
    /// # Returns
    /// `Some(child)` if a matching child exists, `None` otherwise
    ///
    /// # Complexity
    /// O(children) - linear search through all children
    ///
    /// # See Also
    /// - `child_by_kind()`: Panicking variant
    /// - `kind_id()`: Get a node's kind_id for comparison
    pub fn opt_child_by_kind(
        &self,
        unit: CompileUnit<'hir>,
        kind_id: u16,
    ) -> Option<HirNode<'hir>> {
        self.children()
            .iter()
            .map(|id| unit.hir_node(*id))
            .find(|child| child.kind_id() == kind_id)
    }

    /// Find a required child with a specific tree-sitter kind ID.
    ///
    /// Like `opt_child_by_kind()` but panics if the child is not found.
    /// Use only when a child of the given kind is guaranteed to exist.
    ///
    /// # Arguments
    /// * `unit` - The compilation unit for node lookup
    /// * `kind_id` - The tree-sitter kind ID to search for
    ///
    /// # Returns
    /// The child node with the matching kind_id
    ///
    /// # Panics
    /// Panics if no child with the given kind_id exists
    ///
    /// # Example
    /// ```ignore
    /// let body = node.child_by_kind(unit, KIND_BLOCK);
    /// ```
    pub fn child_by_kind(&self, unit: CompileUnit<'hir>, kind_id: u16) -> HirNode<'hir> {
        self.opt_child_by_kind(unit, kind_id)
            .unwrap_or_else(|| panic!("no child with kind_id {}", kind_id))
    }

    /// Recursively search for an identifier within this node.
    ///
    /// Useful for finding the actual identifier in complex AST nodes like generic_type
    /// that wrap the identifier. For example, in `impl<'tcx> Holder<'tcx>`, the type
    /// field points to a generic_type node, which contains the type_identifier "Holder".
    pub fn find_ident(&self, unit: CompileUnit<'hir>) -> Option<&'hir HirIdent<'hir>> {
        // Check if this node is already an identifier
        if let Some(ident) = self.as_ident() {
            return Some(ident);
        }

        // Otherwise, search through children of any node that has them
        let children = match self {
            HirNode::Root(r) => &r.base.children,
            HirNode::Text(_) => return None,
            HirNode::Internal(i) => &i.base.children,
            HirNode::Scope(s) => &s.base.children,
            HirNode::File(f) => &f.base.children,
            HirNode::Ident(_) => return None,
            HirNode::Undefined => return None,
        };

        // Recursively search all children
        for child_id in children {
            let child = unit.hir_node(*child_id);
            if let Some(ident) = child.find_ident(unit) {
                return Some(ident);
            }
        }

        None
    }

    /// Safely downcast to Root variant.
    ///
    /// Returns `Some` if this node is a Root, `None` otherwise.
    /// This is the safe alternative to `expect_root()`.
    #[inline]
    pub fn as_root(&self) -> Option<&'hir HirRoot<'hir>> {
        match self {
            HirNode::Root(r) => Some(r),
            _ => None,
        }
    }

    /// Check if this node is a Root variant without extracting.
    ///
    /// Useful for quick type checking before more expensive operations.
    #[inline]
    pub fn is_root(&self) -> bool {
        matches!(self, HirNode::Root(_))
    }

    /// Guarantee that this node is a Root, panicking if not.
    ///
    /// Use only when the node type is guaranteed by invariants or prior checks.
    /// Prefer `as_root()` for safer code.
    #[inline]
    pub fn expect_root(&self) -> &'hir HirRoot<'hir> {
        match self {
            HirNode::Root(r) => r,
            _ => panic!("Expected Root variant"),
        }
    }

    /// Safely downcast to Text variant.
    ///
    /// Returns `Some` if this node is Text, `None` otherwise.
    #[inline]
    pub fn as_text(&self) -> Option<&'hir HirText<'hir>> {
        match self {
            HirNode::Text(r) => Some(r),
            _ => None,
        }
    }

    /// Check if this node is a Text variant.
    #[inline]
    pub fn is_text(&self) -> bool {
        matches!(self, HirNode::Text(_))
    }

    /// Guarantee that this node is Text, panicking if not.
    #[inline]
    pub fn expect_text(&self) -> &'hir HirText<'hir> {
        match self {
            HirNode::Text(r) => r,
            _ => panic!("Expected Text variant"),
        }
    }

    /// Safely downcast to Internal variant.
    ///
    /// Returns `Some` if this node is Internal, `None` otherwise.
    #[inline]
    pub fn as_internal(&self) -> Option<&'hir HirInternal<'hir>> {
        match self {
            HirNode::Internal(r) => Some(r),
            _ => None,
        }
    }

    /// Check if this node is an Internal variant.
    #[inline]
    pub fn is_internal(&self) -> bool {
        matches!(self, HirNode::Internal(_))
    }

    /// Guarantee that this node is Internal, panicking if not.
    #[inline]
    pub fn expect_internal(&self) -> &'hir HirInternal<'hir> {
        match self {
            HirNode::Internal(r) => r,
            _ => panic!("Expected Internal variant"),
        }
    }

    /// Safely downcast to Scope variant.
    ///
    /// Returns `Some` if this node is a Scope, `None` otherwise.
    #[inline]
    pub fn as_scope(&self) -> Option<&'hir HirScope<'hir>> {
        match self {
            HirNode::Scope(r) => Some(r),
            _ => None,
        }
    }

    /// Check if this node is a Scope variant.
    #[inline]
    pub fn is_scope(&self) -> bool {
        matches!(self, HirNode::Scope(_))
    }

    /// Guarantee that this node is a Scope, panicking if not.
    #[inline]
    pub fn expect_scope(&self) -> &'hir HirScope<'hir> {
        match self {
            HirNode::Scope(r) => r,
            _ => panic!("Expected Scope variant"),
        }
    }

    /// Safely downcast to File variant.
    ///
    /// Returns `Some` if this node is a File, `None` otherwise.
    #[inline]
    pub fn as_file(&self) -> Option<&'hir HirFile<'hir>> {
        match self {
            HirNode::File(r) => Some(r),
            _ => None,
        }
    }

    /// Check if this node is a File variant.
    #[inline]
    pub fn is_file(&self) -> bool {
        matches!(self, HirNode::File(_))
    }

    /// Guarantee that this node is a File, panicking if not.
    #[inline]
    pub fn expect_file(&self) -> &'hir HirFile<'hir> {
        match self {
            HirNode::File(r) => r,
            _ => panic!("Expected File variant"),
        }
    }

    /// Safely downcast to Ident variant.
    ///
    /// Returns `Some` if this node is an Identifier, `None` otherwise.
    #[inline]
    pub fn as_ident(&self) -> Option<&'hir HirIdent<'hir>> {
        match self {
            HirNode::Ident(r) => Some(r),
            _ => None,
        }
    }

    /// Check if this node is an Ident variant.
    #[inline]
    pub fn is_ident(&self) -> bool {
        matches!(self, HirNode::Ident(_))
    }

    /// Guarantee that this node is an Ident, panicking if not.
    #[inline]
    pub fn expect_ident(&self) -> &'hir HirIdent<'hir> {
        match self {
            HirNode::Ident(r) => r,
            _ => panic!("Expected Ident variant"),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, Default)]
/// Unique identifier for a HIR node within a compilation unit.
///
/// Each node in the HIR is assigned a unique HirId that serves as a stable reference.
/// HirIds are used for:
/// - Parent-child relationships
/// - Symbol references
/// - Scope associations
/// - Cross-referencing during analysis
///
/// # Semantics
/// - IDs are unique within a single compilation unit
/// - ID(0) is typically reserved for the global/root scope
/// - IDs are sequential based on traversal order
/// - IDs are stable across multiple passes of the same input
///
/// # Representation
/// HirIds use 32-bit unsigned integers internally, supporting up to 4 billion unique nodes.
/// This is sufficient for typical source files and even large multi-file projects.
pub struct HirId(pub u32);

impl std::fmt::Display for HirId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
/// Common metadata shared by all HIR node types.
///
/// HirBase provides the foundational information for every node, including its identity,
/// parent relationship, connection to the tree-sitter parse tree, and child references.
///
/// # Fields
/// - `hir_id`: Unique identifier for this node
/// - `parent`: Optional reference to the parent node
/// - `node`: Reference to the underlying tree-sitter Node for source locations
/// - `kind`: The type category of this node
/// - `field_id`: Field identifier used in structured queries
/// - `children`: Vector of child node IDs for tree structure
///
/// # Child Lookup
/// HirBase provides methods to find children by field ID or kind, supporting efficient
/// navigation of the AST structure without requiring parent references.
pub struct HirBase<'hir> {
    pub hir_id: HirId,
    pub parent: Option<HirId>,
    pub node: Node<'hir>,
    pub kind: HirKind,
    pub field_id: u16,
    pub children: Vec<HirId>,
}

impl<'hir> HirBase<'hir> {
    /// Find a child node with any of the given field IDs.
    ///
    /// Searches through all children and returns the first one whose field_id matches
    /// any of the provided field IDs. Useful for finding optional fields or alternatives.
    ///
    /// # Arguments
    /// * `unit` - The compilation unit for node lookup
    /// * `fields_id` - Slice of field IDs to search for
    ///
    /// # Returns
    /// `Some(node)` if a matching child is found, `None` otherwise
    ///
    /// # Complexity
    /// O(children × field_ids) - linear search through children and field_ids for each
    pub fn opt_child_by_fields(
        &self,
        unit: CompileUnit<'hir>,
        fields_id: &[u16],
    ) -> Option<HirNode<'hir>> {
        self.children
            .iter()
            .map(|id| unit.hir_node(*id))
            .find(|child| fields_id.contains(&child.field_id()))
    }

    /// Find a child node with a specific field ID.
    ///
    /// Searches through all children and returns the first one with the matching field_id.
    /// This is commonly used to access named fields in structured nodes.
    ///
    /// # Arguments
    /// * `unit` - The compilation unit for node lookup
    /// * `field_id` - The field ID to search for
    ///
    /// # Returns
    /// `Some(node)` if a child with that field_id exists, `None` otherwise
    ///
    /// # Complexity
    /// O(children) - linear search through child field IDs
    ///
    /// # Example
    /// ```ignore
    /// // Find a function's name field
    /// if let Some(name_node) = base.opt_child_by_field(unit, FIELD_NAME) {
    ///     println!("Function name: {:?}", name_node.as_ident());
    /// }
    /// ```
    pub fn opt_child_by_field(
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
/// Root node representing the abstract root of the compilation tree.
///
/// The root node serves as the topmost parent for all other nodes in a compilation unit's HIR.
/// It typically wraps the entire compilation and may optionally reference the source file name.
///
/// # Fields
/// - `base`: Common node metadata
/// - `file_name`: Optional name of the source file (useful for diagnostics)
///
/// # Semantics
/// - Usually has HirId(0) or is the first node created for a unit
/// - Parent of all top-level definitions
/// - Used as the entry point for tree traversal
pub struct HirRoot<'hir> {
    pub base: HirBase<'hir>,
    pub file_name: Option<String>,
}

impl<'hir> HirRoot<'hir> {
    /// Create a new root node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    /// * `file_name` - Optional source file name
    pub fn new(base: HirBase<'hir>, file_name: Option<String>) -> Self {
        Self { base, file_name }
    }
}

#[derive(Debug, Clone)]
/// Node representing textual content in the source.
///
/// Text nodes contain literal string values, comments, documentation, or other textual elements
/// that don't have further structure. These are leaf nodes in the tree.
///
/// # Fields
/// - `base`: Common node metadata
/// - `text`: The actual text content
///
/// # Examples
/// - String literals: `"hello"`, `"world"`
/// - Documentation comments: `/// This is a doc comment`
/// - Inline comments: `// This is a comment`
pub struct HirText<'hir> {
    pub base: HirBase<'hir>,
    pub text: String,
}

impl<'hir> HirText<'hir> {
    /// Create a new text node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    /// * `text` - The text content to store
    pub fn new(base: HirBase<'hir>, text: String) -> Self {
        Self { base, text }
    }
}

#[derive(Debug, Clone)]
/// Intermediate node created during parsing or transformation.
///
/// Internal nodes represent AST constructs created during compilation that don't directly
/// correspond to source syntax. They're useful for representing intermediate transformations
/// or parser-specific constructs that aren't part of the user's source code.
///
/// # Fields
/// - `base`: Common node metadata
///
/// # Use Cases
/// - Synthetic nodes inserted by transformations
/// - Wrapper nodes around actual constructs
/// - Parser-generated intermediate structures
pub struct HirInternal<'hir> {
    pub base: HirBase<'hir>,
}

impl<'hir> HirInternal<'hir> {
    /// Create a new internal node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    pub fn new(base: HirBase<'hir>) -> Self {
        Self { base }
    }
}

#[derive(Debug, Clone)]
/// Node representing a named scope in the code.
///
/// Scope nodes represent namespace boundaries such as function bodies, class definitions,
/// module scopes, or code blocks. Each scope can optionally have an associated identifier
/// (e.g., function or class name).
///
/// # Fields
/// - `base`: Common node metadata
/// - `ident`: Optional identifier associated with this scope (e.g., function name)
///
/// # Examples
/// - Function scopes: `fn foo() { ... }`
/// - Class scopes: `class MyClass { ... }`
/// - Module scopes: `module Utils { ... }`
/// - Block scopes: `{ statement1; statement2; }`
///
/// # Symbol Collection
/// Scopes are critical for symbol resolution - symbols collected within a scope
/// are associated with that scope's lifetime and namespace.
pub struct HirScope<'hir> {
    pub base: HirBase<'hir>,
    pub ident: Option<&'hir HirIdent<'hir>>,
}

impl<'hir> HirScope<'hir> {
    /// Create a new scope node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    /// * `ident` - Optional identifier for the scope
    pub fn new(base: HirBase<'hir>, ident: Option<&'hir HirIdent<'hir>>) -> Self {
        Self { base, ident }
    }

    /// Get a human-readable name for this scope.
    ///
    /// Returns the identifier name if present, otherwise returns "unamed_scope".
    /// Useful for debugging and error messages.
    pub fn owner_name(&self) -> String {
        if let Some(id) = self.ident {
            id.name.clone()
        } else {
            "unamed_scope".to_string()
        }
    }
}

#[derive(Debug, Clone)]
/// Node representing a named identifier or reference.
///
/// Identifier nodes represent named entities such as variable names, function names,
/// type names, or other linguistic identifiers. These are the primary targets for
/// symbol collection and resolution.
///
/// # Fields
/// - `base`: Common node metadata
/// - `name`: The identifier string
///
/// # Examples
/// - Variable declaration: `let my_var = 42;` → identifier "my_var"
/// - Function name: `fn process() { ... }` → identifier "process"
/// - Type reference: `Vec<String>` → identifier "Vec", "String"
///
/// # Role in Symbol Collection
/// Identifiers are typically where symbols are collected. When the collector encounters
/// an identifier in a declaration context, it creates a symbol entry in the current scope.
pub struct HirIdent<'hir> {
    pub base: HirBase<'hir>,
    pub name: String,
}

impl<'hir> HirIdent<'hir> {
    /// Create a new identifier node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    /// * `name` - The identifier string
    pub fn new(base: HirBase<'hir>, name: String) -> Self {
        Self { base, name }
    }
}

#[derive(Debug, Clone)]
/// Node representing a source file.
///
/// File nodes represent complete source files and serve as entry points for language-specific
/// analysis. They provide the file path for diagnostics and reference tracking.
///
/// # Fields
/// - `base`: Common node metadata
/// - `file_path`: The path to the source file
///
/// # Semantics
/// - Usually a top-level node or a child of the root
/// - Contains all file-level definitions and content
/// - File path is critical for error reporting and source mapping
///
/// # Example
/// A file node for "src/main.rs" would contain all module-level declarations
/// and track the file path for accurate error messages.
pub struct HirFile<'hir> {
    pub base: HirBase<'hir>,
    pub file_path: String,
}

impl<'hir> HirFile<'hir> {
    /// Create a new file node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    /// * `file_path` - The path to the source file
    pub fn new(base: HirBase<'hir>, file_path: String) -> Self {
        Self { base, file_path }
    }
}
