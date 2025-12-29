# LLMCC Benchmark Results

Generated on: 2025-12-29 08:29:29

## PageRank Timing (depth=3, top-200)

| Project | Files | LoC | Parse | IR Build | Symbols | Binding | Graph | Link | Total |
|---------|-------|-----|-------|----------|---------|---------|-------|------|-------|
| databend | 3130 | 627K | 0.27s | 1.22s | 0.27s | 3.65s | 0.54s | 0.08s | 6.92s |
| risingwave | 2382 | 578K | 0.30s | 0.76s | 0.31s | 2.14s | 0.49s | 0.07s | 4.77s |
| datafusion | 980 | 498K | 0.24s | 0.79s | 0.26s | 1.73s | 0.31s | 0.06s | 3.81s |
| ruff | 1661 | 418K | 0.20s | 0.56s | 0.16s | 1.83s | 0.32s | 0.06s | 3.67s |
| lance | 442 | 246K | 0.12s | 0.32s | 0.08s | 0.41s | 0.15s | 0.03s | 1.28s |
| qdrant | 864 | 237K | 0.13s | 0.62s | 0.13s | 0.86s | 0.18s | 0.03s | 2.19s |
| codex | 617 | 224K | 0.09s | 0.20s | 0.06s | 0.38s | 0.12s | 0.02s | 1.01s |
| opendal | 715 | 94K | 0.06s | 0.13s | 0.05s | 0.41s | 0.10s | 0.02s | 0.89s |
| tokio | 456 | 92K | 0.04s | 0.15s | 0.04s | 0.16s | 0.07s | 0.01s | 0.56s |
| clap | 118 | 59K | 0.03s | 0.09s | 0.02s | 0.07s | 0.02s | 0.00s | 0.26s |
| ripgrep | 77 | 38K | 0.03s | 0.06s | 0.02s | 0.05s | 0.03s | 0.01s | 0.22s |
| serde | 58 | 33K | 0.02s | 0.05s | 0.01s | 0.06s | 0.02s | 0.00s | 0.18s |
| lancedb | 78 | 30K | 0.03s | 0.05s | 0.01s | 0.04s | 0.02s | 0.00s | 0.17s |
| axum | 109 | 29K | 0.02s | 0.03s | 0.01s | 0.02s | 0.02s | 0.00s | 0.12s |
| llmcc | 45 | 18K | 0.02s | 0.03s | 0.01s | 0.04s | 0.02s | 0.00s | 0.14s |

## Summary

Benchmarked on: Linux zhang 5.15.167.4-microsoft-standard-WSL2 #1 SMP Tue Nov 5 00:21:55 UTC 2024 x86_64 x86_64 x86_64 GNU/Linux

Binary: ../target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 7 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| databend | 7054 | 15255 | 155 | 227 | 100.0% | 100.0% |
| risingwave | 6625 | 13668 | 160 | 226 | 100.0% | 100.0% |
| datafusion | 4887 | 9006 | 156 | 265 | 100.0% | 100.0% |
| ruff | 7098 | 16834 | 170 | 294 | 100.0% | 100.0% |
| lance | 2211 | 3753 | 136 | 203 | 100.0% | 100.0% |
| qdrant | 3090 | 6806 | 149 | 229 | 100.0% | 100.0% |
| codex | 3180 | 5180 | 153 | 228 | 100.0% | 100.0% |
| opendal | 1391 | 1746 | 133 | 129 | 100.0% | 100.0% |
| tokio | 825 | 1126 | 150 | 190 | 90.0% | 90.0% |
| clap | 330 | 500 | 168 | 234 | 50.0% | 60.0% |
| ripgrep | 423 | 513 | 159 | 187 | 70.0% | 70.0% |
| serde | 327 | 614 | 166 | 283 | 50.0% | 60.0% |
| lancedb | 227 | 244 | 136 | 155 | 50.0% | 40.0% |
| axum | 230 | 284 | 139 | 162 | 40.0% | 50.0% |
| llmcc | 255 | 582 | 188 | 447 | 30.0% | 30.0% |
