# llmcc

**llmcc brings multi-depth architecture graphs for code understanding and generation.**

Our goal is to build a multi-depth, tree-like context / architecture view of a codebase, so a coding agent can *walk up* (zoom out) for structure and intent, then *walk down* (zoom in) to the exact crates/modules/files/symbols it needs—getting a highly comprehensive understanding of any codebase (any programming language) and any documents.

## Why multi-depth graphs?

People (and coding agents) need to understand systems from different dimensions. Sometimes you need the high-level architecture to see boundaries, ownership, and how subsystems connect; other times you need the low-level implementation details to make a safe, precise change. llmcc provides multiple depths so you can choose the right “distance” from the code for the task.

| Depth | Perspective | Best for |
|------:|-------------|----------|
| 0 | Project | multi-workspace / repo-to-repo relationships |
| 1 | Library/Crate | ownership boundaries, public API flow |
| 2 | Module | subsystem structure, refactor planning |
| 3 | File + symbol | implementation details, edit planning |

## Walkthrough: Codex (midterm size multi-crate rust project)

This repo includes a ready-made example under [sample](sample). Download and open them in browser for the best viewing experience.

### Depth 1: crate graph

<p align="center">
	<img src="sample/rust/codex-pagerank/depth_1_crate.svg" alt="Codex crate graph (depth 1)" style="max-width: 50%; height: 70%;" />
</p>

### Depth 2: module graph

<p align="center">
	<img src="sample/rust/codex-pagerank/depth_2_module.svg" alt="Codex module graph (depth 2)" style="max-width: 70%; height: auto;" />
</p>

### Depth 3: file + symbol graph

<p align="center">
	<img src="sample/rust/codex-pagerank/depth_3_file.svg" alt="Codex file and symbol graph (depth 3)" style="max-width: 100%; height: auto;" />
</p>

If you open those .dot/.svg files, you’ll see the same system from different “altitudes”, which is exactly what you want when:
- orienting yourself in an unfamiliar repo
- deciding *where* to make a change
- generating a focused context pack for an coding agents


## Performance

llmcc is designed to be very fast, and we will try to make it faster.

The repo contains benchmark for many famous project output here: [sample/benchmark_results_16.md](sample/benchmark_results_8_linux.md).

Excerpt (PageRank timing, depth=3, top-200):

| Project | Files | LoC | Total |
|---------|-------|-----|-------|
| databend | 3130 | 627K | 3.03s |
| ruff | 1661 | 418K | 2.23s |
| codex | 617 | 224K | 0.60s |

## CLI: generate graphs

Build the binary:

```bash
cargo build --release
```

Generate a crate-level graph for Codex (DOT to stdout):

```bash
./target/release/llmcc \
	-d sample/repos/codex/codex-rs \
	--graph \
	--depth 1
```

Generate a PageRank-filtered file+symbol graph (write to a file):

```bash
./target/release/llmcc \
	-d sample/repos/codex/codex-rs \
	--graph \
	--depth 3 \
	--pagerank-top-k 200 \
	-o /tmp/codex_depth3_pagerank.dot
```

Render DOT to SVG (requires Graphviz):

```bash
dot -Tsvg /tmp/codex_depth3_pagerank.dot -o /tmp/codex_depth3_pagerank.svg
```

Tip: for all the sample repos + all depths, run:

```bash
./sample/generate_all.sh
```
