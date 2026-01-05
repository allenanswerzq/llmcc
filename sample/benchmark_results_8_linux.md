# LLMCC Benchmark Results

Generated on: 2026-01-04 23:21:40

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30.3G
- **Available:** 26.3G

### Disk
- **Write Speed:** 1.1 GB/s

### OS
- **Kernel:** 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| databend | rust | 3130 | ~628K | 0.34s | 1.66s | 0.42s | 0.18s | 0.09s | 3.31s |
| risingwave | rust | 2382 | ~579K | 0.40s | 1.03s | 0.30s | 0.15s | 0.08s | 2.51s |
| ruff | rust | 1661 | ~422K | 0.27s | 1.19s | 0.38s | 0.20s | 0.08s | 2.54s |
| rust-analyzer | rust | 1362 | ~474K | 0.19s | 1.12s | 0.23s | 0.13s | 0.06s | 2.06s |
| datafusion | rust | 980 | ~499K | 0.25s | 0.82s | 0.36s | 0.11s | 0.05s | 1.92s |
| qdrant | rust | 864 | ~237K | 0.13s | 0.70s | 0.12s | 0.07s | 0.03s | 1.24s |
| codex | rust | 617 | ~225K | 0.08s | 0.19s | 0.06s | 0.03s | 0.01s | 0.53s |
| opendal | rust | 715 | ~94K | 0.04s | 0.23s | 0.05s | 0.04s | 0.02s | 0.48s |
| tokio | rust | 456 | ~92K | 0.04s | 0.16s | 0.04s | 0.02s | 0.01s | 0.33s |
| candle | rust | 382 | ~159K | 0.06s | 0.21s | 0.06s | 0.03s | 0.02s | 0.54s |
| lance | rust | 442 | ~246K | 0.11s | 0.25s | 0.07s | 0.04s | 0.02s | 0.67s |
| clap | rust | 118 | ~60K | 0.02s | 0.09s | 0.01s | 0.01s | - | 0.16s |
| axum | rust | 109 | ~29K | 0.02s | 0.03s | 0.01s | 0.01s | - | 0.09s |
| serde | rust | 58 | ~33K | 0.02s | 0.05s | 0.01s | 0.01s | - | 0.11s |
| ripgrep | rust | 77 | ~38K | 0.04s | 0.06s | 0.01s | 0.01s | - | 0.16s |
| lancedb | rust | 78 | ~30K | 0.03s | 0.05s | 0.01s | 0.01s | - | 0.15s |
| llmcc | rust | 45 | ~18K | 0.01s | 0.03s | 0.01s | 0.01s | - | 0.09s |

## Summary

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 8 projects
- Large (>500 files): 8 projects

## PageRank Graph Reduction (depth=3, top-200)

| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|----------|------------|------------|----------|----------|----------------|----------------|
| databend | rust | 7067 | 14717 | 158 | 218 | 97.8% | 98.5% |
| risingwave | rust | 6636 | 13442 | 161 | 226 | 97.6% | 98.3% |
| ruff | rust | 7101 | 16840 | 171 | 288 | 97.6% | 98.3% |
| rust-analyzer | rust | 5831 | 18557 | 180 | 457 | 96.9% | 97.5% |
| datafusion | rust | 4889 | 8510 | 153 | 257 | 96.9% | 97.0% |
| qdrant | rust | 3100 | 6718 | 150 | 223 | 95.2% | 96.7% |
| codex | rust | 3174 | 5150 | 156 | 242 | 95.1% | 95.3% |
| opendal | rust | 1394 | 1716 | 133 | 124 | 90.5% | 92.8% |
| tokio | rust | 834 | 1119 | 153 | 189 | 81.7% | 83.1% |
| candle | rust | 2208 | 4594 | 147 | 218 | 93.3% | 95.3% |
| lance | rust | 2218 | 3628 | 143 | 201 | 93.6% | 94.5% |
| clap | rust | 328 | 483 | 171 | 238 | 47.9% | 50.7% |
| axum | rust | 231 | 281 | 138 | 160 | 40.3% | 43.1% |
| serde | rust | 327 | 613 | 167 | 285 | 48.9% | 53.5% |
| ripgrep | rust | 442 | 531 | 158 | 179 | 64.3% | 66.3% |
| lancedb | rust | 246 | 257 | 136 | 152 | 44.7% | 40.9% |
| llmcc | rust | 255 | 562 | 186 | 442 | 27.1% | 21.4% |

## Thread Scaling (databend, depth=3, top-200, 8 cores)

| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|------------|---------|-------|------|-------|---------|
| 1 | 1.73s | 7.85s | 2.10s | 1.06s | 0.40s | 14.1s | - |
| 2 | 0.89s | 4.76s | 1.18s | 0.52s | 0.21s | 8.25s | 1.71x |
| 4 | 0.47s | 2.46s | 0.69s | 0.30s | 0.14s | 4.76s | 2.96x |
| 8 | 0.40s | 2.01s | 0.46s | 0.18s | 0.10s | 3.75s | 3.75x |
| 16 | 0.31s | 1.60s | 0.41s | 0.22s | 0.10s | 3.31s | 4.25x |
