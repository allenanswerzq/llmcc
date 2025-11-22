# llmcc-rust

This crate provides Rust language support for the **llmcc** project. It implements the language-specific logic for parsing, symbol collection, and semantic analysis (binding) of Rust code.

## Overview

`llmcc-rust` integrates with the core compiler infrastructure to provide:
- **Parsing**: Uses `tree-sitter-rust` to generate an AST.
- **Symbol Collection**: A first pass to declare all symbols (structs, functions, variables) in the scope graph.
- **Symbol Binding**: A second pass to resolve references, infer types, and build the dependency graph.

## Architecture

The analysis pipeline consists of three main stages, orchestrated by the `LangRust` implementation in `src/token.rs`.

### 1. Parsing (`src/token.rs`)
The entry point for the crate. It defines the `LangRust` struct which implements the `LanguageTraitImpl`.
- Wraps `tree-sitter-rust` to produce a concrete syntax tree.
- Maps Tree-sitter nodes to LLMCC's internal HIR (High-level Intermediate Representation).
- Auto-generates token definitions via `build.rs`.

### 2. Symbol Collection (`src/collect.rs`)
The **Collection Pass** walks the AST to identify and declare definitions.
- **Visitor**: `CollectorVisitor` traverses the AST.
- **Scopes**: Creates scopes for modules, functions, structs, and blocks.
- **Declarations**: Registers symbols for:
  - Primitives (`i32`, `bool`, etc.)
  - Modules and Crates (parsing `Cargo.toml` via `src/util.rs`)
  - Functions, Structs, Enums, Traits
  - Variables (via pattern matching in `let` bindings and parameters)
- **Visibility**: Handles `pub` and `pub(crate)` modifiers to determine global symbol visibility.

### 3. Symbol Binding (`src/bind/`)
The **Binding Pass** resolves identifiers to their definitions and builds the call graph. This module is split into focused components:

- **Visitor (`src/bind/visitor.rs`)**: The main driver, `BinderVisitor`, walks the AST again.
- **Resolution (`src/bind/resolution.rs`)**: `SymbolResolver` handles complex name lookups, including:
  - Lexical scoping (variables).
  - Path resolution (`std::collections::HashMap`).
  - Method resolution (looking up methods in `impl` blocks).
- **Inference (`src/bind/inference.rs`)**: `ExprResolver` determines the types of expressions to support accurate method resolution.
- **Linking (`src/bind/linker.rs`)**: `SymbolLinker` connects usage sites to definition sites, forming the dependency graph used by downstream LLM tasks.

### Utilities (`src/util.rs`)
Helper functions for filesystem and project structure analysis:
- `parse_crate_name`: Extracts crate names from `Cargo.toml`.
- `parse_module_name`: Handles Rust's module system conventions (e.g., `mod.rs`).

## Development

### Testing
The crate includes extensive unit tests ensuring correct symbol resolution and dependency tracking.

```bash
# Run all tests for this crate
cargo test -p llmcc-rust
```

### Adding New Features
1. **New Syntax**: Update `src/token.rs` (or the build script) if new token types are needed.
2. **New Declarations**: Update `CollectorVisitor` in `src/collect.rs` to register new symbol kinds.
3. **New Resolution Logic**: Update `src/bind/` modules to handle new scoping rules or reference types.
