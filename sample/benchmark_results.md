# LLMCC Benchmark Results

Generated on: 2025-12-28 13:32:55

## Timing Breakdown (depth=3)

| Project | Files | Parse | IR Build | Symbols | Graph Build | Linking | Total |
|---------|-------|-------|----------|---------|-------------|---------|-------|
| serde | 58 | 0.19s | 0.17s | 0.09s | 2.20s | 0.01s | 3.08s |
| datafusion | 980 | 2.78s | 2.09s | 2.21s | 21.36s | 0.13s | 31.95s |
| codex | 617 | 1.12s | 0.88s | 0.40s | 3.73s | 0.06s | 7.00s |
| tokio | 456 | 0.57s | 0.51s | 0.25s | 1.66s | 0.03s | 3.40s |
| llmcc | 45 | 0.09s | 0.09s | 0.03s | 0.49s | 0.01s | 0.83s |
| opendal | 715 | 0.79s | 0.58s | 0.60s | 4.60s | 0.04s | 7.87s |
| lancedb | 78 | 0.19s | 0.14s | 0.05s | 0.39s | 0.01s | 1.01s |
| ripgrep | 77 | 0.20s | 0.16s | 0.05s | 0.48s | 0.01s | 1.12s |
| qdrant | 864 | 1.37s | 1.37s | 0.63s | 7.69s | 0.08s | 11.96s |
| clap | 118 | 0.20s | 0.19s | 0.07s | 0.69s | 0.01s | 1.34s |
| axum | 109 | 0.16s | 0.11s | 0.06s | 0.18s | 0.01s | 0.69s |
| risingwave | 2382 | 3.69s | 3.14s | 1.89s | 23.87s | 0.21s | 36.16s |
| databend | 3130 | 4.46s | 4.43s | 3.52s | 44.27s | 0.23s | 60.34s |

## PageRank Timing (depth=3, top-200)

| Project | Files | Parse | IR Build | Symbols | Graph Build | Linking | Total |
|---------|-------|-------|----------|---------|-------------|---------|-------|
| serde | 58 | 0.14s | 0.13s | 0.04s | 0.63s | 0.01s | 1.06s |
| datafusion | 980 | 2.53s | 2.18s | 0.97s | 13.91s | 0.12s | 21.09s |
| codex | 617 | 1.18s | 0.88s | 0.44s | 3.96s | 0.05s | 7.47s |
| tokio | 456 | 0.58s | 0.48s | 0.26s | 1.65s | 0.03s | 3.43s |
| llmcc | 45 | 0.09s | 0.10s | 0.03s | 0.49s | 0.01s | 0.85s |
| opendal | 715 | 0.82s | 0.57s | 0.65s | 4.57s | 0.04s | 7.96s |
| lancedb | 78 | 0.19s | 0.14s | 0.05s | 0.41s | 0.01s | 1.02s |
| ripgrep | 77 | 0.20s | 0.16s | 0.05s | 0.43s | 0.02s | 1.10s |
| qdrant | 864 | 1.37s | 1.43s | 0.64s | 8.17s | 0.08s | 12.56s |
| clap | 118 | 0.19s | 0.19s | 0.07s | 0.84s | 0.01s | 1.49s |
| axum | 109 | 0.16s | 0.12s | 0.06s | 0.26s | 0.01s | 0.78s |
| risingwave | 2382 | 3.62s | 3.24s | 1.78s | 25.56s | 0.21s | 38.04s |
| databend | 3130 | 4.55s | 4.05s | 3.38s | 36.49s | 0.25s | 52.18s |

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
| serde | 328 | 616 | 168 | 286 | 50.0% | 60.0% |
| datafusion | 4891 | 9010 | 149 | 267 | 100.0% | 100.0% |
| codex | 3190 | 5199 | 155 | 239 | 100.0% | 100.0% |
| tokio | 832 | 1134 | 152 | 194 | 90.0% | 90.0% |
| llmcc | 255 | 582 | 188 | 446 | 30.0% | 30.0% |
| opendal | 1341 | 1695 | 130 | 125 | 100.0% | 100.0% |
| lancedb | 246 | 263 | 130 | 146 | 50.0% | 50.0% |
| ripgrep | 443 | 547 | 160 | 179 | 70.0% | 70.0% |
| qdrant | 3095 | 6834 | 150 | 227 | 100.0% | 100.0% |
| clap | 330 | 499 | 172 | 241 | 50.0% | 60.0% |
| axum | 224 | 276 | 136 | 158 | 40.0% | 50.0% |
| risingwave | 6636 | 13735 | 160 | 221 | 100.0% | 100.0% |
| databend | 7075 | 15078 | 151 | 214 | 100.0% | 100.0% |
