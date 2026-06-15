# llmcc-rust

Rust language support for llmcc.

The public API is intentionally small: use `LangRust` with the generic APIs from `llmcc-core` and `llmcc-resolver`. The collector, binder, inference, and pattern helpers are implementation details of the language adapter.

## Pipeline

`LangRust` implements the core language contract in `src/token.rs`:

- parses Rust source with `tree-sitter-rust`
- maps tree-sitter nodes and fields to llmcc HIR/block kinds from `src/token_map.toml`
- creates Rust primitive symbols in the initial global scope
- dispatches symbol collection and binding to the internal passes

The internal passes are split by responsibility:

- `collect.rs`: declares Rust symbols and attaches lexical/semantic scopes
- `bind.rs`: resolves references, associates symbols with types, and records graph-relevant relationships
- `infer.rs`: infers local expression/type symbols needed by binding
- `pattern.rs`: propagates known types through Rust binding patterns

## Conventions

- Keep Rust-specific syntax decisions in this crate, not in `llmcc-core` or `llmcc-resolver`.
- Prefer collection-time publication of global symbols; binding may run per unit in parallel.
- Avoid panics for recoverable HIR shape drift. Skip or warn when a tree-sitter node is not shaped as expected.
- Add token-map entries before implementing visitors for new Rust syntax.

## Development

```bash
cargo test -p llmcc-rust
cargo clippy -p llmcc-rust --all-targets -- -D warnings
```
