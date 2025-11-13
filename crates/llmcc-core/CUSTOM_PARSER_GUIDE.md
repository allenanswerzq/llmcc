# Custom Parser Implementation Guide

## Quick Start: Adding Your Own Parser

This guide shows you how to implement a custom parser for llmcc by leveraging the new `ParseTree` abstraction.

## The Architecture

```
Your Custom Parser
        ↓
    ParseTree Trait Implementation
        ↓
    LanguageTrait Implementation
        ↓
CompileCtxt::from_sources::<YourLang>()
        ↓
Rest of llmcc (unchanged)
```

## Step 1: Create ParseTree Implementation

```rust
use std::any::Any;
use llmcc_core::lang_def::ParseTree;

// Define your custom AST/tree structure
pub struct MyCustomTree {
    root: MyNodeType,
    // ... other fields
}

impl ParseTree for MyCustomTree {
    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn debug_info(&self) -> String {
        // Provide useful debugging information
        format!("MyParser(nodes: {})", self.count_nodes())
    }
}
```

**Requirements:**
- Must implement `Send + Sync` (for `rayon` parallel parsing)
- Implement `as_any()` for safe downcasting
- Implement `debug_info()` for diagnostics

## Step 2: Implement LanguageTrait

```rust
use llmcc_core::lang_def::{LanguageTrait, ParseTree, HirKind};
use llmcc_core::graph_builder::BlockKind;

#[derive(Debug)]
pub struct LangMyLanguage {}

impl LanguageTrait for LangMyLanguage {
    // YOUR PARSER: Return Box<dyn ParseTree>
    fn parse(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
        let source = text.as_ref();

        // Call your parser
        let tree = my_parser::parse(source)?;

        // Wrap in ParseTree trait object
        Some(Box::new(MyCustomTree {
            root: tree,
            // ... initialize other fields
        }))
    }

    // SEMANTIC MAPPINGS: Same as before
    fn hir_kind(kind_id: u16) -> HirKind {
        match kind_id {
            0 => HirKind::File,
            1 => HirKind::Scope,
            2 => HirKind::Identifier,
            _ => HirKind::Internal,
        }
    }

    fn block_kind(kind_id: u16) -> BlockKind {
        match kind_id {
            1 => BlockKind::Func,
            _ => BlockKind::Undefined,
        }
    }

    fn token_str(kind_id: u16) -> Option<&'static str> {
        match kind_id {
            0 => Some("module"),
            1 => Some("function"),
            2 => Some("identifier"),
            _ => None,
        }
    }

    fn is_valid_token(kind_id: u16) -> bool {
        matches!(kind_id, 0 | 1 | 2)
    }

    fn name_field() -> u16 {
        2 // identifier
    }

    fn type_field() -> u16 {
        2 // identifier
    }

    fn supported_extensions() -> &'static [&'static str] {
        &["myext", "ml"]
    }
}
```

## Step 3: Use with CompileCtxt

```rust
use llmcc_core::context::CompileCtxt;

// Existing code works unchanged:
let source = b"your code here".to_vec();
let cc = CompileCtxt::from_sources::<LangMyLanguage>(&[source]);

// Access your custom tree when needed:
if let Some(parse_tree) = cc.get_parse_tree(0) {
    if let Some(my_tree) = parse_tree.as_any().downcast_ref::<MyCustomTree>() {
        // Use your custom tree features
        println!("Parsed {} nodes", my_tree.count_nodes());
    }
}

// For existing code that expects tree-sitter:
// cc.get_tree(0) -> Option<&Tree>  // Only works if you wrapped TreeSitterParseTree
```

## Migration Path for Tree-Sitter Users

### Option 1: Keep Using Tree-Sitter (Minimal Change)

```rust
// Just use the existing define_tokens! macro - no changes needed!
define_tokens!(
    MyRust,
    (source_file, 0, "source_file", HirKind::File),
    (function_item, 1, "function_item", HirKind::Scope, BlockKind::Func),
    // ...
);
// Your parser still works exactly the same
```

### Option 2: Switch to Custom Parser (Complete Control)

```rust
// Implement LanguageTrait manually instead of using macro
impl LanguageTrait for LangMyCustom {
    fn parse(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
        // Your custom parsing logic
    }
    // ... other methods
}
```

## Type Erasure: Safe Downcasting

The `ParseTree` trait uses type erasure for flexibility:

```rust
// Getting the parse tree
let tree: Option<&Box<dyn ParseTree>> = cc.get_parse_tree(0);

// Safe downcasting - returns Option, never panics
if let Some(custom) = tree.and_then(|t|
    t.as_any().downcast_ref::<MyCustomTree>()
) {
    // Use custom tree features
}

// Alternative using match
match tree {
    Some(pt) => {
        if let Some(custom) = pt.as_any().downcast_ref::<MyCustomTree>() {
            // Custom tree branch
        } else if let Some(ts) = pt.as_any().downcast_ref::<TreeSitterParseTree>() {
            // Tree-sitter branch
        }
    }
    None => { /* no tree */ }
}
```

## Performance Considerations

| Aspect | Impact | Mitigation |
|--------|--------|-----------|
| **Memory** | One extra `Box<dyn ParseTree>` per file | Minimal (16 bytes) |
| **Parsing** | No overhead; same parser code runs | Use efficient parser algorithm |
| **Downcasting** | Single virtual call + pointer check | Cache downcasted reference if needed often |
| **Cached trees** | Two `Vec<Option<...>>` instead of one | Trade-off for backward compatibility |

## Real-World Examples

### Example 1: Custom AST Parser

```rust
pub struct CustomAstTree {
    nodes: Vec<Node>,
    root_id: NodeId,
}

impl ParseTree for CustomAstTree {
    fn as_any(&self) -> &(dyn Any + Send + Sync) { self }
    fn debug_info(&self) -> String {
        format!("CustomAST({} nodes)", self.nodes.len())
    }
}

impl LanguageTrait for LangCustom {
    fn parse(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
        let ast = my_parser::build_ast(text.as_ref())?;
        Some(Box::new(CustomAstTree {
            nodes: ast.nodes,
            root_id: ast.root,
        }))
    }
    // ... semantic mappings
}
```

### Example 2: Incremental Parser

```rust
pub struct IncrementalTree {
    tree: ParseTree,
    version: u64,
}

// Implement incremental updates
impl IncrementalTree {
    pub fn update(&mut self, changes: &[Change]) -> Result<()> {
        // Update only affected parts
        self.tree = incremental_parser::apply_changes(&self.tree, changes)?;
        self.version += 1;
        Ok(())
    }
}

impl ParseTree for IncrementalTree {
    fn as_any(&self) -> &(dyn Any + Send + Sync) { self }
    fn debug_info(&self) -> String {
        format!("Incremental(v{})", self.version)
    }
}
```

### Example 3: Language Variant Adapter

```rust
// Reuse existing parser with different semantic mapping
pub struct Rust2020Tree {
    tree: TreeSitterParseTree,
    // Rust 2020 edition specific info
}

impl ParseTree for Rust2020Tree {
    fn as_any(&self) -> &(dyn Any + Send + Sync) { self }
    fn debug_info(&self) -> String {
        "Rust2020(edition)".to_string()
    }
}

impl LanguageTrait for LangRust2020 {
    fn parse(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
        // Reuse tree-sitter
        let ts_tree = rust_parser::parse(text.as_ref())?;
        Some(Box::new(Rust2020Tree {
            tree: TreeSitterParseTree { tree: ts_tree },
        }))
    }

    fn hir_kind(kind_id: u16) -> HirKind {
        // 2021 edition specific mappings
    }
}
```

## Testing Your Parser

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use llmcc_core::context::CompileCtxt;

    #[test]
    fn test_parse_basic() {
        let code = b"fn main() {}".to_vec();
        let tree = LangMyLang::parse(&code);
        assert!(tree.is_some());
    }

    #[test]
    fn test_with_compile_context() {
        let code = b"fn test() {}".to_vec();
        let cc = CompileCtxt::from_sources::<LangMyLang>(&[code]);

        let tree = cc.get_parse_tree(0);
        assert!(tree.is_some());

        // Access custom features
        if let Some(custom) = tree.unwrap().as_any().downcast_ref::<MyTree>() {
            assert_eq!(custom.node_count(), 2); // e.g., module + function
        }
    }

    #[test]
    fn test_semantic_mappings() {
        assert_eq!(LangMyLang::hir_kind(0), HirKind::File);
        assert_eq!(LangMyLang::token_str(1), Some("function"));
        assert!(LangMyLang::is_valid_token(1));
        assert!(!LangMyLang::is_valid_token(999));
    }
}
```

## Compatibility with Existing Code

### ✅ Works Unchanged
- `define_tokens!` macro with tree-sitter
- Visitor trait generation
- HIR building
- Symbol resolution
- Block analysis

### ✅ Enhanced
- `CompileCtxt::get_parse_tree()` - new generic access
- Custom parsers can coexist with tree-sitter
- Language variants possible

### ⚠️ Considerations
- `CompileCtxt::get_tree()` only works if tree-sitter is available
- For non-tree-sitter parsers, use `get_parse_tree()` and downcast
- Existing code using `tree()` method on CompileUnit continues to work

## Troubleshooting

### "trait `Sync` not implemented"
Make sure your `ParseTree` impl and all fields are `Send + Sync`:
```rust
// ❌ Won't work
pub struct BadTree {
    cell: std::cell::RefCell<Data>,  // RefCell is not Sync
}

// ✅ Works
pub struct GoodTree {
    data: std::sync::RwLock<Data>,  // RwLock is Sync
}
```

### "downcast_ref returns None"
Make sure you're downcasting to the actual concrete type:
```rust
// ❌ Wrong type
let custom = pt.as_any().downcast_ref::<TreeSitterParseTree>();

// ✅ Right type
let custom = pt.as_any().downcast_ref::<MyCustomTree>();
```

### "cannot clone parse tree"
ParseTree is not Clone - it's trait object. Work with references:
```rust
// ❌ Won't compile
let tree1 = cc.get_parse_tree(0).clone();

// ✅ Works with references
if let Some(tree) = cc.get_parse_tree(0) {
    // Use tree by reference
}
```

## Next Steps

1. **Implement ParseTree** for your custom AST
2. **Implement LanguageTrait** with your parser logic
3. **Test with CompileCtxt::from_sources**
4. **Integrate** with existing llmcc analysis pipeline
5. **Benchmark** parser performance with your AST

---

For more details on the refactoring, see `REFACTORING_NOTES.md`.
