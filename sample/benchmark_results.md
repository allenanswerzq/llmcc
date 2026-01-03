# LLMCC Benchmark Results

Generated on: 2026-01-03 04:38:08

## Machine Info

### CPU
- **Model:** AMD EPYC 7763 64-Core Processor
BIOS AMD EPYC 7763 64-Core Processor                 None CPU @ 2.4GHz
- **Cores:** 16 physical, 32 logical (threads)

### Memory
- **Total:** 125Gi
- **Available:** 100Gi

### OS
- **Kernel:** Linux 6.8.0-1040-azure
- **Distribution:** Microsoft Azure Linux 3.0


## PageRank Timing (depth=3, top-200)

| Project | Files | LoC | Parse | IR Build | Symbols | Binding | Graph | Link | Total |
|---------|-------|-----|-------|----------|---------|---------|-------|------|-------|
| databend | 3145 | 630K | 0.19s | 1.81s | 0.26s | 1.65s | 0.62s | 0.03s | 5.87s |
| risingwave | 2384 | 579K | 0.15s | 0.47s | 0.26s | 1.01s | 0.53s | 0.03s | 3.51s |
| datafusion | 983 | 502K | 0.21s | 0.64s | 0.23s | 0.98s | 0.34s | 0.03s | 3.07s |
| ruff | 1663 | 419K | 0.14s | 0.48s | 0.21s | 0.81s | 0.37s | 0.02s | 2.86s |
| lance | 443 | 248K | 0.09s | 0.20s | 0.10s | 0.27s | 0.18s | 0.01s | 1.13s |
| qdrant | 864 | 237K | 0.11s | 0.86s | 0.14s | 0.38s | 0.22s | 0.02s | 2.10s |
| codex | 618 | 224K | 0.07s | 0.16s | 0.11s | 0.38s | 0.15s | 0.01s | 1.11s |
| opendal | 715 | 94K | 0.04s | 0.11s | 0.05s | 0.19s | 0.11s | 0.01s | 0.67s |
| tokio | 456 | 92K | 0.03s | 0.13s | 0.03s | 0.10s | 0.07s | 0.00s | 0.45s |
| clap | 118 | 59K | 0.02s | 0.12s | 0.02s | 0.05s | 0.03s | 0.00s | 0.28s |
| ripgrep | 77 | 38K | 0.04s | 0.07s | 0.03s | 0.04s | 0.03s | 0.00s | 0.24s |
| serde | 58 | 33K | 0.02s | 0.07s | 0.02s | 0.05s | 0.02s | 0.00s | 0.22s |
| lancedb | 78 | 30K | 0.04s | 0.06s | 0.02s | 0.06s | 0.02s | 0.00s | 0.24s |
| axum | 109 | 29K | 0.02s | 0.03s | 0.01s | 0.05s | 0.02s | 0.00s | 0.15s |
| llmcc | 51 | 19K | 0.02s | 0.03s | 0.02s | 0.04s | 0.02s | 0.00s | 0.15s |

## Summary

Binary: /root/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 0 projects
- Medium (50-500 files): 8 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| databend | 7077 | 14799 | 156 | 227 | 100.0% | 100.0% |
| risingwave | 6635 | 13467 | 164 | 223 | 100.0% | 100.0% |
| datafusion | 4918 | 8442 | 150 | 245 | 100.0% | 100.0% |
| ruff | 7115 | 16665 | 175 | 294 | 100.0% | 100.0% |
| lance | 2220 | 3680 | 140 | 201 | 100.0% | 100.0% |
| qdrant | 3088 | 6697 | 150 | 222 | 100.0% | 100.0% |
| codex | 3192 | 5157 | 155 | 237 | 100.0% | 100.0% |
| opendal | 1391 | 1718 | 134 | 125 | 100.0% | 100.0% |
| tokio | 826 | 1117 | 155 | 194 | 90.0% | 90.0% |
| clap | 330 | 484 | 170 | 232 | 50.0% | 60.0% |
| ripgrep | 435 | 533 | 159 | 177 | 70.0% | 70.0% |
| serde | 312 | 601 | 166 | 284 | 50.0% | 60.0% |
| lancedb | 226 | 242 | 129 | 144 | 50.0% | 50.0% |
| axum | 231 | 282 | 139 | 165 | 40.0% | 50.0% |
| llmcc | 287 | 642 | 181 | 413 | 40.0% | 40.0% |

## Thread Scaling (databend, depth=3, top-200)

| Threads | Parse | IR Build | Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|----------|---------|---------|-------|------|-------|---------|
| 1 | 2.61s | 9.58s | 2.48s | 4.56s | 1.02s | 0.37s | 21.81s | - |
| 2 | 1.30s | 5.12s | 1.53s | 2.90s | 0.65s | 0.23s | 13.02s | 1x |
| 4 | 0.65s | 3.55s | 0.78s | 1.68s | 0.38s | 0.13s | 8.40s | 2x |
| 8 | 0.34s | 2.54s | 0.44s | 1.44s | 0.40s | 0.07s | 6.48s | 3x |
| 16 | 0.21s | 2.04s | 0.25s | 1.43s | 0.54s | 0.04s | 5.82s | 3x |
| 24 | 0.20s | 1.98s | 0.22s | 1.55s | 0.65s | 0.04s | 5.94s | 3x |
| 32 | 0.18s | 1.78s | 0.30s | 1.70s | 0.66s | 0.04s | 5.94s | 3x |
