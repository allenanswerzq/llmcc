# LLMCC Benchmark Results

Generated on: 2026-01-03 18:13:45

## Machine Info

### CPU
- **Model:** AMD EPYC 7763 64-Core Processor
BIOS AMD EPYC 7763 64-Core Processor                 None CPU @ 2.4GHz
- **Cores:** 16 physical, 32 logical (threads)

### Memory
- **Total:** 125Gi
- **Available:** 99Gi

### OS
- **Kernel:** Linux 6.8.0-1040-azure
- **Distribution:** Microsoft Azure Linux 3.0


## PageRank Timing (depth=3, top-200)

| Project | Files | LoC | Parse | IR Build | Symbols | Binding | Graph | Link | Total |
|---------|-------|-----|-------|----------|---------|---------|-------|------|-------|
| databend | 3145 | 630K | 0.30s | - | 2.06s | 2.10s | 0.12s | 0.04s | 5.32s |
| risingwave | 2384 | 580K | 0.18s | - | 0.70s | 1.36s | 0.09s | 0.03s | 2.99s |
| datafusion | 983 | 504K | 0.24s | - | 0.99s | 1.12s | 0.10s | 0.04s | 2.90s |
| ruff | 1663 | 423K | 0.23s | - | 1.11s | 1.28s | 0.11s | 0.04s | 3.23s |
| lance | 443 | 248K | 0.08s | - | 0.28s | 0.30s | 0.03s | 0.01s | 0.91s |
| qdrant | 864 | 237K | 0.12s | - | 0.83s | 0.43s | 0.06s | 0.02s | 1.69s |
| codex | 618 | 226K | 0.06s | - | 0.18s | 0.23s | 0.03s | 0.01s | 0.68s |
| opendal | 715 | 94K | 0.04s | - | 0.13s | 0.24s | 0.02s | 0.01s | 0.56s |
| tokio | 456 | 92K | 0.03s | - | 0.15s | 0.09s | 0.02s | 0.00s | 0.37s |
| clap | 118 | 60K | 0.02s | - | 0.15s | 0.05s | 0.02s | 0.00s | 0.28s |
| ripgrep | 77 | 38K | 0.04s | - | 0.09s | 0.03s | 0.02s | 0.00s | 0.22s |
| serde | 58 | 33K | 0.02s | - | 0.08s | 0.04s | 0.01s | 0.00s | 0.18s |
| lancedb | 78 | 30K | 0.03s | - | 0.08s | 0.03s | 0.01s | 0.00s | 0.19s |
| axum | 109 | 29K | 0.02s | - | 0.04s | 0.02s | 0.01s | 0.00s | 0.11s |
| llmcc | 51 | 18K | 0.02s | - | 0.04s | 0.03s | 0.01s | 0.00s | 0.12s |

## Summary

Binary: /root/llmcc/target/x86_64-unknown-linux-gnu/release/llmcc

### Project Sizes
- Small (<50 files): 0 projects
- Medium (50-500 files): 8 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| databend | 7089 | 14774 | 156 | 213 | 100.0% | 100.0% |
| risingwave | 6644 | 13463 | 161 | 221 | 100.0% | 100.0% |
| datafusion | 4909 | 8527 | 154 | 266 | 100.0% | 100.0% |
| ruff | 7118 | 16871 | 171 | 304 | 100.0% | 100.0% |
| lance | 2221 | 3634 | 136 | 195 | 100.0% | 100.0% |
| qdrant | 3095 | 6733 | 155 | 225 | 100.0% | 100.0% |
| codex | 3186 | 5157 | 156 | 240 | 100.0% | 100.0% |
| opendal | 1387 | 1722 | 134 | 125 | 100.0% | 100.0% |
| tokio | 833 | 1118 | 155 | 192 | 90.0% | 90.0% |
| clap | 330 | 484 | 173 | 234 | 50.0% | 60.0% |
| ripgrep | 443 | 536 | 162 | 177 | 70.0% | 70.0% |
| serde | 327 | 613 | 166 | 283 | 50.0% | 60.0% |
| lancedb | 247 | 259 | 140 | 155 | 50.0% | 50.0% |
| axum | 230 | 282 | 139 | 160 | 40.0% | 50.0% |
| llmcc | 287 | 643 | 184 | 423 | 40.0% | 40.0% |

## Thread Scaling (databend, depth=3, top-200)

| Threads | Parse | IR Build | Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|----------|---------|---------|-------|------|-------|---------|
| 1 | 2.63s | - | 11.68s | 4.25s | 1.07s | 0.40s | 21.32s | - |
| 2 | 1.37s | - | 7.64s | 3.14s | 0.72s | 0.27s | 14.14s | 1.50x |
| 4 | 0.69s | - | 3.65s | 1.99s | 0.37s | 0.14s | 7.70s | 2.76x |
| 8 | 0.49s | - | 2.49s | 1.86s | 0.20s | 0.08s | 5.89s | 3.61x |
| 16 | 0.34s | - | 2.18s | 2.03s | 0.18s | 0.07s | 5.51s | 3.86x |
| 24 | 0.41s | - | 2.08s | 2.07s | 0.15s | 0.06s | 5.47s | 3.89x |
| 32 | 0.29s | - | 1.99s | 2.11s | 0.12s | 0.04s | 5.26s | 4.05x |
