# LLMCC Benchmark Results

Generated on: 2026-01-06 00:35:00

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30.3G
- **Available:** 18.8G

### Disk
- **Write Speed:** 769 MB/s

### OS
- **Kernel:** 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| vscode | typescript | 4793 | ~1509K | 0.84s | 1.24s | 0.51s | 0.43s | 0.27s | 4.87s |
| angular | typescript | 2938 | ~718K | 0.42s | 0.32s | 0.12s | 0.09s | 0.04s | 1.35s |
| typescript | typescript | 703 | ~693K | 0.50s | 0.86s | 0.37s | 0.31s | 0.18s | 3.05s |
| prisma | typescript | 1600 | ~178K | 0.24s | 0.13s | 0.04s | 0.02s | 0.01s | 0.57s |
| nest | typescript | 1410 | ~87K | 0.13s | 0.09s | 0.02s | 0.02s | 0.01s | 0.36s |
| shadcn-ui | typescript | 261 | ~68K | 0.04s | 0.03s | 0.01s | 0.01s | 0.00s | 0.14s |
| react | typescript | 412 | ~62K | 0.07s | 0.06s | 0.02s | 0.01s | 0.01s | 0.25s |
| excalidraw | typescript | 236 | ~60K | 0.04s | 0.05s | 0.03s | 0.01s | 0.01s | 0.19s |
| hono | typescript | 297 | ~48K | 0.06s | 0.08s | 0.03s | 0.01s | 0.00s | 0.22s |
| trpc | typescript | 225 | ~44K | 0.03s | 0.02s | 0.01s | 0.01s | 0.00s | 0.11s |
| tanstack-query | typescript | 308 | ~36K | 0.04s | 0.02s | 0.01s | 0.01s | 0.00s | 0.11s |
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
| vscode | typescript | 19435 | 32540 | 135 | 167 | 99.3% | 99.5% |
| angular | typescript | 7478 | 14051 | 157 | 211 | 97.9% | 98.5% |
| typescript | typescript | 15767 | 49895 | 191 | 859 | 98.8% | 98.3% |
| prisma | typescript | 1493 | 1855 | 149 | 181 | 90.0% | 90.2% |
| nest | typescript | 775 | 855 | 112 | 124 | 85.5% | 85.5% |
| shadcn-ui | typescript | 273 | 361 | 188 | 267 | 31.1% | 26.0% |
| react | typescript | 1031 | 1640 | 170 | 328 | 83.5% | 80.0% |
| excalidraw | typescript | 106 | 93 | 100 | 88 | 5.7% | 5.4% |
| hono | typescript | 132 | 122 | 115 | 104 | 12.9% | 14.8% |
| trpc | typescript | 417 | 510 | 177 | 220 | 57.6% | 56.9% |
| tanstack-query | typescript | 341 | 506 | 177 | 271 | 48.1% | 46.4% |
| zustand | typescript | 6 | 4 | 6 | 4 | 0.0% | 0.0% |
