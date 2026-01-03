# LLMCC Benchmark Results

Generated on: 2026-01-03 10:50:06

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30Gi
- **Available:** 26Gi

### OS
- **Kernel:** Linux 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS


## PageRank Timing (depth=3, top-200)

| Project | Files | LoC | Parse | IR Build | Symbols | Binding | Graph | Link | Total |
|---------|-------|-----|-------|----------|---------|---------|-------|------|-------|
| databend | 3130 | 627K | 0.33s | - | 1.48s | 2.82s | 0.19s | 0.10s | 5.51s |
| risingwave | 2382 | 578K | 0.22s | - | 0.79s | 1.97s | 0.12s | 0.06s | 3.67s |
| datafusion | 980 | 498K | 0.26s | - | 1.01s | 1.41s | 0.19s | 0.07s | 3.39s |
| ruff | 1661 | 418K | 0.25s | - | 1.06s | 1.80s | 0.16s | 0.07s | 3.71s |
| lance | 442 | 246K | 0.12s | - | 0.29s | 0.36s | 0.04s | 0.02s | 1.05s |
| qdrant | 864 | 237K | 0.14s | - | 0.61s | 0.72s | 0.08s | 0.03s | 1.77s |
| codex | 617 | 224K | 0.08s | - | 0.20s | 0.36s | 0.04s | 0.02s | 0.86s |
| opendal | 715 | 94K | 0.04s | - | 0.14s | 0.35s | 0.02s | 0.01s | 0.67s |
| tokio | 456 | 92K | 0.04s | - | 0.15s | 0.11s | 0.02s | 0.01s | 0.39s |
| clap | 118 | 59K | 0.02s | - | 0.08s | 0.07s | 0.01s | 0.00s | 0.23s |
| ripgrep | 77 | 38K | 0.04s | - | 0.06s | 0.04s | 0.01s | 0.00s | 0.19s |
| serde | 58 | 33K | 0.02s | - | 0.05s | 0.05s | 0.01s | 0.00s | 0.17s |
| lancedb | 78 | 30K | 0.03s | - | 0.05s | 0.03s | 0.01s | 0.00s | 0.17s |
| axum | 109 | 29K | 0.02s | - | 0.03s | 0.02s | 0.01s | 0.00s | 0.10s |
| llmcc | 45 | 18K | 0.02s | - | 0.03s | 0.04s | 0.01s | 0.00s | 0.11s |

## Summary

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 7 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| databend | 7046 | 14676 | 157 | 214 | 100.0% | 100.0% |
| risingwave | 6636 | 13464 | 159 | 226 | 100.0% | 100.0% |
| datafusion | 4887 | 8509 | 154 | 260 | 100.0% | 100.0% |
| ruff | 7101 | 16899 | 172 | 291 | 100.0% | 100.0% |
| lance | 2217 | 3641 | 139 | 200 | 100.0% | 100.0% |
| qdrant | 3095 | 6689 | 143 | 216 | 100.0% | 100.0% |
| codex | 3177 | 5157 | 151 | 233 | 100.0% | 100.0% |
| opendal | 1393 | 1719 | 135 | 127 | 100.0% | 100.0% |
| tokio | 831 | 1117 | 150 | 192 | 90.0% | 90.0% |
| clap | 326 | 480 | 173 | 234 | 50.0% | 60.0% |
| ripgrep | 445 | 546 | 161 | 177 | 70.0% | 70.0% |
| serde | 327 | 615 | 167 | 286 | 50.0% | 60.0% |
| lancedb | 247 | 256 | 139 | 156 | 50.0% | 40.0% |
| axum | 230 | 281 | 136 | 159 | 50.0% | 50.0% |
| llmcc | 255 | 563 | 190 | 450 | 30.0% | 30.0% |

## Thread Scaling (databend, depth=3, top-200, 16 cores)

| Threads | Parse | IR Build | Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|----------|---------|---------|-------|------|-------|---------|
| 1 | 1.76s | - | 7.81s | 3.58s | 1.04s | 0.41s | 15.54s | - |
| 2 | 1.11s | - | 4.94s | 2.36s | 0.53s | 0.21s | 9.84s | 1.57x |
| 4 | 0.47s | - | 2.58s | 2.85s | 0.37s | 0.12s | 7.03s | 2.21x |
| 8 | 0.36s | - | 1.93s | 2.83s | 0.21s | 0.10s | 6.01s | 2.58x |
| 16 | 0.34s | - | 1.44s | 2.90s | 0.18s | 0.08s | 5.53s | 2.81x |
