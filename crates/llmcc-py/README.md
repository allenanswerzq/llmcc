# llmcc-py

This crate provides Python language support for the **llmcc** project. It implements the language-specific logic for parsing, symbol collection, and semantic analysis (binding) of Python code.

## Overview

`llmcc-py` integrates with the core compiler infrastructure to provide:
- **Parsing**: Uses `tree-sitter-python` to generate an AST.
- **Symbol Collection**: A first pass to declare all symbols (classes, functions, variables) in the scope graph.
- **Symbol Binding**: A second pass to resolve references, infer types, and build the dependency graph.

## Architecture

The analysis pipeline consists of three main stages, orchestrated by the `LangPython` implementation in `src/token.rs`.

### 1. Parsing (`src/token.rs`)
The entry point for the crate. It defines the `LangPython` struct which implements the `LanguageTraitImpl`.
- Wraps `tree-sitter-python` to produce a concrete syntax tree.
- Maps Tree-sitter nodes to LLMCC's internal HIR (High-level Intermediate Representation).
- Auto-generates token definitions via `build.rs`.

### 2. Symbol Collection (`src/collect.rs`)
The **Collection Pass** walks the AST to identify and declare definitions.
- **Visitor**: `CollectorVisitor` traverses the AST.
- **Scopes**: Creates scopes for modules, classes, functions, and blocks.
- **Declarations**: Registers symbols for:
  - Built-in types (`int`, `str`, `bool`, etc.)
  - Modules and Packages
  - Classes and Functions
  - Variables (via assignment and parameters)
- **Visibility**: Handles Python's `__all__` and naming conventions for public/private visibility.

### 3. Symbol Binding (`src/bind.rs`)
The **Binding Pass** resolves identifiers to their definitions and builds the call graph.
- **Resolution**: Handles lexical scoping and attribute access.
- **Inference**: Determines types of expressions where possible.
- **Linking**: Connects usage sites to definition sites, forming the dependency graph.

### Utilities (`src/util.rs`)
Helper functions for filesystem and project structure analysis:
- `parse_module_name`: Handles Python's module system conventions.
- `parse_package_name`: Extracts package names from `pyproject.toml` or `setup.py`.

## Development

### Testing
The crate includes extensive unit tests ensuring correct symbol resolution and dependency tracking.

```bash
# Run all tests for this crate
cargo test -p llmcc-py
```

### Adding New Features
1. **New Syntax**: Update `src/token.rs` (or the build script) if new token types are needed.
2. **New Declarations**: Update `CollectorVisitor` in `src/collect.rs` to register new symbol kinds.
3. **New Resolution Logic**: Update `src/bind.rs` to handle new scoping rules or reference types.
