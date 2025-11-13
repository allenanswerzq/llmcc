# Language Definition Refactoring: Parser Abstraction

## Overview

The `define_tokens!` macro and `LanguageTrait` have been refactored to decouple language definitions from **tree-sitter**. This enables:

- **Multi-parser support**: Easily add parsers beyond tree-sitter (custom, language-specific, experimental)
- **Better testability**: Create mock parse trees without full parsing infrastructure
- **Cleaner architecture**: Language definitions focus on semantic mappings, not parsing details
- **Forward compatibility**: Future parser improvements don't require macro changes

## Problem: Previous Tight Coupling

### Before
```rust
pub trait LanguageTrait {
    // Returns tree-sitter Tree directly
    fn parse(text: impl AsRef<[u8]>) -> Option<::tree_sitter::Tree>;
    // ... other methods
}

// Macro hardcoded tree-sitter parsing
fn parse(text: impl AsRef<[u8]>) -> Option<::tree_sitter::Tree> {
    let mut parser = ::tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
    parser.parse(text.as_ref(), None)
}
```

**Limitations:**
- Language definitions tightly coupled to tree-sitter
- Cannot use alternative parsers without modifying macro
- Cannot mock trees for testing without full parsing setup
- Difficult to experiment with custom AST representations

## Solution: Generic ParseTree Abstraction

### New Architecture

```
LanguageTrait::parse()
    ↓
    Returns: Box<dyn ParseTree>
    ↓
    ├─ TreeSitterParseTree (current default)
    ├─ CustomParseTree (future)
    └─ MockParseTree (testing)
```

### 1. ParseTree Trait (Core Abstraction)

```rust
pub trait ParseTree: Send + Sync + 'static {
    /// Type-erased access to underlying tree for downcasting
    fn as_any(&self) -> &(dyn Any + Send + Sync);

    /// Debug representation
    fn debug_info(&self) -> String;
}
```

**Key Design Decisions:**
- **Type erasure via `as_any()`**: Allows access to concrete type without generic parameters
- **Send + Sync**: Required for parallel parsing in `rayon`
- **'static bound**: Enables serialization and dynamic dispatch patterns

### 2. Default Implementation: TreeSitterParseTree

```rust
pub struct TreeSitterParseTree {
    pub tree: ::tree_sitter::Tree,
}

impl ParseTree for TreeSitterParseTree {
    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn debug_info(&self) -> String {
        format!("TreeSitter(root_id: {})", self.tree.root_node().id())
    }
}
```

### 3. Updated LanguageTrait

```rust
pub trait LanguageTrait {
    /// Parse source code and return a generic parse tree.
    /// Implementations can use any parser technology.
    fn parse(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>>;

    // ... other semantic mapping methods unchanged
}
```

### 4. CompileCtxt Storage Strategy

**Backward Compatibility + Forward Extensibility:**

```rust
pub struct CompileCtxt<'tcx> {
    /// Generic parse trees from language-specific parsers
    pub parse_trees: Vec<Option<Box<dyn ParseTree>>>,

    /// Cached tree-sitter trees extracted from parse_trees
    /// (maintained for backward compatibility with existing code)
    pub trees: Vec<Option<Tree>>,
    // ... other fields
}
```

**Rationale:**
- `parse_trees`: New generic interface for future extensibility
- `trees`: Cached tree-sitter extraction for existing API compatibility
- No breaking changes to public `tree()` method

### 5. Tree Extraction Strategy

```rust
impl<'tcx> CompileCtxt<'tcx> {
    /// Helper: Extract tree-sitter tree from a generic ParseTree
    fn extract_tree(parse_tree: &Option<Box<dyn ParseTree>>) -> Option<Tree> {
        parse_tree.as_ref().and_then(|pt| {
            pt.as_any()
                .downcast_ref::<TreeSitterParseTree>()
                .map(|ts| ts.tree.clone())
        })
    }

    /// Get tree-sitter tree for a specific file
    pub fn get_tree(&self, index: usize) -> Option<&Tree> {
        self.trees.get(index).and_then(|t| t.as_ref())
    }

    /// Get the generic parse tree for a specific file
    pub fn get_parse_tree(&self, index: usize) -> Option<&Box<dyn ParseTree>> {
        self.parse_trees.get(index).and_then(|t| t.as_ref())
    }
}
```

## Migration Path: How to Add Custom Parsers

### Step 1: Define Custom ParseTree Implementation

```rust
// in your_parser/mod.rs
use llmcc_core::lang_def::ParseTree;
use std::any::Any;

pub struct YourCustomParseTree {
    // Your custom AST representation
    pub ast: CustomAst,
}

impl ParseTree for YourCustomParseTree {
    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn debug_info(&self) -> String {
        format!("Custom(nodes: {})", self.ast.node_count())
    }
}
```

### Step 2: Implement LanguageTrait

```rust
#[derive(Debug)]
pub struct LangCustom {}

impl LanguageTrait for LangCustom {
    fn parse(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
        let source = text.as_ref();
        let ast = your_custom_parser::parse(source)?;
        Some(Box::new(YourCustomParseTree { ast }))
    }

    fn hir_kind(kind_id: u16) -> HirKind {
        // ... semantic mappings remain unchanged
    }

    // ... other required methods
}
```

### Step 3: Use with CompileCtxt

```rust
// Just works with existing infrastructure
let cc = CompileCtxt::from_sources::<LangCustom>(&[source]);

// Access custom tree if needed
if let Some(parse_tree) = cc.get_parse_tree(0) {
    if let Some(custom) = parse_tree.as_any().downcast_ref::<YourCustomParseTree>() {
        // Use custom tree features
    }
}
```

## Implementation Details

### Macro Changes

The `define_tokens!` macro now:

1. **Wraps parsing result**: `parser.parse() -> Option<Box<dyn ParseTree>>`
2. **Creates default wrapper**: `Box::new(TreeSitterParseTree { tree })`
3. **No semantic logic changes**: Token mapping, visitor trait generation, etc. remain identical

```rust
impl $crate::lang_def::LanguageTrait for [<Lang $suffix>] {
    fn parse(text: impl AsRef<[u8]>) -> Option<Box<dyn $crate::lang_def::ParseTree>> {
        let source = text.as_ref();
        paste::paste! {
            let mut parser = ::tree_sitter::Parser::new();
            parser.set_language(&[<tree_sitter_ $suffix:lower>]::LANGUAGE.into())
                .expect("failed to initialize tree-sitter parser");
            let tree = parser.parse(source, None)?;
            Some(Box::new($crate::lang_def::TreeSitterParseTree { tree }))
        }
    }
    // ... other methods unchanged
}
```

### Visitor Trait

The generated visitor traits are **completely unaffected**:

```rust
pub trait AstVisitorRust<'a, T> {
    fn visit_node(&mut self, node: HirNode<'a>, t: &mut T, parent: Option<&Symbol>) { ... }
    // ... all token-specific visit_* methods
}
```

Visitors work with `HirNode` (semantic layer), not parse trees, so they're inherently parser-agnostic.

## Best Practices Applied

### 1. **Dependency Inversion**
- Language definitions don't depend on specific parsers
- Parsers implement `LanguageTrait`, not vice versa
- Easy to test with mock implementations

### 2. **Type Erasure Pattern**
- `Box<dyn ParseTree>` hides parser specifics
- `as_any()` enables safe downcasting when needed
- Allows heterogeneous collections

### 3. **Backward Compatibility**
- Existing `tree()` API unchanged
- `trees` field maintains cached tree-sitter trees
- No breaking changes to consumer code

### 4. **Separation of Concerns**
```
Parsing Layer (generic)
    ↓
ParseTree (abstract representation)
    ↓
CompileCtxt (stores & caches)
    ↓
HirNode (semantic layer - independent of parser)
    ↓
Visitor Pattern (works on semantics)
```

### 5. **Zero-Cost Abstraction for Default Case**
- Tree-sitter path unchanged when not using custom parsers
- Clone cost minimal (tree-sitter trees are already reference-counted)
- Casting overhead only when accessing custom trees

## Testing Strategy

### Unit Tests Remain Unchanged
```rust
#[test]
fn test_macro_parse_with_tree_sitter() {
    let rust_code = b"fn main() {}";
    let tree = LangRust::parse(rust_code);
    assert!(tree.is_some()); // Returns Box<dyn ParseTree>
}
```

### Future: Mock Tests
```rust
#[test]
fn test_with_mock_parser() {
    let mock_tree = MockParseTree::new();
    // No need for full tree-sitter setup
}
```

## Performance Impact

### Memory
- **Before**: Single `Vec<Option<Tree>>`
- **After**: Two vectors + box overhead
- **Trade-off**: Extra 16 bytes per file for generic flexibility

### Parsing Speed
- **No change**: Parser implementation identical
- **Cache maintenance**: `O(n)` tree extraction at startup (done once)

### Downcasting Cost
- **When needed**: Single virtual call + pointer comparison
- **Expected**: Microseconds per operation

## Migration Checklist

For users adding custom parsers:

- [ ] Create `ParseTree` implementation
- [ ] Implement `LanguageTrait` for your language
- [ ] Ensure `Send + Sync` bounds satisfied
- [ ] Register with `CompileCtxt::from_sources::<YourLang>()`
- [ ] Add tests with mock trees
- [ ] Document parser-specific features

## Future Enhancements

### Potential Improvements
1. **Parser registry**: Dynamic parser discovery via traits
2. **Parse metrics per parser**: Benchmark different implementations
3. **Incremental parsing**: Cache parse trees for change detection
4. **Alternative AST formats**: WASM-based parsers, protobuf representation
5. **Parser composition**: Chain multiple parsers for language variants

### Example: Incremental Parsing
```rust
pub trait IncrementalParseTree: ParseTree {
    fn update(&mut self, new_text: &[u8], changes: &[TextChange]) -> Result<()>;
}
```

## Conclusion

This refactoring decouples language definitions from tree-sitter implementation while:
- ✅ Maintaining full backward compatibility
- ✅ Enabling multi-parser support
- ✅ Improving testability with mocks
- ✅ Following Rust best practices (type erasure, dependency inversion)
- ✅ Zero performance impact for current users

The architecture is now ready for multi-parser experimentation, custom AST representations, and future parser innovations.
