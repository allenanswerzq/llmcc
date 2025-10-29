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
	llmcc --dir crates/llmcc-core/src --lang rust --design-graph --pagerank --top-k 100
	```

- Direct dependencies of a symbol:

	```bash
	llmcc --dir crates/llmcc-core/src --lang rust --query CompileCtxt --depends
	```

- Transitive dependency fan-out:

	```bash
	llmcc --dir crates/llmcc-core/src --lang rust --query CompileCtxt --depends --recursive
	```

- Direct dependents of a symbol:

	```bash
	llmcc --dir crates/llmcc-core/src --lang rust --query CompileCtxt --dependents
	```

- Transitive dependents (callers) view:

	```bash
	llmcc --dir crates/llmcc-core/src --lang rust --query CompileCtxt --dependents --recursive
	```

- Metadata-only summary (file + line ranges), instead of code texts:

	```bash
	llmcc --dir crates/llmcc-core/src --lang rust --query CompileCtxt --depends --summary
	```

- Apply to multiple directories, analyze relation not only inside each dir, but also cross dir:

	```bash
	llmcc --dir crates/llmcc-core/src --dir crates/llmcc-rust/src --lang rust --design-graph --pagerank --top-k 25
	```

- Analyze multiple files in one run:

	```bash
	llmcc --file crates/llmcc/src/main.rs --file crates/llmcc/src/lib.rs --lang rust --query run_main
	```

## python

Install the published package from PyPI:

```bash
pip install llmcc
```

With the package available, invoke the API directly:

```python
import llmcc

graph = llmcc.run(
	dirs=["crates/llmcc-core/src"],
	lang="rust",
	query="CompileCtxt",
	design_graph=True,
)
print(graph)
```
