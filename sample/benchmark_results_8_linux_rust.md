# LLMCC Benchmark Results

Generated on: 2026-01-06 00:36:09

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30.3G
- **Available:** 18.8G

### Disk
- **Write Speed:** 724 MB/s

### OS
- **Kernel:** 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| databend | rust | 3151 | ~633K | 0.64s | 0.75s | 0.56s | 0.23s | 0.12s | 3.10s |
| risingwave | rust | 2389 | ~582K | 0.42s | 0.54s | 0.32s | 0.15s | 0.09s | 2.13s |
| datafusion | rust | 986 | ~505K | 0.36s | 0.48s | 0.52s | 0.18s | 0.10s | 2.04s |
| rust-analyzer | rust | 1362 | ~474K | 0.28s | 0.51s | 0.32s | 0.15s | 0.09s | 1.76s |
| ruff | rust | 1664 | ~424K | 0.37s | 0.62s | 0.31s | 0.18s | 0.09s | 1.99s |
| lance | rust | 448 | ~251K | 0.14s | 0.15s | 0.11s | 0.04s | 0.02s | 0.68s |
| qdrant | rust | 864 | ~237K | 0.19s | 0.28s | 0.18s | 0.08s | 0.03s | 0.97s |
| codex | rust | 624 | ~230K | 0.15s | 0.15s | 0.07s | 0.05s | 0.03s | 0.64s |
| candle | rust | 383 | ~159K | 0.09s | 0.14s | 0.08s | 0.04s | 0.02s | 0.54s |
| opendal | rust | 715 | ~94K | 0.13s | 0.08s | 0.04s | 0.03s | 0.02s | 0.44s |
| tokio | rust | 456 | ~92K | 0.06s | 0.09s | 0.03s | 0.02s | 0.01s | 0.29s |
| clap | rust | 118 | ~60K | 0.03s | 0.04s | 0.02s | 0.01s | 0.00s | 0.15s |
| ripgrep | rust | 77 | ~38K | 0.04s | 0.04s | 0.01s | 0.01s | 0.01s | 0.16s |
| serde | rust | 58 | ~33K | 0.03s | 0.04s | 0.02s | 0.01s | 0.01s | 0.13s |
| lancedb | rust | 78 | ~30K | 0.04s | 0.03s | 0.02s | 0.01s | 0.01s | 0.14s |
| axum | rust | 109 | ~29K | 0.03s | 0.02s | 0.01s | 0.01s | 0.00s | 0.10s |
| llmcc | rust | 52 | ~19K | 0.02s | 0.02s | 0.01s | 0.01s | 0.00s | 0.10s |

## Summary

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 0 projects
- Medium (50-500 files): 9 projects
- Large (>500 files): 8 projects

## PageRank Graph Reduction (depth=3, top-200)

| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|----------|------------|------------|----------|----------|----------------|----------------|
| databend | rust | 7136 | 15561 | 151 | 214 | 97.9% | 98.6% |
| risingwave | rust | 6675 | 13807 | 157 | 230 | 97.6% | 98.3% |
| datafusion | rust | 4938 | 9048 | 154 | 280 | 96.9% | 96.9% |
| rust-analyzer | rust | 5879 | 19205 | 179 | 495 | 97.0% | 97.4% |
| ruff | rust | 7136 | 17360 | 173 | 317 | 97.6% | 98.2% |
| lance | rust | 2259 | 3972 | 133 | 221 | 94.1% | 94.4% |
| qdrant | rust | 3105 | 6888 | 144 | 201 | 95.4% | 97.1% |
| codex | rust | 3252 | 5287 | 154 | 243 | 95.3% | 95.4% |
| candle | rust | 2215 | 4657 | 144 | 207 | 93.5% | 95.6% |
| opendal | rust | 1394 | 1730 | 137 | 130 | 90.2% | 92.5% |
| tokio | rust | 836 | 1163 | 157 | 201 | 81.2% | 82.7% |
| clap | rust | 329 | 495 | 173 | 238 | 47.4% | 51.9% |
| ripgrep | rust | 443 | 545 | 162 | 184 | 63.4% | 66.2% |
| serde | rust | 329 | 617 | 167 | 278 | 49.2% | 54.9% |
| lancedb | rust | 245 | 286 | 138 | 181 | 43.7% | 36.7% |
| axum | rust | 229 | 288 | 139 | 174 | 39.3% | 39.6% |
| llmcc | rust | 296 | 669 | 187 | 439 | 36.8% | 34.4% |

## Thread Scaling (databend, depth=3, top-200, 8 cores)

| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|------------|---------|-------|------|-------|---------|
| 1 | 2.48s | 4.37s | 2.66s | 1.17s | 0.58s | 12.3s | - |
| 2 | 1.44s | 2.13s | 1.55s | 0.69s | 0.40s | 7.13s | 1.73x |
| 4 | 0.88s | 1.36s | 0.88s | 0.39s | 0.20s | 4.43s | 2.79x |
| 8 | 0.78s | 0.96s | 0.59s | 0.26s | 0.15s | 3.46s | 3.57x |
| 16 | 0.67s | 0.85s | 0.51s | 0.20s | 0.11s | 3.07s | 4.02x |
