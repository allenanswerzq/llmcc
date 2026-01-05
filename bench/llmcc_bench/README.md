# llmcc-bench

Cross-platform benchmarking and graph generation tools for llmcc.

## Features

- **Cross-platform**: Works on Windows, Linux, and macOS
- **Repository fetching**: Shallow clone sample Rust projects
- **Benchmarking**: Run performance benchmarks with timing breakdowns
- **Graph generation**: Generate architecture graphs at multiple depth levels
- **Thread scaling**: Measure parallel performance
- **Markdown reports**: Auto-generate comprehensive benchmark reports

## Installation

```bash
# From the llmcc project root
pip install -e ./llmcc_bench

# Or install directly
cd llmcc_bench && pip install -e .
```

## Usage

```bash
# Show help
python -m llmcc_bench --help

# Show system info and configuration
python -m llmcc_bench info

# Fetch sample repositories
python -m llmcc_bench fetch

# Run benchmarks
python -m llmcc_bench benchmark

# Generate architecture graphs
python -m llmcc_bench generate

# Clean up generated files
python -m llmcc_bench clean
```

## Commands

### `fetch`

Fetch sample Rust repositories for benchmarking.

```bash
# Fetch all repositories
python -m llmcc_bench fetch

# Fetch specific repos
python -m llmcc_bench fetch tokio axum

# Force re-clone
python -m llmcc_bench fetch --force

# List available repositories
python -m llmcc_bench fetch --list
```

### `benchmark`

Run performance benchmarks on sample projects.

```bash
# Benchmark all projects
python -m llmcc_bench benchmark

# Benchmark specific projects
python -m llmcc_bench benchmark databend risingwave

# Skip thread scaling benchmark
python -m llmcc_bench benchmark --skip-scaling

# Custom PageRank top-K
python -m llmcc_bench benchmark --top-k 100
```

### `generate`

Generate architecture graphs for sample projects.

```bash
# Generate all graphs
python -m llmcc_bench generate

# Generate for specific projects
python -m llmcc_bench generate tokio axum

# Also generate SVG files (requires Graphviz)
python -m llmcc_bench generate --svg
```

### `clean`

Clean up generated files.

```bash
# Remove project output directories
python -m llmcc_bench clean

# Also remove benchmark logs and results
python -m llmcc_bench clean --all

# Dry run (show what would be removed)
python -m llmcc_bench clean --dry-run
```

### `info`

Show system information and configuration.

```bash
python -m llmcc_bench info
```

## Sample Projects

The following Rust projects are available for benchmarking:

| Category | Projects |
|----------|----------|
| Core | ripgrep, tokio, serde, clap, axum, codex, llmcc, ruff |
| ML & AI | candle |
| Developer Tools | rust-analyzer |
| Database | lancedb, lance, opendal, risingwave, databend, datafusion, qdrant |

## Output

### Benchmark Results

Results are saved to `sample/benchmark_results_<cores>.md` with:

- Machine info (CPU, memory, OS)
- PageRank timing table (per-project breakdown)
- Project size summary
- Graph reduction statistics
- Thread scaling data

### Generated Graphs

Graphs are saved to `sample/<project>/` and `sample/<project>-pagerank/`:

- `depth_0_project.dot` - Project level
- `depth_1_crate.dot` - Crate level  
- `depth_2_module.dot` - Module level
- `depth_3_file.dot` - File level

## Requirements

- Python 3.9+
- Git (for fetching repositories)
- llmcc binary (build with `cargo build --release`)
- Graphviz (optional, for SVG generation)
