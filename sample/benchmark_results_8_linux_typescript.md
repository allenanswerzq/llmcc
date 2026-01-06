# LLMCC Benchmark Results

Generated on: 2026-01-05 19:14:16

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30.3G
- **Available:** 20.0G

### Disk
- **Write Speed:** 820 MB/s

### OS
- **Kernel:** 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| vscode | typescript | 4792 | ~1508K | 0.68s | 2.39s | 0.50s | 0.43s | 0.27s | 5.85s |
| angular | typescript | 2940 | ~717K | 0.28s | 0.92s | 0.14s | 0.10s | 0.04s | 1.86s |
| typescript | typescript | 703 | ~693K | 0.51s | 1.85s | 0.39s | 0.35s | 0.19s | 4.05s |
| prisma | typescript | 1600 | ~178K | 0.11s | 0.79s | 0.04s | 0.02s | 0.01s | 1.07s |
| nest | typescript | 1410 | ~87K | 0.05s | 0.24s | 0.02s | 0.02s | 0.01s | 0.42s |
| shadcn-ui | typescript | 261 | ~68K | 0.03s | 0.23s | 0.01s | 0.01s | 0.00s | 0.33s |
| react | typescript | 412 | ~62K | 0.06s | 0.18s | 0.02s | 0.02s | 0.01s | 0.36s |
| excalidraw | typescript | 236 | ~60K | 0.03s | 0.06s | 0.04s | 0.01s | 0.01s | 0.21s |
| hono | typescript | 297 | ~48K | 0.05s | 0.09s | 0.03s | 0.01s | 0.00s | 0.22s |
| trpc | typescript | 225 | ~44K | 0.02s | 0.05s | 0.01s | 0.00s | 0.00s | 0.12s |
| tanstack-query | typescript | 308 | ~36K | 0.03s | 0.10s | 0.01s | 0.01s | 0.00s | 0.19s |
| zustand | typescript | 17 | ~1K | 0.01s | 0.01s | 0.00s | 0.00s | 0.00s | 0.04s |

## Summary

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 6 projects
- Large (>500 files): 5 projects

## PageRank Graph Reduction (depth=3, top-200)

| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|----------|------------|------------|----------|----------|----------------|----------------|
| vscode | typescript | 18002 | 29363 | 126 | 158 | 99.3% | 99.5% |
| angular | typescript | 6743 | 12051 | 144 | 165 | 97.9% | 98.6% |
| typescript | typescript | 13081 | 42421 | 190 | 919 | 98.5% | 97.8% |
| prisma | typescript | 1180 | 1387 | 144 | 150 | 87.8% | 89.2% |
| nest | typescript | 739 | 814 | 108 | 110 | 85.4% | 86.5% |
| shadcn-ui | typescript | 199 | 259 | 183 | 235 | 8.0% | 9.3% |
| react | typescript | 829 | 1231 | 168 | 298 | 79.7% | 75.8% |
| excalidraw | typescript | 90 | 82 | 88 | 80 | 2.2% | 2.4% |
| hono | typescript | 102 | 92 | 85 | 73 | 16.7% | 20.7% |
| trpc | typescript | 360 | 411 | 162 | 206 | 55.0% | 49.9% |
| tanstack-query | typescript | 324 | 467 | 178 | 265 | 45.1% | 43.3% |
| zustand | typescript | 6 | 4 | 6 | 4 | 0.0% | 0.0% |
