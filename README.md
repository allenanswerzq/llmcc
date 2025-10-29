# llmcc

*"Prompts are the modern assembly language, models are the modern CPU."*

llmcc is a universal context builder for any language, any document.

## abstract

llmcc explores automated context generation through symbolic graph analysis. bridging the semantic gap between human-written code/documents and AI model understanding, using modern compiler design principles.

## design

![design](doc/design.svg)

## run

`llmcc` accepts repeated `--file` inputs or repeated `--dir` inputs (choose one mode per run) and targets Rust by default. Sample commands covering the main CLI surfaces:

- High level design graph with PageRank:

	```bash
	llmcc --dir ../codex/codex-rs --design-graph --pagerank --top-k 100
	```

- Switch to Python analysis:

	```bash
	llmcc --dir ../proj --lang python
	```

- Direct dependencies of a symbol:

	```bash
	llmcc --dir ../codex/codex-rs/core --query Codex --depends
	```

- Transitive dependency fan-out:

	```bash
	llmcc --dir ../codex/codex-rs/core --query Codex --depends --recursive
	```

- Direct dependents of a symbol:

	```bash
	llmcc --dir ../codex/codex-rs/core --query Codex --dependents
	```

- Transitive dependents (callers) view:

	```bash
	llmcc --dir ../codex/codex-rs/core --query Codex --dependents --recursive
	```

- Metadata-only summary (file + line ranges):

	```bash
	llmcc --dir ../codex/codex-rs/core --query Codex --depends --summary
	```

- Analyze multiple files in one run:

	```bash
	llmcc --file src/main.rs --file src/lib.rs --query init_system
	```

- Combine several directories (Rust default):

	```bash
	llmcc --dir ../codex/codex-rs/core --dir ../codex/codex-rs/tui --pagerank --top-k 100
	```
