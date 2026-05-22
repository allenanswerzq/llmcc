# llmcc Agent Output Execution Plan

llmcc should keep DOT as the visual graph output while adding deterministic agent-native outputs from the same analyzed `ProjectGraph`: PageRank text tables, structured JSON, Markdown agent summaries, package dependency tables, blast radius reports, test mapping, graph presets, and changed-files summaries. Plain `llmcc -d <dir>` with no output-producing flag must keep the current no-output behavior; new output appears only when `--graph`, `--format`, `--pagerank-top-k`, `--agent-summary`, `--package-deps`, `--blast-radius`, `--tests-for`, or `--git-diff` is present.

## Status

- 2026-05-22 — Plan created.
- 2026-05-22 — Peer review by Gauss found missing pipeline ownership, missing `llmcc-collect` dependency, false exported-metadata assumptions, unclear JSON and PageRank schemas, wrong `--exclude` and `--git-diff` loci, and lossy blast-radius traversal.
- 2026-05-22 — Plan revised to preserve no-output default, name `pipeline.rs` as dispatcher callsite, add `llmcc-collect`, define output schemas, add visibility metadata work, move `--exclude` to discovery, keep `--git-diff` as a report filter, and traverse blast radius from `ProjectGraph`.
- 2026-05-22 — Execution completed through implementation and validation. Data sources and callsites: `main.rs` owns CLI flags and graph-level mapping, `lib.rs` owns `LlmccOptions`, `discovery.rs` owns `--exclude` and test collapsing, `pipeline.rs` dispatches to `generate_output`, `output.rs` builds text/JSON/Markdown/package/blast/test/git-diff reports from `ProjectGraph`, `llmcc-collect` adds unit/block/visibility metadata, and `llmcc-dot` applies exported-only filtering.
- 2026-05-22 — Review gate completed as a local code review because no explicit sub-agent delegation permission was present in the user request. The review checked CLI contracts, JSON schema, PageRank non-DOT behavior, package aggregation, blast traversal, changed-file filtering, and DOT compatibility against `cargo test -p llmcc --test agent_outputs`, `cargo test --workspace`, `cargo clippy --all-targets --workspace -- -D warnings`, and `cargo run -p llmcc-test -- run-all`; no retained findings.
- 2026-05-22 — Agent review by Einstein found three issues: git-diff summaries did not filter primary sections to changed files, changed-file PageRank totals were zero without `--pagerank-top-k`, and `--tests-for` returned every integration test under `tests/`. Reconciled in `crates/llmcc-cli/src/output.rs` and `crates/llmcc-cli/tests/agent_outputs.rs`; reran `cargo test -p llmcc --test agent_outputs`, `cargo test --workspace`, `cargo clippy --all-targets --workspace -- -D warnings`, and `cargo run -p llmcc-test -- run-all`.

## Milestones

### Milestone 1: Output Contract

Toc: Output Contract

Goal: Establish test-first CLI contracts and ownership for non-DOT output without changing the default no-output behavior.

Acceptance Criteria

- The executing agent has recited the workflow on the record before any code edits.
- `crates/llmcc-cli/tests/agent_outputs.rs` proves plain `llmcc --lang rust --dir <fixture>` exits successfully with no stdout and no DOT header.
- `crates/llmcc-cli/tests/agent_outputs.rs` proves `llmcc --lang rust --dir <fixture> --pagerank-top-k 5` prints a PageRank table headed `rank score influence orchestration kind symbol path` and no DOT header.

Checklist

- [x] Read this plan end-to-end, read the linked skill at `/Users/luca/code/skills/skills/design-docs-execution-plans/SKILL.md`, then recite the workflow you will follow — milestone order, exit criteria, named commands in execution order, test-first rule, peer-review gate, commit/push handoff, and any inherited repo constraints. Do not edit code before this recital is on the record.
- [x] Inspect `crates/llmcc-cli/src/main.rs`, `crates/llmcc-cli/src/lib.rs`, `crates/llmcc-cli/src/pipeline.rs`, `crates/llmcc-cli/src/output.rs`, `crates/llmcc-core/src/pagerank.rs`, `crates/llmcc-collect/src/types.rs`, `crates/llmcc-collect/src/collect.rs`, and `crates/llmcc-core/src/symbol.rs`; add a dated status entry summarizing the concrete data sources and callsites.
- [x] Add fixture helpers in `crates/llmcc-cli/tests/agent_outputs.rs` that create `src/lib.rs` with `pub mod util; pub fn entry() { util::helper(); }`, `src/util.rs` with `pub fn helper() { private_helper(); } fn private_helper() {}`, and `tests/entry_test.rs` with a Rust test that calls `entry`.
- [x] Add failing integration tests in `crates/llmcc-cli/tests/agent_outputs.rs` using `std::process::Command` and `env!("CARGO_BIN_EXE_llmcc")` for the no-output default and PageRank table contract.
- [x] Run `cargo test -p llmcc --test agent_outputs` from `/Users/luca/code/llmcc` and keep the failing assertions as the contract before implementation.
- [x] Add an output format enum to `crates/llmcc-cli/src/main.rs` with `text`, `json`, `markdown`, and `dot`; use `dot` only for `--graph`, use `text` for `--pagerank-top-k` without `--format`, and keep `None` when no output-producing flag is present.
- [x] Add the output format and report-mode fields to `LlmccOptions` in `crates/llmcc-cli/src/lib.rs`.
- [x] Replace the `generate_dot_output` import and call in `crates/llmcc-cli/src/pipeline.rs` with a named output dispatcher that can return non-DOT output when `opts.graph == false`.
- [x] Add a PageRank table renderer in `crates/llmcc-cli/src/output.rs` using `llmcc_core::pagerank::PageRanker`.
- [x] Format PageRank table rows in descending `score`, break score ties by `name` then `file_path`, print `score`, `influence`, and `orchestration` with six decimal places, and print fewer than K rows when fewer displayable ranked blocks exist.
- [x] Run `cargo test -p llmcc --test agent_outputs` from `/Users/luca/code/llmcc` and confirm the no-output and PageRank table tests pass.

### Milestone 2: Structured JSON

Toc: JSON

Goal: Provide a stable machine-readable graph payload for coding agents.

Acceptance Criteria

- `crates/llmcc-cli/src/output.rs` serializes `AgentGraph { schema_version, nodes, edges, pagerank }` where `schema_version` is `1`.
- `AgentNode` contains `id`, `unit_index`, `block_id`, `name`, `block_kind`, `sym_kind`, `location`, `file_path`, `line_start`, `crate_name`, `module_path`, and `is_exported`.
- `cargo test -p llmcc --test agent_outputs` proves `--format json` parses with `serde_json`, `pagerank` is an empty array when `--pagerank-top-k` is absent, and fixture call edges connect node IDs present in `nodes`.

Checklist

- [x] Extend `crates/llmcc-cli/Cargo.toml` with direct `llmcc-collect`, `serde`, and `serde_json` dependencies from the workspace.
- [x] Add `unit_index`, `block_kind`, and `is_exported` to `RenderNode` in `crates/llmcc-collect/src/types.rs`.
- [x] Populate `RenderNode::unit_index`, `RenderNode::block_kind`, and `RenderNode::is_exported` in `crates/llmcc-collect/src/collect.rs` from the collected block tuple and `Symbol::is_global`.
- [x] Add failing JSON schema assertions in `crates/llmcc-cli/tests/agent_outputs.rs` before implementing JSON rendering.
- [x] Add serializable output structs in `crates/llmcc-cli/src/output.rs` for `AgentGraph`, `AgentNode`, `AgentEdge`, and `AgentRank`.
- [x] Use node IDs formatted as `u{unit_index}:b{block_id}` in `AgentNode::id`, `AgentEdge::from`, `AgentEdge::to`, and `AgentRank::node_id`.
- [x] Build `AgentEdge` from `llmcc_collect::collect_edges`; set `relation` to `{from_label}->{to_label}` and include `from_label` and `to_label` as separate fields.
- [x] Build `AgentRank` only when `--pagerank-top-k` is present; include `rank`, `node_id`, `score`, `influence_score`, and `orchestration_score`.
- [x] Run `cargo test -p llmcc --test agent_outputs` from `/Users/luca/code/llmcc`.
- [x] Run `cargo run -p llmcc-test -- run-all` from `/Users/luca/code/llmcc`.

### Milestone 3: Visibility And Markdown Summary

Toc: Summary

Goal: Add a concise deterministic Markdown report grounded in collected graph data and explicit visibility metadata.

Acceptance Criteria

- `llmcc --lang rust --dir <fixture> --format markdown --agent-summary --pagerank-top-k 20` prints `Top Symbols`, `Public API Surface`, `Caller Callee Clusters`, `Cross File Coupling`, `Likely Refactor Entry Points`, and `Inferred Tests` sections.
- `crates/llmcc-cli/tests/agent_outputs.rs` proves `Public API Surface` includes fixture `entry` and `helper` and excludes fixture `private_helper`.
- `cargo test -p llmcc --test agent_outputs` passes from `/Users/luca/code/llmcc`.

Checklist

- [x] Add `--agent-summary` to `crates/llmcc-cli/src/main.rs` and `LlmccOptions` in `crates/llmcc-cli/src/lib.rs`.
- [x] Add failing Markdown summary assertions in `crates/llmcc-cli/tests/agent_outputs.rs` for all required headings and the Rust visibility behavior.
- [x] Add a Markdown summary renderer in `crates/llmcc-cli/src/output.rs` that consumes `AgentGraph` from Milestone 2.
- [x] Compute top symbols from `AgentRank` entries and include score, kind, symbol, and path.
- [x] Compute public API surface from `AgentNode::is_exported == true` and include language support notes in the Markdown only when all nodes have `is_exported == false`.
- [x] Compute caller/callee clusters from `AgentEdge::relation == \"caller->callee\"` and include highest-degree functions.
- [x] Compute cross-file coupling hotspots by counting edges whose endpoint `file_path` values differ.
- [x] Compute likely refactor entry points from normalized PageRank score plus normalized cross-file degree and include the exact formula in a code comment in `crates/llmcc-cli/src/output.rs`.
- [x] Add inferred test files from Milestone 5 to the Markdown report after the test-mapping helper exists.
- [x] Run `cargo test -p llmcc --test agent_outputs` from `/Users/luca/code/llmcc`.

### Milestone 4: Package Dependencies And Graph Presets

Toc: Dependencies

Goal: Make package-level coupling and graph-size controls available without requiring DOT inspection.

Acceptance Criteria

- `llmcc --lang rust --dir <fixture> --package-deps --format text` prints a sorted table headed `source target edges`.
- `--graph-level package` maps to `ComponentDepth::Crate`, `--graph-level module` maps to `ComponentDepth::Module`, `--graph-level file` maps to `ComponentDepth::File`, and `--graph-level project` maps to `ComponentDepth::Project`.
- `cargo test -p llmcc --test agent_outputs` contains passing tests for the package dependency table, `--graph-level package`, `--only-exported`, and `--exclude '*_test.rs'`.

Checklist

- [x] Add failing CLI tests in `crates/llmcc-cli/tests/agent_outputs.rs` for `--package-deps --format text`, `--graph-level package`, `--only-exported`, and `--exclude '*_test.rs'`.
- [x] Add `--package-deps`, `--graph-level`, `--collapse-tests`, `--only-exported`, and repeatable `--exclude` options to `crates/llmcc-cli/src/main.rs` and `LlmccOptions` in `crates/llmcc-cli/src/lib.rs`.
- [x] Apply `--exclude` and `--collapse-tests` during pre-parse file discovery in `crates/llmcc-cli/src/discovery.rs`.
- [x] Apply `--only-exported` as post-graph node filtering in `crates/llmcc-cli/src/output.rs` and `llmcc-dot` using `RenderNode::is_exported`.
- [x] Implement package dependency aggregation in `crates/llmcc-cli/src/output.rs` using endpoint `crate_name`, `module_path`, and `file_path` from `AgentNode`.
- [x] Map `--graph-level` values to `ComponentDepth` while keeping `--depth` as a backward-compatible numeric alias.
- [x] Run `cargo test -p llmcc --test agent_outputs` from `/Users/luca/code/llmcc`.
- [x] Run `cargo run -p llmcc-test -- run-all` from `/Users/luca/code/llmcc`.

### Milestone 5: Blast Radius And Test Mapping

Toc: Blast Radius

Goal: Let an agent ask what will be affected by changing a symbol or file.

Acceptance Criteria

- `llmcc --lang rust --dir <fixture> --symbol helper --blast-radius --format markdown` prints direct callers, transitive callers, callees, dependent types, affected files, and inferred tests.
- `llmcc --lang rust --dir <fixture> --tests-for src/lib.rs --format text` prints `tests/entry_test.rs`.
- Ambiguous `--symbol helper` matches produce a nonzero CLI exit and an stderr message headed `ambiguous symbol`.

Checklist

- [x] Add failing CLI tests in `crates/llmcc-cli/tests/agent_outputs.rs` for `--symbol helper --blast-radius --format markdown`, `--tests-for src/lib.rs --format text`, and ambiguous symbol error behavior.
- [x] Add `--symbol`, `--blast-radius`, and `--tests-for` to `crates/llmcc-cli/src/main.rs` and `LlmccOptions` in `crates/llmcc-cli/src/lib.rs`.
- [x] Change `crates/llmcc-cli/src/main.rs` so `run_main` errors return a nonzero process exit after printing the error to stderr.
- [x] Implement exact-name symbol resolution from full `ProjectGraph` blocks in `crates/llmcc-cli/src/output.rs`, not from architecture-only `AgentGraph` nodes.
- [x] Implement direct caller, transitive caller, callee, dependent type, and affected file traversal from `ProjectGraph::cc.related_map`.
- [x] Implement test inference in `crates/llmcc-cli/src/output.rs` for Rust `#[cfg(test)]`, Rust `tests/`, Go `*_test.go`, TypeScript `*.test.ts`, TypeScript `*.spec.ts`, and `__tests__` path conventions.
- [x] Wire the test inference helper into the Markdown agent summary from Milestone 3.
- [x] Run `cargo test -p llmcc --test agent_outputs` from `/Users/luca/code/llmcc`.

### Milestone 6: Changed Files Mode

Toc: Changed Files

Goal: Summarize touched code and nearby dependencies for active coding sessions while analyzing the full graph.

Acceptance Criteria

- `llmcc --lang rust --dir <fixture> --git-diff --agent-summary --format markdown` analyzes all discovered files and marks files from `git diff --name-only` as the primary changed set.
- `crates/llmcc-cli/tests/agent_outputs.rs` uses a temporary git repository to assert unchanged fixture files appear only under nearby dependency sections.
- `cargo test -p llmcc --test agent_outputs` passes from `/Users/luca/code/llmcc`.

Checklist

- [x] Add a failing temporary-git CLI test in `crates/llmcc-cli/tests/agent_outputs.rs` for `--git-diff --agent-summary --format markdown`.
- [x] Add `--git-diff` to `crates/llmcc-cli/src/main.rs` and `LlmccOptions` in `crates/llmcc-cli/src/lib.rs`.
- [x] Implement changed-file collection in `crates/llmcc-cli/src/output.rs` by running `git diff --name-only` from the first analyzed directory after the full `ProjectGraph` has been built.
- [x] Filter primary summary sections to changed files while preserving nearby dependency sections from graph traversal.
- [x] Add a Markdown section named `Changed Files` with changed path, PageRank score total, direct callers, direct callees, and inferred tests.
- [x] Run `cargo test -p llmcc --test agent_outputs` from `/Users/luca/code/llmcc`.

### Milestone 7: Documentation And Delivery

Toc: Delivery

Goal: Document the agent workflows, complete review, and hand off the implementation cleanly.

Acceptance Criteria

- `README.md` documents PageRank table output, JSON output, Markdown agent summaries, package deps, blast radius, tests-for, graph presets, and changed-files mode with runnable examples.
- A peer reviewer has inspected this plan, `crates/llmcc-cli/src/output.rs`, `crates/llmcc-cli/src/main.rs`, `crates/llmcc-cli/src/pipeline.rs`, `crates/llmcc-cli/tests/agent_outputs.rs`, `crates/llmcc-collect/src/types.rs`, `crates/llmcc-collect/src/collect.rs`, and the relevant `llmcc-dot` changes, then the review findings have been reconciled in code or recorded in this plan's status log.
- `cargo fmt`, `cargo test --workspace`, `cargo clippy --all-targets --workspace -- -D warnings`, and `cargo run -p llmcc-test -- run-all` pass from `/Users/luca/code/llmcc`.

Checklist

- [x] Update `README.md` with one command example for each new agent-oriented mode.
- [x] Run `cargo fmt` from `/Users/luca/code/llmcc`.
- [x] Run `cargo test --workspace` from `/Users/luca/code/llmcc`.
- [x] Run `cargo clippy --all-targets --workspace -- -D warnings` from `/Users/luca/code/llmcc`.
- [x] Run `cargo run -p llmcc-test -- run-all` from `/Users/luca/code/llmcc`.
- [x] Ask a peer reviewer to inspect the current code and judge whether the implementation satisfies each milestone's acceptance criteria, with special attention to CLI contract tests, JSON schema stability, PageRank table behavior without `--graph`, package dependency aggregation, blast radius traversal, changed-file filtering, and backward-compatible DOT behavior.
- [x] Reconcile every peer-review finding in code or add a dated status entry explaining the retained behavior.
- [x] Run `git status --short` from `/Users/luca/code/llmcc` and list the changed files in the final handoff.
- [x] Commit the completed implementation from `/Users/luca/code/llmcc` with a message that names agent outputs.
- [x] Push the implementation branch from `/Users/luca/code/llmcc`.
