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
//! All HIR nodes are allocated in an arena for efficient memory management
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

use std::cell::Cell;
use std::fmt;

use parking_lot::RwLock;
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

use crate::context::CompileUnit;
use crate::declare_arena;
use crate::scope::Scope;
use crate::symbol::Symbol;

// Declare the arena with all HIR types
declare_arena!([
    hir_root: HirRoot,
    hir_text: HirText,
    hir_internal: HirInternal,
    hir_scope: HirScope<'tcx>,
    hir_file: HirFile,
    hir_ident: HirIdent<'tcx>,
    symbol: Symbol,
] @vec [
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
        let id = self.id();
        let kind = self.kind();
        let mut f = format!("{}:{}", kind, id);
        if let Some(scope) = unit.opt_get_scope(id) {
            f.push_str(&format!("   s:{}", scope.format_compact()));
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
    /// if base.child_by_field(unit, id).is_some() {
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
    /// - `child_by_kind()`: Find children by their kind_id
    pub fn kind_id(&self) -> u16 {
        self.base().unwrap().kind_id
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
    /// let node_id = node.id();
    /// let parent_node = unit.hir_node(node_id);
    /// ```
    pub fn id(&self) -> HirId {
        self.base().unwrap().id
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
        self.base().unwrap().start_byte
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
        self.base().unwrap().end_byte
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

/// Common metadata shared by all HIR node types.
///
/// HirBase provides the foundational information for every node, including its identity,
/// parent relationship, connection to the tree-sitter parse tree, and child references.
///
/// # Fields
/// - `id`: Unique identifier for this node
/// - `parent`: Optional reference to the parent node
/// - `node`: Reference to the underlying tree-sitter Node for source locations
/// - `kind`: The type category of this node
/// - `field_id`: Field identifier used in structured queries
/// - `kind_id`: kind ID for this node (field_id: kind_id) uniquely identifies the node type
/// - `children`: Vector of child node IDs for tree structure
///
/// # Child Lookup
/// HirBase provides methods to find children by field ID or kind, supporting efficient
/// navigation of the AST structure without requiring parent references.
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
    /// if let Some(name_node) = base.child_by_field(unit, FIELD_NAME) {
    ///     println!("Function name: {:?}", name_node.as_ident());
    /// }
    /// ```
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
pub struct HirRoot {
    pub base: HirBase,
    pub file_name: Option<String>,
}

impl HirRoot {
    /// Create a new root node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    /// * `file_name` - Optional source file name
    pub fn new(base: HirBase, file_name: Option<String>) -> Self {
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
pub struct HirText {
    pub base: HirBase,
    pub text: String,
}

impl HirText {
    /// Create a new text node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    /// * `text` - The text content to store
    pub fn new(base: HirBase, text: String) -> Self {
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
pub struct HirInternal {
    pub base: HirBase,
}

impl HirInternal {
    /// Create a new internal node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    pub fn new(base: HirBase) -> Self {
        Self { base }
    }
}

#[derive(Debug)]
/// Node representing a named scope in the code.
///
/// Scope nodes represent namespace boundaries such as function bodies, class definitions,
/// module scopes, or code blocks. Each scope can optionally have an associated identifier
/// (e.g., function or class name).
///
/// # Fields
/// - `base`: Common node metadata
/// - `ident`: Optional identifier associated with this scope (e.g., function name)
/// - `scope`: Optional reference to the Scope collection for this scope
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
    pub base: HirBase,
    pub ident: RwLock<Option<&'hir HirIdent<'hir>>>,
    pub scope: RwLock<Option<&'hir Scope<'hir>>>,
}

impl<'hir> HirScope<'hir> {
    /// Create a new scope node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    /// * `ident` - Optional identifier for the scope
    pub fn new(base: HirBase, ident: Option<&'hir HirIdent<'hir>>) -> Self {
        Self {
            base,
            ident: RwLock::new(ident),
            scope: RwLock::new(None),
        }
    }

    /// Get a human-readable name for this scope.
    ///
    /// Returns the identifier name if present, otherwise returns "unamed_scope".
    /// Useful for debugging and error messages.
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
    pub base: HirBase,
    pub name: String,
    pub symbol: RwLock<Option<&'hir Symbol>>,
}

impl<'hir> HirIdent<'hir> {
    /// Create a new identifier node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    /// * `name` - The identifier string
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
        *self.symbol.read().expect("symbol must be set")
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
pub struct HirFile {
    pub base: HirBase,
    pub file_path: String,
}

impl<'hir> HirFile {
    /// Create a new file node.
    ///
    /// # Arguments
    /// * `base` - Common metadata for this node
    /// * `file_path` - The path to the source file
    pub fn new(base: HirBase, file_path: String) -> Self {
        Self { base, file_path }
    }
}
