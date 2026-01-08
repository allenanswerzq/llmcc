# LLMCC Benchmark Results

Generated on: 2026-01-07 11:15:19

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30.3G
- **Available:** 20.3G

### Disk
- **Write Speed:** 865 MB/s

### OS
- **Kernel:** 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| json | cpp | 50 | - | 0.14s | 0.14s | 0.05s | 0.14s | 0.01s | 0.60s |
| fmt | cpp | 17 | - | 0.04s | 0.04s | 0.02s | 0.03s | 0.01s | 0.20s |
| spdlog | cpp | 111 | - | 0.05s | 0.06s | 0.02s | 0.06s | 0.01s | 0.29s |
| imgui | cpp | 59 | - | 0.13s | 0.12s | 0.05s | 0.31s | 0.03s | 1.80s |
| leveldb | cpp | 129 | - | 0.03s | 0.03s | 0.01s | 0.05s | 0.01s | 0.21s |
| llama-cpp | cpp | 470 | - | 0.37s | 0.68s | 0.22s | 0.47s | 0.07s | 12.4s |

## Summary

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 5 projects
- Large (>500 files): 0 projects

## PageRank Graph Reduction (depth=3, top-200)

| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|----------|------------|------------|----------|----------|----------------|----------------|
| json | cpp | 833 | 1342 | 155 | 335 | 81.4% | 75.0% |
| fmt | cpp | 1597 | 2628 | 119 | 212 | 92.5% | 91.9% |
| spdlog | cpp | 2765 | 4362 | 152 | 222 | 94.5% | 94.9% |
| imgui | cpp | 2827 | 3370 | 132 | 154 | 95.3% | 95.4% |
| leveldb | cpp | 1933 | 2637 | 151 | 177 | 92.2% | 93.3% |
| llama-cpp | cpp | 6560 | 6654 | 74 | 78 | 98.9% | 98.8% |

## Thread Scaling (databend, depth=3, top-200, 8 cores)

| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|------------|---------|-------|------|-------|---------|
| 1 | 3.07s | 4.41s | 2.88s | 1.57s | 1.05s | 14.5s | - |
| 2 | 3.60s | 6.11s | 2.09s | 0.90s | 0.66s | 14.4s | 1.01x |
| 4 | 0.85s | 1.20s | 0.70s | 0.43s | 0.28s | 4.28s | 3.39x |
| 8 | 0.65s | 0.90s | 0.48s | 0.30s | 0.23s | 3.35s | 4.34x |
| 16 | 0.62s | 0.67s | 0.40s | 0.35s | 0.20s | 2.97s | 4.89x |
