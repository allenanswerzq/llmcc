# llmcc

*"Prompts are the modern assembly language, models are the modern CPU."*

llmcc is a universal context builder for any language, any document.

## abstract

llmcc explores automated context generation through symbolic graph analysis. bridging the semantic gap between human-written code/documents and AI model understanding, using modern compiler design principles.

## design

![design](doc/design.svg)

## run

- High level design graph with PageRank:

	```bash
	llmcc --dir ../codex/codex-rs --lang rust --design-graph --pagerank --top-k 100
	```

- Direct dependencies of a symbol:

	```bash
	llmcc --dir ../codex/codex-rs/core --lang rust --query Codex --depends
	```

- Transitive dependency fan-out:

	```bash
	llmcc --dir ../codex/codex-rs/core --lang rust --query Codex --depends --recursive
	```

- Direct dependents of a symbol:

	```bash
	llmcc --dir ../codex/codex-rs/core --lang rust --query Codex --dependents
	```

- Transitive dependents (callers) view:

	```bash
	llmcc --dir ../codex/codex-rs/core --lang rust --query Codex --dependents --recursive
	```

- Metadata-only summary (file + line ranges), instead of code texts:

	```bash
	llmcc --dir ../codex/codex-rs/core --lang rust --query Codex --depends --summary
	```

- Apply to multiple directories, analyze relation not only inside each dir, but also cross dir:

	```bash
	llmcc --dir ../codex/codex-rs/core --dir ../codex/codex-rs/tui --lang rust --design-graph --pagerank --top-k 100
	```

- Analyze multiple files in one run:

	```bash
	llmcc --file src/main.rs --file src/lib.rs --lang rust --query init_system
	```

## python bindings (uv)

The Python workflow now relies on [uv](https://github.com/astral-sh/uv) for environment
management. To build and exercise the bindings:

```bash
uv sync --extra dev
uv run maturin develop --manifest-path crates/llmcc-bindings/Cargo.toml
uv run python examples/basic.py
```

The script exercises the single-file, multi-file, and multi-directory cases along with the
design-graph, PageRank, dependents, and summary options exposed through the CLI.
