# llmcc-simple

A minimal, publicly accessible test language implementation for llmcc testing.

## Purpose

`llmcc-simple` provides a simple language implementation that can be used across all llmcc crates for testing purposes. Unlike the test module in `llmcc-core` (which is only available during `#[cfg(test)]`), this crate is publicly available and can be used in dev-dependencies.

## Features

- **LangSimple**: A minimal language implementation with language traits and constants
- **SimpleParseNode**: A simple AST node representation for testing
- **SimpleParseTree**: A custom parse tree wrapper
- **simple_parser**: A basic line-by-line parser that recognizes:
  - `fn ` prefix as function definitions
  - Non-empty, non-comment lines as statements
  - `//` prefix as comments (ignored)

## Usage

Add to your `Cargo.toml` dev-dependencies:

```toml
[dev-dependencies]
llmcc-simple = { path = "../llmcc-simple", version = "0.2.50" }
```

Then use in your tests:

```rust
use llmcc_simple::LangSimple;
use llmcc_core::context::CompileCtxt;

#[test]
fn my_test() {
    let cc = CompileCtxt::from_sources::<LangSimple>(&[]);
    // Your test code here
}
```

## Example

```rust
use llmcc_simple::{LangSimple, SimpleParseNode};

let source = b"fn main() {}\nfn helper() {}\nlet x = 42;";
let parse_tree = LangSimple::parse(source).expect("Parsing should succeed");
```
