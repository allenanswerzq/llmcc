# llmcc-format

Machine-readable JSON graph format for llmcc.

`llmcc-dot` renders architecture graphs for humans. This crate renders the same collected graph facts into a stable JSON document for tests, agents, and downstream tools.

The initial schema is `llmcc.graph` version `1`.

## Contract

The JSON document is intentionally small:

- `schema` and `schema_version` identify the contract.
- `depth` records whether nodes are file-level blocks or project/package/namespace aggregates.
- `nodes` contain stable ids, labels, kinds, source line metadata, and contributing block ids.
- `edges` point from dependency source to dependency target and include the semantic edge kind plus weight.

Absolute paths are not part of the stable JSON contract. File-level nodes keep file name and line only; package and namespace metadata carry architecture grouping.

## Test Backend

`llmcc-test` now uses this crate directly as its assertion backend. JSON test suites store the expected `llmcc.graph` document instead of asserting DOT text or old `.llmcc` expectation blocks.
