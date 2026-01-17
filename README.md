# llmcc

**multi-depth architecture view for codebases in extremely fast speed**

**problem**: grep and rag based solution don't scale well: slow searches, token cost, stale indexes, expensive cloud infra etc. they dont work too well on large codebases.

llmcc tries a different approach. It builds a multi-depth architecture view that lets agents *zoom out* to see the big picture, then *zoom in* to see extact symbols they need, such that agents can have a highly comprehensive understanding in very fast speed, **no complex rag stuff**, fully agentic method, its like grep but for architecture.

## Supported Languages

| Language | Status |
|----------|--------|
| Rust | ✅ Supported |
| TypeScript | ✅ Supported |
| C++ | 🔜 Planned |
| Python | 🔜 Planned |
| Go | 🔜 Planned |
| more |

## Why multi-depth graphs?

People (and coding agents) need to understand systems from different dimensions. Sometimes you need the high-level architecture to see boundaries, ownership, and how subsystems connect; other times you need the low-level implementation details to make a safe, precise change. llmcc provides multiple depths so you can choose the right “distance” from the code for the task.

| Depth | Perspective | Best for |
|------:|-------------|----------|
| 0 | Project | multi-workspace / repo-to-repo relationships |
| 1 | Library/Crate | ownership boundaries, public API flow |
| 2 | Module | subsystem structure, refactor planning |
| 3 | File + symbol | implementation details, edit planning |

## Walkthrough: Codex (midterm size multi-crate rust project)

This repo includes many examples under [sample](sample). Download and open them in browser for the best viewing experience.

### Depth 1: crate graph

<p style="height: 200px; text-align: center;">
	<img src="sample/rust/codex-pagerank/depth_1_crate.svg" alt="Codex crate graph (depth 1)" style="max-width: 100%; height: 100%;" />
</p>

### Depth 2: module graph

<p align="center">
	<img src="sample/rust/codex-pagerank/depth_2_module.svg" alt="Codex module graph (depth 2)" style="max-width: 70%; height: auto;" />
</p>

### Depth 3: file + symbol graph

<p align="center">
	<img src="sample/rust/codex-pagerank/depth_3_file.svg" alt="Codex file and symbol graph (depth 3)" style="max-width: 100%; height: auto;" />
</p>

Here's a small portion of the graph at depth 3, showing the core abstraction layer for prompt handling in Codex. Developers and AI agents can quickly grasp the architecture by examining this view.

<p style="height: 200px; text-align: center;">
	<img src="doc/codex.jpg" alt="codex core logic" style="max-width: 100%; height: auto;" />
</p>


## Performance

llmcc is designed to be very fast, and we will try to make it faster.

The repo contains benchmark for many famous project output here: [sample/benchmark_results_16.md](sample/benchmark_results_8_linux_rust.md).

Excerpt (PageRank timing, depth=3, top-200):

| Project | Files | LoC | Total |
|---------|-------|-----|-------|
| databend | 3130 | 627K | 2.53s |
| ruff | 1661 | 418K | 1.73s |
| codex | 617 | 224K | 0.46s |

## Installation

### npm / npx (Recommended)

The easiest way to use llmcc is via npm. No build required:

```bash
# Or install globally
npm install -g llmcc-cli
llmcc --help
```

### Cargo (Rust)

```bash
cargo install llmcc
```

### From Source

```bash
git clone https://github.com/allenanswerzq/llmcc.git
cd llmcc
cargo build --release
./target/release/llmcc --help
```

## CLI: generate graphs

Generate a crate-level graph for Codex (DOT to stdout):

```bash
llmcc \
	-d sample/repos/codex/codex-rs \
	--graph \
	--lang rust \
	--depth 1
```

Generate a PageRank-filtered file+symbol graph (write to a file):

```bash
llmcc \
	-d sample/repos/codex/codex-rs \
	--graph \
	--depth 3 \
	--pagerank-top-k 200 \
	--lang rust \
	-o /tmp/codex_depth3_pagerank.dot
```

Render DOT to SVG (requires Graphviz):

```bash
dot -Tsvg /tmp/codex_depth3_pagerank.dot -o /tmp/codex_depth3_pagerank.svg
```

For generating sample graphs:

```bash
just gen rust
```
