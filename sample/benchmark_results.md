# LLMCC Benchmark Results

Generated on: 2025-12-31 15:37:35

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30Gi
- **Available:** 22Gi

### OS
- **Kernel:** Linux 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS


## PageRank Timing (depth=3, top-200)

| Project | Files | LoC | Parse | IR Build | Symbols | Binding | Graph | Link | Total |
|---------|-------|-----|-------|----------|---------|---------|-------|------|-------|
| databend | 3145 | 630K | 0.24s | 1.35s | 0.32s | 3.25s | 0.58s | 0.08s | 6.66s |
| risingwave | 2384 | 579K | 0.24s | 0.65s | 0.22s | 1.95s | 0.45s | 0.07s | 4.22s |
| datafusion | 981 | 502K | 0.20s | 0.63s | 0.21s | 1.40s | 0.28s | 0.05s | 3.16s |
| ruff | 1663 | 419K | 0.19s | 0.49s | 0.16s | 1.65s | 0.33s | 0.05s | 3.36s |
| lance | 443 | 247K | 0.14s | 0.28s | 0.06s | 0.42s | 0.13s | 0.02s | 1.19s |
| qdrant | 864 | 237K | 0.12s | 0.50s | 0.09s | 0.67s | 0.17s | 0.02s | 1.82s |
| codex | 617 | 224K | 0.09s | 0.25s | 0.09s | 0.35s | 0.12s | 0.03s | 1.09s |
| opendal | 715 | 94K | 0.06s | 0.13s | 0.06s | 0.30s | 0.10s | 0.02s | 0.77s |
| tokio | 456 | 92K | 0.04s | 0.14s | 0.06s | 0.12s | 0.06s | 0.01s | 0.48s |
| clap | 118 | 59K | 0.03s | 0.08s | 0.02s | 0.06s | 0.02s | 0.00s | 0.24s |
| ripgrep | 77 | 38K | 0.04s | 0.05s | 0.02s | 0.04s | 0.03s | 0.01s | 0.22s |
| serde | 58 | 33K | 0.02s | 0.06s | 0.01s | 0.05s | 0.02s | 0.00s | 0.19s |
| lancedb | 78 | 30K | 0.03s | 0.05s | 0.01s | 0.03s | 0.02s | 0.00s | 0.17s |
| axum | 109 | 29K | 0.02s | 0.03s | 0.01s | 0.02s | 0.02s | 0.00s | 0.11s |
| llmcc | 51 | 19K | 0.02s | 0.03s | 0.01s | 0.04s | 0.02s | 0.01s | 0.14s |

## Summary

Binary: /home/yibai/poly/llmcc-test/target/release/llmcc

### Project Sizes
- Small (<50 files): 0 projects
- Medium (50-500 files): 8 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| databend | 7108 | 14874 | 156 | 225 | 100.0% | 100.0% |
| risingwave | 6630 | 13401 | 164 | 226 | 100.0% | 100.0% |
| datafusion | 4901 | 8517 | 156 | 268 | 100.0% | 100.0% |
| ruff | 7115 | 16805 | 172 | 291 | 100.0% | 100.0% |
| lance | 2218 | 3652 | 135 | 188 | 100.0% | 100.0% |
| qdrant | 3092 | 6721 | 155 | 231 | 100.0% | 100.0% |
| codex | 3190 | 5150 | 157 | 234 | 100.0% | 100.0% |
| opendal | 1392 | 1719 | 134 | 132 | 100.0% | 100.0% |
| tokio | 827 | 1110 | 155 | 194 | 90.0% | 90.0% |
| clap | 329 | 482 | 169 | 233 | 50.0% | 60.0% |
| ripgrep | 438 | 531 | 159 | 181 | 70.0% | 70.0% |
| serde | 326 | 613 | 167 | 284 | 50.0% | 60.0% |
| lancedb | 224 | 242 | 137 | 153 | 40.0% | 40.0% |
| axum | 229 | 280 | 137 | 157 | 50.0% | 50.0% |
| llmcc | 287 | 643 | 180 | 417 | 40.0% | 40.0% |
