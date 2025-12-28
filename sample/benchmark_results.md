# LLMCC Benchmark Results

Generated on: 2025-12-28 14:04:20

## Timing Breakdown (depth=3)

| Project | Files | Parse | IR Build | Symbols | Binding | Graph | Link | Total |
|---------|-------|-------|----------|---------|---------|-------|------|-------|

## PageRank Timing (depth=3, top-200)

| Project | Files | Parse | IR Build | Symbols | Binding | Graph | Link | Total |
|---------|-------|-------|----------|---------|---------|-------|------|-------|
| serde | 58 | 0.25s | 0.17s | 0.07s | 0.67s | 0.03s | 0.01s | 1.37s |
| datafusion | 980 | 4.19s | 2.23s | 1.11s | 12.94s | 0.41s | 0.11s | 22.66s |
| codex | 617 | 1.93s | 1.08s | 0.51s | 4.21s | 0.19s | 0.06s | 9.10s |
| tokio | 456 | 1.18s | 0.53s | 0.28s | 1.80s | 0.09s | 0.03s | 4.46s |
| llmcc | 45 | 0.17s | 0.11s | 0.05s | 0.45s | 0.02s | 0.01s | 1.01s |
| opendal | 715 | 1.71s | 0.66s | 0.73s | 4.87s | 0.14s | 0.05s | 10.11s |
| lancedb | 78 | 0.34s | 0.16s | 0.07s | 0.42s | 0.02s | 0.01s | 1.41s |
| ripgrep | 77 | 0.36s | 0.17s | 0.10s | 0.44s | 0.03s | 0.01s | 1.46s |
| qdrant | 864 | 2.75s | 1.57s | 0.77s | 8.55s | 0.28s | 0.07s | 15.02s |
| clap | 118 | 0.37s | 0.20s | 0.11s | 0.81s | 0.02s | 0.01s | 1.82s |
| axum | 109 | 0.34s | 0.14s | 0.10s | 0.29s | 0.02s | 0.01s | 1.14s |
| risingwave | 2382 | 6.97s | 3.39s | 1.95s | 23.65s | 0.70s | 0.21s | 41.59s |
| databend | 3130 | 4.75s | 4.32s | 3.24s | 37.23s | 0.89s | 0.25s | 54.18s |

## Summary

Benchmarked on: Linux zhang 5.15.167.4-microsoft-standard-WSL2 #1 SMP Tue Nov 5 00:21:55 UTC 2024 x86_64 x86_64 x86_64 GNU/Linux

Binary: ../target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 6 projects
- Large (>500 files): 6 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| serde | 328 | 616 | 166 | 284 | 50.0% | 60.0% |
| datafusion | 4891 | 9010 | 142 | 252 | 100.0% | 100.0% |
| codex | 3190 | 5199 | 147 | 240 | 100.0% | 100.0% |
| tokio | 832 | 1134 | 153 | 193 | 90.0% | 90.0% |
| llmcc | 255 | 582 | 168 | 430 | 40.0% | 30.0% |
| opendal | 1341 | 1695 | 127 | 126 | 100.0% | 100.0% |
| lancedb | 246 | 263 | 131 | 148 | 50.0% | 50.0% |
| ripgrep | 443 | 547 | 153 | 179 | 70.0% | 70.0% |
| qdrant | 3095 | 6834 | 151 | 220 | 100.0% | 100.0% |
| clap | 330 | 499 | 171 | 241 | 50.0% | 60.0% |
| axum | 224 | 276 | 137 | 160 | 40.0% | 50.0% |
| risingwave | 6636 | 13735 | 161 | 224 | 100.0% | 100.0% |
| databend | 7075 | 15078 | 152 | 210 | 100.0% | 100.0% |
