//! Visitor pattern implementation for HIR (High-level Intermediate Representation) traversal.
//!
//! This module provides a generic visitor trait for traversing the hierarchical structure of
//! the Abstract Syntax Tree (AST) represented as a HIR graph. The visitor pattern enables
//! extensible AST processing without modifying the HIR node structures.
//!
//! # Overview
//!
//! - `HirVisitor`: The primary trait for implementing custom AST traversal logic
//! - Supports depth-first traversal with parent block tracking
//! - Node-specific visit methods for each HIR kind (File, Scope, Text, Internal, etc.)
//! - Default implementations delegate to `visit_children` for generic traversal
//!
//! # Architecture
//!
//! The visitor pattern follows a two-method dispatch:
//! 1. `visit_node(node, parent)` - Dispatches based on node kind to specific visitor methods
//! 2. Node-specific methods (e.g., `visit_file`, `visit_scope`) - Can be overridden for custom behavior
//!
//! The default traversal strategy is depth-first: each node visits its children in order,
//! maintaining the parent block ID for context tracking throughout the tree.
//!
//! # Lifetime Parameters
//!
//! - `'v`: The lifetime of the compilation unit and HIR nodes being visited
//!
//! # Example Usage
//!
//! ```ignore
//! struct SymbolCollector<'v> {
//!     unit: CompileUnit<'v>,
//!     symbols: Vec<String>,
//! }
//!
//! impl<'v> HirVisitor<'v> for SymbolCollector<'v> {
//!     fn unit(&self) -> CompileUnit<'v> {
//!         self.unit
//!     }
//!
//!     fn visit_ident(&mut self, node: HirNode<'v>, parent: BlockId) {
//!         // Custom identifier visitation logic
//!         if let Some(name) = node.text(self.unit()) {
//!             self.symbols.push(name.to_string());
//!         }
//!         // Still visit children if needed
//!         self.visit_children(node, parent);
//!     }
//! }
//! ```
//!
//! # Implementation Patterns
//!
//! ## Pattern 1: Data Collection
//! Override specific visit methods to collect information from matching nodes.
//! This is useful for gathering symbols, references, or metrics.
//!
//! ## Pattern 2: Tree Transformation
//! Override methods and skip `visit_children` for nodes that should not be traversed further.
//! This enables selective tree pruning or early termination.
//!
//! ## Pattern 3: Context-Aware Traversal
//! Use the `parent` parameter to maintain context about the enclosing scope or block.
//! This enables semantic analysis that depends on positional context.
//!
//! # Performance Considerations
//!
//! - The visitor pattern is stack-based (depth-first), using the call stack for recursion
//! - For very deep ASTs, consider converting to an iterative visitor with an explicit stack
//! - Node retrieval via `self.unit().hir_node(*child_id)` performs hash map lookups
//! - Visiting all nodes in a compilation unit is O(n) where n is the number of nodes
//!
//! # Error Handling
//!
//! The current implementation logs unhandled node kinds using `eprintln!`. For production use:
//! - Consider returning `Result` types from visit methods
//! - Implement proper error recovery strategies
//! - Track which node kinds are encountered for debugging
//!
//! # Thread Safety
//!
//! The `HirVisitor` trait is not `Send` or `Sync` by default. For multi-threaded collection:
//! - Use separate visitor instances per thread
//! - Each thread should have its own compilation unit reference
//! - Use thread-safe data structures for shared results
//!
//! # Future Extensions
//!
//! Potential enhancements to this trait:
//! - Support for visitor state management (pre-order, in-order, post-order)
//! - Visitor composition for combining multiple visitors
//! - Backtracking support for complex analysis patterns
//! - Lazy evaluation strategies for large ASTs

use crate::CompileUnit;
use crate::graph_builder::BlockId;
use crate::ir::{HirKind, HirNode};

/// Generic visitor trait for traversing the HIR (High-level Intermediate Representation) tree.
///
/// This trait defines the interface for implementing custom AST traversal logic. The visitor pattern
/// enables extensible processing of HIR nodes without modifying the node structures themselves.
///
/// # Type Parameters
/// - `'v`: The lifetime of the compilation unit and HIR nodes being visited
///
/// # Required Methods
/// - `unit()`: Returns the CompileUnit context for node lookups
///
/// # Optional Methods
/// All visit methods have default implementations that delegate to `visit_children`.
/// Override specific methods to implement custom traversal behavior.
///
/// # Dispatch Mechanism
/// The `visit_node` method performs single-dispatch based on `HirKind`, routing to appropriate
/// node-specific visitor methods. This enables specialized handling for different node types.
pub trait HirVisitor<'v> {
    /// Returns the compilation unit context for HIR node lookups.
    ///
    /// The compilation unit provides access to the HIR graph, allowing the visitor to
    /// retrieve child nodes and their properties during traversal.
    ///
    /// # Returns
    /// A `CompileUnit` reference with lifetime `'v`, tied to the visitor's lifetime
    ///
    /// # Example
    /// ```ignore
    /// let unit = self.unit();
    /// let child = unit.hir_node(child_id);
    /// ```
    fn unit(&self) -> CompileUnit<'v>;

    /// Visits all children of a given HIR node in order.
    ///
    /// This is the default traversal behavior used by most visitor methods. It recursively
    /// visits each child node using `visit_node`, maintaining the parent block ID for context.
    ///
    /// # Arguments
    /// * `node` - The parent HIR node whose children should be visited
    /// * `parent` - The parent block ID (maintained for context tracking)
    ///
    /// # Traversal Order
    /// Children are visited in the order they appear in the HIR's child list (typically
    /// source order). This enables reliable processing of sequential code structures.
    ///
    /// # Default Behavior
    /// This is automatically called by default implementations of all node-specific visit methods.
    /// Override this for global child visitation customization.
    fn visit_children(&mut self, node: HirNode<'v>, parent: BlockId) {
        let children = node.children();
        for child_id in children {
            let child = self.unit().hir_node(*child_id);
            self.visit_node(child, parent);
        }
    }

    /// Visits a File node and its children.
    ///
    /// File nodes represent the root of an entire source file's AST. They typically contain
    /// module-level declarations, imports, and top-level definitions.
    ///
    /// # Arguments
    /// * `node` - The File node
    /// * `parent` - The parent block ID
    ///
    /// # Default Behavior
    /// Delegates to `visit_children` to recursively visit all top-level declarations
    ///
    /// # Override Example
    /// Override this to process file-level attributes or metadata before/after child traversal
    fn visit_file(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }

    /// Visits a Scope node and its children.
    ///
    /// Scope nodes represent bounded symbol scopes (e.g., function bodies, modules, blocks).
    /// They define namespace boundaries and enable proper symbol resolution.
    ///
    /// # Arguments
    /// * `node` - The Scope node
    /// * `parent` - The parent block ID
    ///
    /// # Default Behavior
    /// Delegates to `visit_children` to recursively visit all symbols and nested scopes
    ///
    /// # Override Example
    /// Override this to perform scope-level analysis like symbol table construction
    fn visit_scope(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }

    /// Visits a Text node and its children.
    ///
    /// Text nodes represent textual content in the source code, typically string literals,
    /// comments, or raw text blocks.
    ///
    /// # Arguments
    /// * `node` - The Text node
    /// * `parent` - The parent block ID
    ///
    /// # Default Behavior
    /// Delegates to `visit_children` (though text nodes typically have no children)
    ///
    /// # Override Example
    /// Override this to collect string literals or analyze documentation comments
    fn visit_text(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }

    /// Visits an Internal node and its children.
    ///
    /// Internal nodes represent intermediate AST nodes created during parsing or
    /// transformation. They're typically not directly mapped to source syntax.
    ///
    /// # Arguments
    /// * `node` - The Internal node
    /// * `parent` - The parent block ID
    ///
    /// # Default Behavior
    /// Delegates to `visit_children` to recursively visit internal structure
    ///
    /// # Override Example
    /// Override this to skip internal nodes or perform special internal node processing
    fn visit_internal(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }

    /// Visits an Undefined node and its children.
    ///
    /// Undefined nodes represent HIR nodes that couldn't be parsed or whose kind is unknown.
    /// They may result from parsing errors or incomplete language support.
    ///
    /// # Arguments
    /// * `node` - The Undefined node
    /// * `parent` - The parent block ID
    ///
    /// # Default Behavior
    /// Delegates to `visit_children` (though undefined nodes should ideally be investigated)
    ///
    /// # Override Example
    /// Override this to log or track malformed nodes for error recovery
    fn visit_undefined(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }

    /// Visits an Identifier node and its children.
    ///
    /// Identifier nodes represent named references to symbols, types, or other entities.
    /// They're critical for symbol resolution and semantic analysis.
    ///
    /// # Arguments
    /// * `node` - The Identifier node
    /// * `parent` - The parent block ID
    ///
    /// # Default Behavior
    /// Delegates to `visit_children` to recursively visit any qualified parts
    ///
    /// # Override Example
    /// Override this to collect symbol references or build dependency graphs
    fn visit_ident(&mut self, node: HirNode<'v>, parent: BlockId) {
        self.visit_children(node, parent);
    }

    /// Dispatches node visitation based on the node's `HirKind`.
    ///
    /// This is the core dispatch mechanism for the visitor pattern. It routes each node to
    /// its corresponding visit method based on kind, enabling polymorphic behavior.
    ///
    /// # Arguments
    /// * `node` - The HIR node to visit
    /// * `parent` - The parent block ID to pass to visit methods
    ///
    /// # Dispatch Table
    /// - `HirKind::File` → `visit_file()`
    /// - `HirKind::Scope` → `visit_scope()`
    /// - `HirKind::Text` → `visit_text()`
    /// - `HirKind::Internal` → `visit_internal()`
    /// - `HirKind::Undefined` → `visit_undefined()`
    /// - `HirKind::Identifier` → `visit_ident()`
    /// - Other kinds → Logs unhandled node kind warning
    ///
    /// # Error Handling
    /// Unhandled node kinds produce a warning via `eprintln!` but do not panic.
    /// This allows processing to continue for partially understood ASTs.
    fn visit_node(&mut self, node: HirNode<'v>, parent: BlockId) {
        match node.kind() {
            HirKind::File => self.visit_file(node, parent),
            HirKind::Scope => self.visit_scope(node, parent),
            HirKind::Text => self.visit_text(node, parent),
            HirKind::Internal => self.visit_internal(node, parent),
            HirKind::Undefined => self.visit_undefined(node, parent),
            HirKind::Identifier => self.visit_ident(node, parent),
            _ => {
                eprintln!("Unhandled node kind: {}", node.format_node(self.unit()));
            }
        }
    }
}
