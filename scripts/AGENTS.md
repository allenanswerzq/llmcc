# Agent Instructions for llmcc

## Available Tool: llmcc

You have access to a powerful code analysis tool called `llmcc` that generates multi-depth architecture graphs for understanding codebases.

### Location
The `llmcc` binary is in your PATH. Just use `llmcc` directly (do NOT use `~/.cargo/bin/llmcc` as tilde doesn't expand in quotes).

### Usage
```bash
llmcc -d <directory> --graph --depth <0-3> [options]
```

### Options
| Option | Description |
|--------|-------------|
| `-d <dir>` | Directory to scan recursively (can specify multiple: `-d dir1 -d dir2`) |
| `-f <file>` | Individual file to analyze (can specify multiple: `-f file1 -f file2`) |
| `--lang <rust\|typescript\|ts>` | Language (default: rust) |
| `--graph` | Generate DOT graph output |
| `--depth <0-3>` | Granularity: 0=project, 1=crate, 2=module, 3=file+symbol |
| `--pagerank-top-k <N>` | Show only top N nodes by importance |
| `--cluster-by-crate` | Group modules by parent crate |
| `--short-labels` | Use shortened labels |
| `-o <file>` | Write output to file |

### When to Use llmcc
- **Understanding unfamiliar codebases** - Run at depth 1-2 to see high-level architecture
- **Before making changes** - Analyze at depth 3 to understand dependencies
- **Identifying important code** - Use `--pagerank-top-k` to find central components

### Examples

**High-level crate overview:**
```bash
llmcc -d /path/to/project --graph --depth 1
```

**Module-level with top 50 important nodes:**
```bash
llmcc -d /path/to/project --graph --depth 2 --pagerank-top-k 50
```

**File-level analysis of specific crate:**
```bash
llmcc -d /path/to/project/crates/core --graph --depth 3 --pagerank-top-k 100
```

**TypeScript project:**
```bash
llmcc -d /path/to/ts/project --lang typescript --graph --depth 3
```

### Output Format
The output is in DOT format
```bash
llmcc -d /project --graph --depth 2 -o /tmp/graph.dot
```

## Guidelines
- Use llmcc when you need to understand project structure before making changes
- Start with lower depth (1-2) for overview, then drill down to depth 3 for details
- Use `--pagerank-top-k` for large projects to focus on important components
- Do not worry about generate SVG graphs, those are for human, NOT for you, you need to utlize this tool to understand codebases
