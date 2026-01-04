# LLMCC Benchmark Results

Generated on: 2026-01-04 15:17:36

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 16 physical, 16 logical (threads)

### Memory
- **Total:** Unknown
- **Available:** Unknown

### OS
- **Kernel:** Windows 10.0.26200
- **Distribution:** Microsoft Windows 11 Enterprise

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| databend | rust | 3147 | ~744K | 0.46s | 4.68s | 0.53s | 0.18s | 0.07s | 7.24s |
| ruff | rust | 1663 | ~347K | 0.37s | 2.78s | 0.39s | 0.21s | 0.06s | 4.61s |
| rust-analyzer | rust | 1362 | ~286K | 0.27s | 3.10s | 0.30s | 0.14s | 0.05s | 4.50s |
| datafusion | rust | 985 | ~269K | 0.41s | 2.34s | 0.48s | 0.13s | 0.05s | 4.15s |
| qdrant | rust | 864 | ~200K | 0.18s | 1.99s | 0.14s | 0.08s | 0.03s | 2.84s |
| codex | rust | 619 | ~158K | 0.12s | 0.50s | 0.09s | 0.04s | 0.02s | 1.16s |
| opendal | rust | 715 | ~152K | 0.06s | 0.45s | 0.05s | 0.02s | 0.01s | 0.84s |
| tokio | rust | 456 | ~152K | 0.05s | 0.51s | 0.04s | 0.02s | 0.01s | 0.78s |
| candle | rust | 383 | ~113K | 0.08s | 0.63s | 0.10s | 0.04s | 0.02s | 1.16s |
| lance | rust | 447 | ~102K | 0.15s | 0.66s | 0.11s | 0.04s | 0.02s | 1.42s |
| clap | rust | 118 | ~66K | 0.03s | 0.33s | 0.02s | 0.01s | - | 0.45s |
| axum | rust | 109 | ~58K | 0.02s | 0.07s | 0.01s | - | - | 0.15s |
| serde | rust | 58 | ~42K | 0.02s | 0.21s | 0.02s | 0.01s | - | 0.30s |
| ripgrep | rust | 77 | ~20K | 0.04s | 0.14s | 0.02s | 0.01s | 0.01s | 0.28s |
| lancedb | rust | 78 | ~17K | 0.03s | 0.15s | 0.01s | 0.01s | - | 0.27s |
| llmcc | rust | 52 | ~12K | 0.02s | 0.06s | 0.01s | - | - | 0.14s |
| risingwave | rust | 0 | - | - | - | - | - | - | - |

## Summary

Binary: C:\Users\zhangqiang\llmcc\target\release\llmcc.exe

### Project Sizes
- Small (<50 files): 0 projects
- Medium (50-500 files): 9 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3)

| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|----------|------------|------------|----------|----------|----------------|----------------|
| databend | rust | 7090 | 14771 | 151 | 219 | 97.9% | 98.5% |
| ruff | rust | 7120 | 16938 | 170 | 288 | 97.6% | 98.3% |
| rust-analyzer | rust | 5827 | 18578 | 176 | 474 | 97.0% | 97.4% |
| datafusion | rust | 4929 | 8518 | 149 | 264 | 97.0% | 96.9% |
| qdrant | rust | 3104 | 6778 | 151 | 226 | 95.1% | 96.7% |
| codex | rust | 3193 | 5168 | 153 | 226 | 95.2% | 95.6% |
| opendal | rust | 1393 | 1718 | 134 | 127 | 90.4% | 92.6% |
| tokio | rust | 836 | 1118 | 154 | 190 | 81.6% | 83.0% |
| candle | rust | 2213 | 4609 | 149 | 216 | 93.3% | 95.3% |
| lance | rust | 2228 | 3700 | 139 | 197 | 93.8% | 94.7% |
| clap | rust | 330 | 483 | 171 | 235 | 48.2% | 51.3% |
| axum | rust | 231 | 284 | 138 | 159 | 40.3% | 44.0% |
| serde | rust | 327 | 613 | 168 | 285 | 48.6% | 53.5% |
| ripgrep | rust | 446 | 541 | 161 | 180 | 63.9% | 66.7% |
| lancedb | rust | 246 | 258 | 138 | 156 | 43.9% | 39.5% |
| llmcc | rust | 293 | 638 | 187 | 418 | 36.2% | 34.5% |
| risingwave | rust | - | - | - | - | - | - |

## Thread Scaling (databend, depth=3, top-200, 16 cores)

| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|------------|---------|-------|------|-------|---------|
| 1 | 3.09s | 24.4s | 3.17s | 1.18s | 0.44s | 34.3s | - |
| 2 | 1.68s | 15.0s | 1.54s | 0.59s | 0.22s | 20.8s | 1.65x |
| 4 | 0.92s | 7.81s | 0.83s | 0.31s | 0.13s | 11.4s | 3.00x |
| 8 | 0.65s | 5.16s | 0.52s | 0.19s | 0.09s | 7.97s | 4.31x |
| 16 | 0.64s | 4.85s | 0.58s | 0.23s | 0.07s | 7.68s | 4.47x |
