# LLMCC Benchmark Results

Generated on: 2026-01-05 01:11:36

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 16 physical, 16 logical (threads)

### Memory
- **Total:** Unknown
- **Available:** Unknown

### Disk
- **Write Speed:** 964.8 MB/s

### OS
- **Kernel:** Windows 10.0.26200
- **Distribution:** Microsoft Windows 11 Enterprise

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| databend | rust | 3147 | ~631K | 0.44s | 0.83s | 0.42s | 0.21s | 0.07s | 2.90s |
| ruff | rust | 1663 | ~423K | 0.34s | 0.60s | 0.29s | 0.15s | 0.05s | 2.05s |
| rust-analyzer | rust | 1362 | ~474K | 0.24s | 0.49s | 0.26s | 0.12s | 0.04s | 1.64s |
| datafusion | rust | 985 | ~505K | 0.40s | 0.54s | 0.46s | 0.13s | 0.04s | 2.15s |
| qdrant | rust | 864 | ~237K | 0.16s | 0.27s | 0.14s | 0.07s | 0.02s | 0.98s |
| codex | rust | 619 | ~227K | 0.10s | 0.12s | 0.06s | 0.03s | 0.01s | 0.58s |
| opendal | rust | 715 | ~94K | 0.05s | 0.08s | 0.05s | 0.02s | 0.01s | 0.39s |
| tokio | rust | 456 | ~92K | 0.06s | 0.07s | 0.03s | 0.02s | - | 0.30s |
| candle | rust | 383 | ~159K | 0.09s | 0.12s | 0.06s | 0.03s | 0.01s | 0.53s |
| lance | rust | 447 | ~250K | 0.13s | 0.14s | 0.08s | 0.03s | 0.01s | 0.73s |
| clap | rust | 118 | ~60K | 0.02s | 0.04s | 0.01s | 0.01s | - | 0.13s |
| axum | rust | 109 | ~29K | 0.02s | 0.02s | 0.01s | - | - | 0.08s |
| serde | rust | 58 | ~33K | 0.02s | 0.03s | 0.01s | 0.01s | - | 0.11s |
| ripgrep | rust | 77 | ~38K | 0.04s | 0.03s | 0.01s | 0.01s | - | 0.15s |
| lancedb | rust | 78 | ~30K | 0.03s | 0.03s | 0.01s | 0.01s | - | 0.13s |
| llmcc | rust | 52 | ~19K | 0.01s | 0.02s | 0.01s | - | - | 0.08s |
| risingwave | rust | 0 | - | - | - | - | - | - | - |

## Summary

Binary: C:\Users\zhangqiang\llmcc\target\release\llmcc.exe

### Project Sizes
- Small (<50 files): 0 projects
- Medium (50-500 files): 9 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3, top-200)

| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|----------|------------|------------|----------|----------|----------------|----------------|
| databend | rust | 7080 | 14741 | 156 | 217 | 97.8% | 98.5% |
| ruff | rust | 7122 | 16938 | 170 | 285 | 97.6% | 98.3% |
| rust-analyzer | rust | 5835 | 18659 | 182 | 479 | 96.9% | 97.4% |
| datafusion | rust | 4921 | 8542 | 150 | 255 | 97.0% | 97.0% |
| qdrant | rust | 3102 | 6761 | 146 | 223 | 95.3% | 96.7% |
| codex | rust | 3189 | 5161 | 151 | 221 | 95.3% | 95.7% |
| opendal | rust | 1394 | 1719 | 137 | 129 | 90.2% | 92.5% |
| tokio | rust | 829 | 1109 | 154 | 189 | 81.4% | 83.0% |
| candle | rust | 2215 | 4607 | 151 | 216 | 93.2% | 95.3% |
| lance | rust | 2215 | 3653 | 140 | 199 | 93.7% | 94.6% |
| clap | rust | 323 | 469 | 171 | 233 | 47.1% | 50.3% |
| axum | rust | 229 | 283 | 139 | 160 | 39.3% | 43.5% |
| serde | rust | 327 | 613 | 168 | 285 | 48.6% | 53.5% |
| ripgrep | rust | 443 | 533 | 159 | 176 | 64.1% | 67.0% |
| lancedb | rust | 227 | 244 | 140 | 156 | 38.3% | 36.1% |
| llmcc | rust | 293 | 638 | 186 | 419 | 36.5% | 34.3% |
| risingwave | rust | - | - | - | - | - | - |

## Thread Scaling (databend, depth=3, top-200, 16 cores)

| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|------------|---------|-------|------|-------|---------|
| 1 | 2.58s | 4.55s | 2.39s | 0.98s | 0.36s | 11.9s | - |
| 2 | 1.49s | 2.63s | 1.54s | 0.56s | 0.20s | 7.52s | 1.59x |
| 4 | 0.87s | 1.49s | 1.02s | 0.30s | 0.11s | 4.98s | 2.40x |
| 8 | 0.59s | 1.03s | 0.49s | 0.17s | 0.07s | 3.44s | 3.47x |
| 16 | 0.49s | 0.92s | 0.41s | 0.15s | 0.06s | 3.05s | 3.92x |
