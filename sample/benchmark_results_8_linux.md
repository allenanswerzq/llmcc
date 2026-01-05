# LLMCC Benchmark Results

Generated on: 2026-01-05 01:26:48

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30.3G
- **Available:** 26.3G

### Disk
- **Write Speed:** 1.2 GB/s

### OS
- **Kernel:** 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| databend | rust | 3130 | ~628K | 0.34s | 0.81s | 0.44s | 0.18s | 0.10s | 2.53s |
| risingwave | rust | 2382 | ~579K | 0.23s | 0.44s | 0.24s | 0.11s | 0.05s | 1.57s |
| ruff | rust | 1661 | ~422K | 0.28s | 0.58s | 0.29s | 0.16s | 0.06s | 1.73s |
| rust-analyzer | rust | 1362 | ~474K | 0.17s | 0.36s | 0.22s | 0.11s | 0.05s | 1.21s |
| datafusion | rust | 980 | ~499K | 0.24s | 0.56s | 0.37s | 0.15s | 0.06s | 1.71s |
| qdrant | rust | 864 | ~237K | 0.13s | 0.24s | 0.14s | 0.07s | 0.03s | 0.81s |
| codex | rust | 617 | ~225K | 0.09s | 0.10s | 0.07s | 0.04s | 0.02s | 0.46s |
| opendal | rust | 715 | ~94K | 0.05s | 0.09s | 0.04s | 0.02s | 0.01s | 0.32s |
| tokio | rust | 456 | ~92K | 0.05s | 0.07s | 0.03s | 0.02s | 0.01s | 0.24s |
| candle | rust | 382 | ~159K | 0.06s | 0.11s | 0.06s | 0.03s | 0.02s | 0.42s |
| lance | rust | 442 | ~246K | 0.11s | 0.16s | 0.10s | 0.04s | 0.02s | 0.63s |
| clap | rust | 118 | ~60K | 0.03s | 0.05s | 0.01s | 0.01s | - | 0.13s |
| axum | rust | 109 | ~29K | 0.02s | 0.02s | 0.01s | - | - | 0.07s |
| serde | rust | 58 | ~33K | 0.02s | 0.03s | 0.01s | 0.01s | - | 0.10s |
| ripgrep | rust | 77 | ~38K | 0.04s | 0.03s | 0.01s | 0.02s | 0.01s | 0.15s |
| lancedb | rust | 78 | ~30K | 0.03s | 0.03s | 0.02s | 0.01s | - | 0.11s |
| llmcc | rust | 45 | ~18K | 0.02s | 0.02s | 0.01s | 0.01s | - | 0.08s |

## Summary

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 8 projects
- Large (>500 files): 8 projects

## PageRank Graph Reduction (depth=3, top-200)

| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|----------|------------|------------|----------|----------|----------------|----------------|
| databend | rust | 7037 | 14686 | 157 | 216 | 97.8% | 98.5% |
| risingwave | rust | 6635 | 13430 | 161 | 226 | 97.6% | 98.3% |
| ruff | rust | 7093 | 16814 | 168 | 284 | 97.6% | 98.3% |
| rust-analyzer | rust | 5837 | 18713 | 180 | 455 | 96.9% | 97.6% |
| datafusion | rust | 4893 | 8447 | 148 | 257 | 97.0% | 97.0% |
| qdrant | rust | 3105 | 6752 | 151 | 222 | 95.1% | 96.7% |
| codex | rust | 3175 | 5153 | 153 | 229 | 95.2% | 95.6% |
| opendal | rust | 1391 | 1718 | 137 | 128 | 90.2% | 92.5% |
| tokio | rust | 833 | 1117 | 151 | 191 | 81.9% | 82.9% |
| candle | rust | 2205 | 4604 | 141 | 211 | 93.6% | 95.4% |
| lance | rust | 2210 | 3632 | 143 | 207 | 93.5% | 94.3% |
| clap | rust | 328 | 485 | 175 | 246 | 46.6% | 49.3% |
| axum | rust | 229 | 280 | 140 | 160 | 38.9% | 42.9% |
| serde | rust | 326 | 613 | 167 | 280 | 48.8% | 54.3% |
| ripgrep | rust | 443 | 531 | 160 | 183 | 63.9% | 65.5% |
| lancedb | rust | 225 | 242 | 139 | 157 | 38.2% | 35.1% |
| llmcc | rust | 255 | 562 | 187 | 447 | 26.7% | 20.5% |

## Thread Scaling (databend, depth=3, top-200, 8 cores)

| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|------------|---------|-------|------|-------|---------|
| 1 | 1.63s | 3.30s | 1.98s | 1.04s | 0.37s | 9.20s | - |
| 2 | 0.82s | 2.02s | 1.10s | 0.49s | 0.20s | 5.26s | 1.75x |
| 4 | 0.45s | 1.34s | 0.62s | 0.29s | 0.13s | 3.41s | 2.70x |
| 8 | 0.32s | 0.80s | 0.45s | 0.20s | 0.10s | 2.40s | 3.83x |
| 16 | 0.34s | 0.63s | 0.38s | 0.14s | 0.07s | 2.10s | 4.38x |
