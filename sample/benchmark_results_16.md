# LLMCC Benchmark Results

Generated on: 2026-01-03 12:06:00

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30Gi
- **Available:** 25Gi

### OS
- **Kernel:** Linux 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS


## PageRank Timing (depth=3, top-200)

| Project | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|-------|-----|-------|------------|---------|-------|------|-------|
| databend | 3130 | 627K | 0.31s | 1.45s | 0.39s | 0.18s | 0.09s | 3.03s |
| risingwave | 2382 | 578K | 0.25s | 0.77s | 0.25s | 0.12s | 0.05s | 1.93s |
| datafusion | 980 | 498K | 0.29s | 0.99s | 0.39s | 0.11s | 0.05s | 2.17s |
| ruff | 1661 | 418K | 0.23s | 1.12s | 0.26s | 0.15s | 0.07s | 2.23s |
| rust-analyzer | 1362 | 392K | 0.20s | 1.12s | 0.21s | 0.12s | 0.06s | 2.03s |
| lance | 442 | 246K | 0.10s | 0.24s | 0.08s | 0.04s | 0.02s | 0.68s |
| qdrant | 864 | 237K | 0.12s | 0.62s | 0.12s | 0.08s | 0.03s | 1.17s |
| codex | 617 | 224K | 0.08s | 0.21s | 0.07s | 0.05s | 0.02s | 0.60s |
| candle | 382 | 159K | 0.07s | 0.23s | 0.06s | 0.03s | 0.02s | 0.56s |
| opendal | 715 | 94K | 0.04s | 0.16s | 0.04s | 0.02s | 0.01s | 0.38s |
| tokio | 456 | 92K | 0.04s | 0.14s | 0.03s | 0.02s | 0.01s | 0.31s |
| clap | 118 | 59K | 0.02s | 0.09s | 0.01s | 0.01s | 0.00s | 0.17s |
| ripgrep | 77 | 38K | 0.03s | 0.06s | 0.01s | 0.01s | 0.00s | 0.16s |
| serde | 58 | 33K | 0.02s | 0.05s | 0.01s | 0.01s | 0.00s | 0.13s |
| lancedb | 78 | 30K | 0.03s | 0.05s | 0.01s | 0.01s | 0.00s | 0.14s |
| axum | 109 | 29K | 0.02s | 0.03s | 0.01s | 0.01s | 0.00s | 0.09s |
| llmcc | 45 | 18K | 0.02s | 0.03s | 0.01s | 0.01s | 0.00s | 0.08s |

## Summary

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 8 projects
- Large (>500 files): 8 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| databend | 7059 | 14744 | 155 | 217 | 100.0% | 100.0% |
| risingwave | 6638 | 13426 | 161 | 224 | 100.0% | 100.0% |
| datafusion | 4889 | 8493 | 151 | 259 | 100.0% | 100.0% |
| ruff | 7104 | 16871 | 171 | 286 | 100.0% | 100.0% |
| rust-analyzer | 5826 | 18610 | 179 | 474 | 100.0% | 100.0% |
| lance | 2220 | 3646 | 137 | 191 | 100.0% | 100.0% |
| qdrant | 3101 | 6747 | 153 | 221 | 100.0% | 100.0% |
| codex | 3178 | 5152 | 150 | 228 | 100.0% | 100.0% |
| candle | 2203 | 4600 | 144 | 209 | 100.0% | 100.0% |
| opendal | 1398 | 1720 | 133 | 126 | 100.0% | 100.0% |
| tokio | 837 | 1121 | 152 | 192 | 90.0% | 90.0% |
| clap | 330 | 483 | 170 | 238 | 50.0% | 60.0% |
| ripgrep | 441 | 538 | 163 | 184 | 70.0% | 70.0% |
| serde | 328 | 615 | 168 | 286 | 50.0% | 60.0% |
| lancedb | 247 | 256 | 137 | 152 | 50.0% | 50.0% |
| axum | 230 | 282 | 140 | 159 | 40.0% | 50.0% |
| llmcc | 255 | 562 | 185 | 438 | 30.0% | 30.0% |

## Thread Scaling (databend, depth=3, top-200, 16 cores)

| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|------------|---------|-------|------|-------|---------|
| 1 | 1.65s | 7.93s | 2.18s | 1.02s | 0.36s | 14.02s | - |
| 2 | 0.90s | 4.71s | 1.25s | 0.57s | 0.21s | 8.33s | 1.68x |
| 4 | 0.49s | 2.39s | 0.64s | 0.42s | 0.14s | 4.73s | 2.96x |
| 8 | 0.42s | 1.92s | 0.48s | 0.22s | 0.10s | 3.76s | 3.72x |
| 16 | 0.47s | 1.60s | 0.43s | 0.21s | 0.10s | 3.42s | 4.09x |
