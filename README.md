# llmcc

*"Prompts are the modern assembly language, models are the modern CPU."*

llmcc is a universal context builder for any language, any document.

## abstract

llmcc explores automated context generation through symbolic graph analysis. bridging the semantic gap between human-written code/documents and AI model understanding, using modern compiler design principles.

## design

![design](doc/design.svg)

## run

```bash
llmcc [OPTIONS] < --file <FILE>...|--dir <DIR>... >
```

**Input** (required, one of):
- `-f, --file <FILE>...` — Individual files to compile (repeatable)
- `-d, --dir <DIR>...` — Directories to scan recursively (repeatable)

**Language** (optional):
- `--lang <LANG>` — Language: 'rust' or 'python' [default: rust]

**Analysis** (optional):
- `--design-graph` — Generate high-level design graph
- `--pagerank --top-k <K>` — Rank by importance (PageRank) and limit to top K
- `--query <NAME>` — Symbol/function to analyze
- `--depends` — Show what the symbol depends on
- `--dependents` — Show what depends on the symbol
- `--recursive` — Include transitive dependencies (vs. direct only)

**Output format** (optional):
- `--summary` — Show file paths and line ranges (vs. full code texts)
- `--print-ir` — Internal: print intermediate representation
- `--print-block` — Internal: print basic block graph

**Examples:**
```bash
# Design graph with PageRank ranking
llmcc --dir crates/llmcc-core/src --lang rust --design-graph --pagerank --top-k 100

# Dependencies and dependents of a symbol
llmcc --dir crates/llmcc-core/src --lang rust --query CompileCtxt --depends
llmcc --dir crates/llmcc-core/src --lang rust --query CompileCtxt --dependents --recursive

# Cross-directory analysis
llmcc --dir crates/llmcc-core/src --dir crates/llmcc-rust/src --lang rust --design-graph --pagerank --top-k 25

# Multiple files
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
