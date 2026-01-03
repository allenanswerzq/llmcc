# LLMCC Benchmark Results

Generated on: 2026-01-03 11:25:37

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
| databend | 3145 | 630K | 0.18s | 1.93s | 0.21s | 1.96s | 0.09s | 0.03s | 5.15s |
| risingwave | 2384 | 580K | 0.16s | 0.44s | 0.19s | 1.36s | 0.08s | 0.03s | 2.87s |
| datafusion | 983 | 504K | 0.16s | 0.60s | 0.23s | 1.06s | 0.10s | 0.04s | 2.60s |
| ruff | 1663 | 423K | 0.16s | 0.47s | 0.17s | 1.21s | 0.08s | 0.03s | 2.61s |
| lance | 443 | 248K | 0.09s | 0.17s | 0.06s | 0.29s | 0.03s | 0.01s | 0.87s |
| qdrant | 864 | 237K | 0.11s | 0.87s | 0.10s | 0.50s | 0.05s | 0.02s | 1.90s |
| codex | 618 | 226K | 0.07s | 0.15s | 0.05s | 0.25s | 0.03s | 0.01s | 0.74s |
| opendal | 715 | 94K | 0.04s | 0.10s | 0.03s | 0.26s | 0.02s | 0.01s | 0.58s |
| tokio | 456 | 92K | 0.03s | 0.13s | 0.03s | 0.09s | 0.01s | 0.00s | 0.38s |
| clap | 118 | 60K | 0.03s | 0.13s | 0.02s | 0.05s | 0.01s | 0.00s | 0.27s |
| ripgrep | 77 | 38K | 0.04s | 0.06s | 0.02s | 0.04s | 0.01s | 0.00s | 0.22s |
| serde | 58 | 33K | 0.02s | 0.08s | 0.02s | 0.05s | 0.01s | 0.00s | 0.21s |
| lancedb | 78 | 30K | 0.04s | 0.06s | 0.02s | 0.03s | 0.01s | 0.00s | 0.20s |
| axum | 109 | 29K | 0.02s | 0.03s | 0.01s | 0.02s | 0.01s | 0.00s | 0.10s |
| llmcc | 51 | 18K | 0.02s | 0.04s | 0.01s | 0.03s | 0.01s | 0.00s | 0.13s |

## Summary

Binary: /root/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 0 projects
- Medium (50-500 files): 8 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| databend | 7100 | 14925 | 154 | 219 | 100.0% | 100.0% |
| risingwave | 6644 | 13470 | 166 | 231 | 100.0% | 100.0% |
| datafusion | 4920 | 8519 | 150 | 256 | 100.0% | 100.0% |
| ruff | 7112 | 16679 | 173 | 311 | 100.0% | 100.0% |
| lance | 2218 | 3655 | 139 | 201 | 100.0% | 100.0% |
| qdrant | 3097 | 6700 | 155 | 247 | 100.0% | 100.0% |
| codex | 3192 | 5159 | 154 | 228 | 100.0% | 100.0% |
| opendal | 1394 | 1724 | 135 | 129 | 100.0% | 100.0% |
| tokio | 831 | 1118 | 152 | 189 | 90.0% | 90.0% |
| clap | 328 | 483 | 171 | 238 | 50.0% | 60.0% |
| ripgrep | 439 | 534 | 162 | 183 | 70.0% | 70.0% |
| serde | 326 | 613 | 167 | 284 | 50.0% | 60.0% |
| lancedb | 226 | 242 | 136 | 152 | 40.0% | 40.0% |
| axum | 231 | 279 | 140 | 159 | 40.0% | 50.0% |
| llmcc | 287 | 642 | 183 | 422 | 40.0% | 40.0% |

## Thread Scaling (databend, depth=3, top-200)

| Threads | Parse | IR Build | Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|----------|---------|---------|-------|------|-------|---------|
| 1 | 2.76s | 9.31s | 2.73s | 4.33s | 0.93s | 0.37s | 21.74s | - |
| 2 | 1.32s | 5.10s | 1.84s | 2.95s | 0.68s | 0.25s | 13.12s | 1.65x |
| 4 | 0.66s | 2.66s | 0.92s | 1.95s | 0.27s | 0.12s | 7.39s | 2.94x |
| 8 | 0.34s | 1.85s | 0.47s | 1.89s | 0.16s | 0.07s | 5.54s | 3.92x |
| 16 | 0.20s | 1.69s | 0.29s | 2.14s | 0.12s | 0.04s | 5.19s | 4.18x |
| 24 | 0.20s | 2.01s | 0.23s | 2.10s | 0.10s | 0.04s | 5.42s | 4.01x |
| 32 | 0.17s | 1.92s | 0.22s | 2.09s | 0.10s | 0.03s | 5.24s | 4.14x |
