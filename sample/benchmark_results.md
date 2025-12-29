# LLMCC Benchmark Results

Generated on: 2025-12-29 19:12:22

## PageRank Timing (depth=3, top-200)

| Project | Files | LoC | Parse | IR Build | Symbols | Binding | Graph | Link | Total |
|---------|-------|-----|-------|----------|---------|---------|-------|------|-------|
| databend | 3130 | 627K | 0.30s | 1.37s | 0.33s | 2.84s | 0.49s | 0.08s | 6.22s |
| risingwave | 2382 | 578K | 0.31s | 0.86s | 0.29s | 1.91s | 0.40s | 0.05s | 4.43s |
| datafusion | 980 | 498K | 0.27s | 0.71s | 0.25s | 1.36s | 0.27s | 0.06s | 3.30s |
| ruff | 1661 | 418K | 0.19s | 0.55s | 0.16s | 1.77s | 0.30s | 0.05s | 3.49s |
| lance | 442 | 246K | 0.10s | 0.23s | 0.07s | 0.39s | 0.13s | 0.02s | 1.07s |
| qdrant | 864 | 237K | 0.13s | 0.51s | 0.11s | 0.64s | 0.15s | 0.02s | 1.77s |
| codex | 617 | 224K | 0.09s | 0.18s | 0.06s | 0.33s | 0.12s | 0.02s | 0.95s |
| opendal | 715 | 94K | 0.05s | 0.12s | 0.04s | 0.39s | 0.09s | 0.01s | 0.78s |
| tokio | 456 | 92K | 0.04s | 0.13s | 0.04s | 0.12s | 0.07s | 0.01s | 0.48s |
| clap | 118 | 59K | 0.02s | 0.09s | 0.02s | 0.06s | 0.03s | 0.00s | 0.25s |
| ripgrep | 77 | 38K | 0.04s | 0.06s | 0.02s | 0.04s | 0.03s | 0.00s | 0.22s |
| serde | 58 | 33K | 0.02s | 0.05s | 0.01s | 0.06s | 0.02s | 0.00s | 0.18s |
| lancedb | 78 | 30K | 0.03s | 0.05s | 0.01s | 0.04s | 0.02s | 0.00s | 0.17s |
| axum | 109 | 29K | 0.02s | 0.03s | 0.01s | 0.02s | 0.02s | 0.00s | 0.11s |
| llmcc | 45 | 18K | 0.01s | 0.03s | 0.01s | 0.03s | 0.01s | 0.00s | 0.11s |

## Summary

Benchmarked on: Linux zhang 5.15.167.4-microsoft-standard-WSL2 #1 SMP Tue Nov 5 00:21:55 UTC 2024 x86_64 x86_64 x86_64 GNU/Linux

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 7 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| databend | 7050 | 15139 | 154 | 226 | 100.0% | 100.0% |
| risingwave | 6626 | 13660 | 165 | 227 | 100.0% | 100.0% |
| datafusion | 4887 | 9001 | 152 | 252 | 100.0% | 100.0% |
| ruff | 7097 | 16930 | 171 | 310 | 100.0% | 100.0% |
| lance | 2212 | 3776 | 138 | 213 | 100.0% | 100.0% |
| qdrant | 3105 | 6873 | 155 | 229 | 100.0% | 100.0% |
| codex | 3193 | 5192 | 155 | 239 | 100.0% | 100.0% |
| opendal | 1392 | 1749 | 132 | 129 | 100.0% | 100.0% |
| tokio | 831 | 1135 | 151 | 193 | 90.0% | 90.0% |
| clap | 329 | 500 | 172 | 246 | 50.0% | 60.0% |
| ripgrep | 445 | 555 | 161 | 183 | 70.0% | 70.0% |
| serde | 326 | 614 | 165 | 284 | 50.0% | 60.0% |
| lancedb | 226 | 244 | 135 | 153 | 50.0% | 40.0% |
| axum | 229 | 282 | 138 | 162 | 40.0% | 50.0% |
| llmcc | 252 | 575 | 191 | 461 | 30.0% | 20.0% |
