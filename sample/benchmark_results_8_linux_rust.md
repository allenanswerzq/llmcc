# LLMCC Benchmark Results

Generated on: 2026-01-10 11:08:02

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30.3G
- **Available:** 25.3G

### Disk
- **Write Speed:** 864 MB/s

### OS
- **Kernel:** 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| substrate | rust | 3805 | ~1100K | 2.54s | 1.42s | 0.61s | 0.36s | 0.24s | 6.62s |
| aptos-core | rust | 2681 | ~848K | 0.98s | 1.10s | 0.55s | 0.29s | 0.22s | 3.92s |
| sui | rust | 2612 | ~809K | 0.89s | 0.98s | 0.53s | 0.37s | 0.27s | 3.99s |
| databend | rust | 3151 | ~633K | 0.71s | 0.84s | 0.58s | 0.39s | 0.28s | 3.75s |
| risingwave | rust | 2389 | ~582K | 0.47s | 0.67s | 0.27s | 0.16s | 0.12s | 2.32s |
| datafusion | rust | 986 | ~505K | 0.42s | 0.71s | 0.61s | 0.20s | 0.16s | 2.59s |
| solana | rust | 1101 | ~498K | 0.46s | 0.38s | 0.19s | 0.12s | 0.08s | 1.64s |
| rust-analyzer | rust | 1362 | ~474K | 0.28s | 0.44s | 0.25s | 0.15s | 0.11s | 1.64s |
| ruff | rust | 1664 | ~424K | 0.47s | 0.73s | 0.38s | 0.24s | 0.14s | 2.44s |
| reth | rust | 1184 | ~279K | 0.32s | 0.29s | 0.17s | 0.10s | 0.05s | 1.27s |
| lance | rust | 448 | ~251K | 0.14s | 0.17s | 0.11s | 0.05s | 0.05s | 0.76s |
| lighthouse | rust | 702 | ~251K | 0.17s | 0.20s | 0.10s | 0.05s | 0.04s | 0.78s |
| qdrant | rust | 864 | ~237K | 0.18s | 0.27s | 0.15s | 0.09s | 0.06s | 1.02s |
| codex | rust | 624 | ~230K | 0.14s | 0.13s | 0.08s | 0.05s | 0.03s | 0.61s |
| cairo | rust | 630 | ~184K | 0.27s | 0.55s | 0.32s | 0.21s | 0.16s | 1.81s |
| snarkvm | rust | 1350 | ~183K | 0.32s | 0.20s | 0.09s | 0.05s | 0.06s | 0.96s |
| foundry | rust | 454 | ~161K | 0.10s | 0.11s | 0.06s | 0.03s | 0.02s | 0.47s |
| candle | rust | 383 | ~159K | 0.08s | 0.12s | 0.07s | 0.04s | 0.03s | 0.52s |
| opendal | rust | 715 | ~94K | 0.14s | 0.08s | 0.04s | 0.03s | 0.02s | 0.44s |
| tokio | rust | 456 | ~92K | 0.05s | 0.07s | 0.03s | 0.02s | 0.01s | 0.29s |
| starknet-foundry | rust | 358 | ~77K | 0.04s | 0.03s | 0.02s | 0.01s | 0.01s | 0.17s |
| alloy | rust | 335 | ~69K | 0.07s | 0.06s | 0.03s | 0.02s | 0.01s | 0.29s |
| clap | rust | 118 | ~60K | 0.03s | 0.05s | 0.01s | 0.01s | 0.01s | 0.16s |
| revm | rust | 227 | ~41K | 0.04s | 0.04s | 0.02s | 0.02s | 0.01s | 0.18s |
| ripgrep | rust | 77 | ~38K | 0.04s | 0.04s | 0.02s | 0.01s | 0.01s | 0.19s |
| serde | rust | 58 | ~33K | 0.02s | 0.03s | 0.02s | 0.01s | 0.01s | 0.13s |
| lancedb | rust | 78 | ~30K | 0.03s | 0.03s | 0.02s | 0.01s | 0.01s | 0.16s |
| axum | rust | 109 | ~29K | 0.02s | 0.02s | 0.01s | 0.01s | 0.00s | 0.11s |
| llmcc | rust | 52 | ~19K | 0.02s | 0.02s | 0.01s | 0.01s | 0.00s | 0.11s |

## Summary

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 0 projects
- Medium (50-500 files): 13 projects
- Large (>500 files): 16 projects

## PageRank Graph Reduction (depth=3, top-200)

| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|----------|------------|------------|----------|----------|----------------|----------------|
| substrate | rust | 13337 | 25738 | 115 | 140 | 99.1% | 99.5% |
| aptos-core | rust | 8869 | 18275 | 140 | 203 | 98.4% | 98.9% |
| sui | rust | 12118 | 31999 | 153 | 295 | 98.7% | 99.1% |
| databend | rust | 6994 | 14959 | 152 | 252 | 97.8% | 98.3% |
| risingwave | rust | 6410 | 12978 | 150 | 230 | 97.7% | 98.2% |
| datafusion | rust | 4687 | 8900 | 151 | 301 | 96.8% | 96.6% |
| solana | rust | 4383 | 7231 | 124 | 119 | 97.2% | 98.4% |
| rust-analyzer | rust | 5869 | 20090 | 174 | 502 | 97.0% | 97.5% |
| ruff | rust | 7129 | 17338 | 168 | 298 | 97.6% | 98.3% |
| reth | rust | 2714 | 4719 | 144 | 196 | 94.7% | 95.8% |
| lance | rust | 2052 | 3475 | 127 | 196 | 93.8% | 94.4% |
| lighthouse | rust | 2306 | 5415 | 164 | 357 | 92.9% | 93.4% |
| qdrant | rust | 3031 | 6304 | 126 | 169 | 95.8% | 97.3% |
| codex | rust | 2885 | 4293 | 147 | 227 | 94.9% | 94.7% |
| cairo | rust | 4393 | 10368 | 155 | 320 | 96.5% | 96.9% |
| snarkvm | rust | 999 | 2590 | 183 | 593 | 81.7% | 77.1% |
| foundry | rust | 1287 | 1767 | 150 | 174 | 88.3% | 90.2% |
| candle | rust | 2172 | 4270 | 125 | 149 | 94.2% | 96.5% |
| opendal | rust | 1438 | 1757 | 132 | 123 | 90.8% | 93.0% |
| tokio | rust | 817 | 1189 | 147 | 177 | 82.0% | 85.1% |
| starknet-foundry | rust | 825 | 1375 | 163 | 204 | 80.2% | 85.2% |
| alloy | rust | 789 | 1095 | 155 | 248 | 80.4% | 77.4% |
| clap | rust | 319 | 495 | 176 | 270 | 44.8% | 45.5% |
| revm | rust | 496 | 928 | 176 | 234 | 64.5% | 74.8% |
| ripgrep | rust | 433 | 547 | 159 | 193 | 63.3% | 64.7% |
| serde | rust | 322 | 555 | 169 | 254 | 47.5% | 54.2% |
| lancedb | rust | 249 | 297 | 155 | 192 | 37.8% | 35.4% |
| axum | rust | 230 | 287 | 141 | 184 | 38.7% | 35.9% |
| llmcc | rust | 280 | 531 | 187 | 310 | 33.2% | 41.6% |

## Thread Scaling (databend, depth=3, top-200, 8 cores)

| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|------------|---------|-------|------|-------|---------|
| 1 | 2.43s | 4.32s | 2.38s | 1.48s | 1.02s | 12.8s | - |
| 2 | 1.46s | 2.24s | 1.32s | 0.81s | 0.53s | 7.33s | 1.74x |
| 4 | 1.08s | 1.55s | 0.85s | 0.50s | 0.34s | 5.22s | 2.44x |
| 8 | 0.79s | 1.22s | 0.55s | 0.35s | 0.24s | 4.03s | 3.16x |
| 16 | 0.78s | 0.91s | 0.61s | 0.35s | 0.23s | 3.70s | 3.45x |
