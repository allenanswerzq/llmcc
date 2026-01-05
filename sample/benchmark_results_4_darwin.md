# LLMCC Benchmark Results

Generated on: 2026-01-05 01:44:01

## Machine Info

### CPU
- **Model:** Intel(R) Core(TM) i5-1038NG7 CPU @ 2.00GHz
- **Cores:** 4 physical, 8 logical (threads)

### Memory
- **Total:** 16.0G
- **Available:** 2.4G

### Disk
- **Write Speed:** unknown

### OS
- **Kernel:** 19.6.0
- **Distribution:** Mac OS X 10.15.7

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| databend | rust | 3147 | ~631K | 1.52s | 2.15s | 1.49s | 0.48s | 0.19s | 7.80s |
| risingwave | rust | 2384 | ~580K | 1.20s | 1.87s | 1.10s | 0.43s | 0.21s | 6.53s |
| ruff | rust | 1663 | ~423K | 1.27s | 1.75s | 1.34s | 0.74s | 0.20s | 6.94s |
| rust-analyzer | rust | 1362 | ~474K | 0.99s | 1.89s | 1.20s | 0.33s | 0.13s | 5.68s |
| datafusion | rust | 984 | ~505K | 1.15s | 1.51s | 1.70s | 0.59s | 0.15s | 6.59s |
| qdrant | rust | 864 | ~237K | 0.58s | 0.90s | 0.54s | 0.17s | 0.07s | 2.90s |
| codex | rust | 619 | ~227K | 0.61s | 0.63s | 0.37s | 0.12s | 0.07s | 2.33s |
| opendal | rust | 715 | ~94K | 0.19s | 0.62s | 0.21s | 0.09s | 0.03s | 1.47s |
| tokio | rust | 456 | ~92K | 0.22s | 0.27s | 0.13s | 0.06s | 0.03s | 0.94s |
| candle | rust | 383 | ~159K | 0.30s | 0.68s | 0.40s | 0.14s | 0.05s | 2.01s |
| lance | rust | 447 | ~250K | 0.79s | 0.81s | 0.49s | 0.20s | 0.05s | 3.02s |
| clap | rust | 118 | ~60K | 0.06s | 0.10s | 0.06s | 0.02s | 0.01s | 0.31s |
| axum | rust | 109 | ~29K | 0.04s | 0.06s | 0.03s | 0.01s | - | 0.20s |
| serde | rust | 58 | ~33K | 0.05s | 0.09s | 0.05s | 0.02s | 0.01s | 0.27s |
| ripgrep | rust | 77 | ~38K | 0.08s | 0.14s | 0.06s | 0.04s | 0.02s | 0.44s |
| lancedb | rust | 78 | ~30K | 0.07s | 0.08s | 0.05s | 0.03s | 0.01s | 0.32s |
| llmcc | rust | 52 | ~19K | 0.03s | 0.05s | 0.04s | 0.01s | - | 0.18s |

## Summary

Binary: /Users/yibai/Monorepo/50-59_Codebase/50_Code/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 0 projects
- Medium (50-500 files): 9 projects
- Large (>500 files): 8 projects

## PageRank Graph Reduction (depth=3, top-200)

| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|----------|------------|------------|----------|----------|----------------|----------------|
| databend | rust | 7092 | 14706 | 156 | 214 | 97.8% | 98.5% |
| risingwave | rust | 6645 | 13472 | 161 | 225 | 97.6% | 98.3% |
| ruff | rust | 7114 | 16737 | 170 | 308 | 97.6% | 98.2% |
| rust-analyzer | rust | 5831 | 18696 | 179 | 464 | 96.9% | 97.5% |
| datafusion | rust | 4916 | 8425 | 155 | 256 | 96.8% | 97.0% |
| qdrant | rust | 3102 | 6719 | 148 | 218 | 95.2% | 96.8% |
| codex | rust | 3189 | 5169 | 157 | 237 | 95.1% | 95.4% |
| opendal | rust | 1393 | 1716 | 136 | 128 | 90.2% | 92.5% |
| tokio | rust | 832 | 1120 | 147 | 185 | 82.3% | 83.5% |
| candle | rust | 2212 | 4606 | 143 | 212 | 93.5% | 95.4% |
| lance | rust | 2229 | 3694 | 140 | 189 | 93.7% | 94.9% |
| clap | rust | 327 | 481 | 172 | 239 | 47.4% | 50.3% |
| axum | rust | 230 | 284 | 140 | 166 | 39.1% | 41.5% |
| serde | rust | 327 | 613 | 166 | 283 | 49.2% | 53.8% |
| ripgrep | rust | 441 | 534 | 157 | 177 | 64.4% | 66.9% |
| lancedb | rust | 225 | 241 | 139 | 153 | 38.2% | 36.5% |
| llmcc | rust | 293 | 639 | 185 | 414 | 36.9% | 35.2% |

## Thread Scaling (databend, depth=3, top-200, 4 cores)

| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|------------|---------|-------|------|-------|---------|
| 1 | 4.58s | 7.49s | 4.38s | 1.56s | 0.62s | 21.3s | - |
| 2 | 2.82s | 3.82s | 2.46s | 0.80s | 0.29s | 12.6s | 1.69x |
| 4 | 1.60s | 2.28s | 1.86s | 0.53s | 0.20s | 8.38s | 2.54x |
| 8 | 1.77s | 2.02s | 1.57s | 0.48s | 0.20s | 7.97s | 2.67x |
