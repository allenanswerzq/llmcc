# LLMCC Benchmark Results

Generated on: 2026-01-03 10:22:35

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
| databend | 3145 | 630K | 0.18s | 1.93s | 0.22s | 2.12s | 0.68s | 0.04s | 6.56s |
| risingwave | 2384 | 580K | 0.15s | 0.41s | 0.19s | 1.45s | 0.57s | 0.03s | 3.99s |
| datafusion | 983 | 504K | 0.17s | 0.62s | 0.22s | 1.06s | 0.36s | 0.04s | 3.18s |
| ruff | 1663 | 423K | 0.15s | 0.49s | 0.16s | 1.23s | 0.41s | 0.03s | 3.33s |
| lance | 443 | 248K | 0.10s | 0.17s | 0.07s | 0.30s | 0.19s | 0.01s | 1.08s |
| qdrant | 864 | 237K | 0.10s | 0.87s | 0.10s | 0.50s | 0.24s | 0.02s | 2.19s |
| codex | 618 | 226K | 0.07s | 0.15s | 0.06s | 0.26s | 0.15s | 0.01s | 0.94s |
| opendal | 715 | 94K | 0.04s | 0.13s | 0.03s | 0.22s | 0.11s | 0.01s | 0.68s |
| tokio | 456 | 92K | 0.03s | 0.14s | 0.03s | 0.09s | 0.07s | 0.00s | 0.46s |
| clap | 118 | 60K | 0.02s | 0.12s | 0.02s | 0.05s | 0.03s | 0.00s | 0.28s |
| ripgrep | 77 | 38K | 0.04s | 0.06s | 0.03s | 0.03s | 0.03s | 0.00s | 0.25s |
| serde | 58 | 33K | 0.02s | 0.06s | 0.02s | 0.04s | 0.02s | 0.00s | 0.20s |
| lancedb | 78 | 30K | 0.03s | 0.06s | 0.02s | 0.03s | 0.03s | 0.00s | 0.20s |
| axum | 109 | 29K | 0.02s | 0.03s | 0.01s | 0.02s | 0.02s | 0.00s | 0.12s |
| llmcc | 51 | 18K | 0.02s | 0.03s | 0.01s | 0.03s | 0.02s | 0.00s | 0.14s |

## Summary

Binary: /root/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 0 projects
- Medium (50-500 files): 8 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| databend | 7100 | 14865 | 156 | 217 | 100.0% | 100.0% |
| risingwave | 6636 | 13445 | 162 | 214 | 100.0% | 100.0% |
| datafusion | 4910 | 8520 | 149 | 244 | 100.0% | 100.0% |
| ruff | 7116 | 16817 | 176 | 312 | 100.0% | 100.0% |
| lance | 2216 | 3669 | 140 | 191 | 100.0% | 100.0% |
| qdrant | 3099 | 6673 | 152 | 211 | 100.0% | 100.0% |
| codex | 3192 | 5151 | 156 | 221 | 100.0% | 100.0% |
| opendal | 1397 | 1722 | 133 | 125 | 100.0% | 100.0% |
| tokio | 828 | 1111 | 159 | 200 | 90.0% | 90.0% |
| clap | 330 | 484 | 173 | 236 | 50.0% | 60.0% |
| ripgrep | 445 | 540 | 161 | 179 | 70.0% | 70.0% |
| serde | 326 | 613 | 167 | 285 | 50.0% | 60.0% |
| lancedb | 226 | 242 | 128 | 140 | 50.0% | 50.0% |
| axum | 230 | 282 | 141 | 161 | 40.0% | 50.0% |
| llmcc | 287 | 642 | 182 | 423 | 40.0% | 40.0% |

## Thread Scaling (databend, depth=3, top-200)

| Threads | Parse | IR Build | Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|----------|---------|---------|-------|------|-------|---------|
| 1 | 2.65s | 9.39s | 3.08s | 4.76s | 0.99s | 0.39s | 22.60s | - |
| 2 | 1.32s | 5.13s | 1.80s | 3.21s | 0.69s | 0.26s | 13.72s | 1.64x |
| 4 | 0.71s | 2.68s | 0.96s | 2.17s | 0.42s | 0.14s | 8.43s | 2.68x |
| 8 | 0.34s | 2.25s | 0.52s | 1.97s | 0.27s | 0.07s | 6.80s | 3.32x |
| 16 | 0.19s | 1.79s | 0.28s | 2.18s | 0.67s | 0.05s | 6.54s | 3.45x |
| 24 | 0.18s | 1.84s | 0.25s | 2.11s | 0.71s | 0.04s | 6.54s | 3.45x |
| 32 | 0.19s | 1.93s | 0.22s | 2.18s | 0.73s | 0.04s | 6.74s | 3.35x |
