# LLMCC Benchmark Results

Generated on: 2026-01-06 22:33:51

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30.3G
- **Available:** 19.7G

### Disk
- **Write Speed:** 1.0 GB/s

### OS
- **Kernel:** 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| llama-cpp | cpp | 470 | - | 0.28s | 0.00s | 0.00s | 0.00s | 0.00s | 0.00s |

## Summary

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 0 projects
- Medium (50-500 files): 1 projects
- Large (>500 files): 0 projects

## PageRank Graph Reduction (depth=3, top-200)

| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|----------|------------|------------|----------|----------|----------------|----------------|
| llama-cpp | cpp | - | - | - | - | - | - |

## Thread Scaling (databend, depth=3, top-200, 8 cores)

| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |
|---------|-------|------------|---------|-------|------|-------|---------|
| 1 | 2.01s | 3.82s | 2.38s | 1.27s | 0.91s | 11.4s | - |
| 2 | 1.14s | 1.97s | 1.13s | 0.67s | 0.46s | 6.19s | 1.84x |
| 4 | 0.79s | 1.24s | 0.60s | 0.36s | 0.25s | 3.92s | 2.91x |
| 8 | 0.60s | 0.83s | 0.55s | 0.31s | 0.20s | 3.16s | 3.61x |
| 16 | 0.56s | 0.85s | 0.46s | 0.29s | 0.17s | 3.06s | 3.73x |
