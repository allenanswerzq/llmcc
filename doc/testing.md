# llmcc-test

`llmcc-test` is a lightweight corpus runner inspired by tree-sitter's
`test/corpus` format. Each corpus file (`*.llmcc`) contains one or more test
cases that materialize an in-memory project, run part of the llmcc pipeline, and
compare the result against inline expectations.

```
=== basic function symbols ===
lang: rust

--- file: src/lib.rs ---
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

--- expect:symbols ---
 0 | Function     | crate::add [global]
```

## CLI

```
cargo run -p llmcc-test -- run        # execute entire corpus
cargo run -p llmcc-test -- run --filter basic
cargo run -p llmcc-test -- run --update   # bless snapshots
cargo run -p llmcc-test -- list      # discover case ids
```

- `--root DIR` selects a different corpus directory (default `tests/corpus`).
- `--filter SUBSTR` only runs cases whose id contains `SUBSTR`.
- `--update` rewrites the `--- expect:* ---` blocks with the fresh output.

## File format

* `=== case name ===` starts a new case. Cases are scoped by their file path,
  so the full id is `<relative/path>::<case name>`.
* Optional metadata:
- `lang: rust|python` (defaults to `rust`; other languages will be wired up once their pipelines expose the new resolver APIs)
  - `args: ...` (reserved for future CLI-based assertions)
* `--- file: relative/path ---` declares a virtual source file. Multiple files
  may exist per case, allowing cross-file relationships.
* `--- expect:symbols ---` stores a textual snapshot of the symbol table derived
  from the resolver. Additional expectation kinds will be added as the harness
  evolves (e.g. parse trees, design graphs, CLI output).

All sections are separated by blank lines. The runner re-serializes files when
`--update` is used, so the format is canonical.

## Workflow

1. Add or edit a case under `tests/corpus/<lang>/<suite>.llmcc`.
2. Leave the expectation block empty (or outdated).
3. Run `cargo run -p llmcc-test -- run --filter <case> --update` to bless.
4. Run without `--update` to ensure regression coverage.

These tests cover the entire “parse → IR → symbol collection” flow per language,
making it straightforward to introduce new syntax or resolver rules without
assembling ad-hoc repositories. More expectation kinds will be layered on top of
the same infrastructure.
