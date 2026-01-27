# LLMCC Benchmark Results

Generated on: 2026-01-16 21:06:23

## Machine Info

### CPU
- **Model:** AMD Ryzen 9 8945HS w/ Radeon 780M Graphics
- **Cores:** 8 physical, 16 logical (threads)

### Memory
- **Total:** 30.3G
- **Available:** 21.4G

### Disk
- **Write Speed:** 1000 MB/s

### OS
- **Kernel:** 5.15.167.4-microsoft-standard-WSL2
- **Distribution:** Ubuntu 24.04.3 LTS

## PageRank Timing (depth=3, top-200)

| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |
|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|
| tensorflow | cpp | 10339 | ~2676K | 8.61s | 4.65s | 1.65s | 5.99s | 0.98s | 25.9s |
| clickhouse | cpp | 8314 | ~1171K | 3.06s | 3.48s | 1.20s | 2.79s | 0.48s | 13.0s |
| opencv | cpp | 3782 | ~765K | 2.66s | 5.75s | 1.75s | 3.93s | 0.31s | 16.8s |
| pytorch | cpp | 4077 | ~699K | 1.23s | 1.77s | 0.64s | 2.14s | 0.26s | 7.46s |
| rocksdb | cpp | 1320 | ~436K | 0.63s | 1.05s | 0.40s | 2.07s | 0.18s | 4.98s |
| grpc | cpp | 2846 | ~424K | 1.17s | 1.42s | 7.31s | 1.33s | 0.27s | 12.4s |
| protobuf | cpp | 1160 | ~232K | 1.23s | 1.44s | 0.47s | 1.46s | 0.17s | 5.31s |
| llama-cpp | cpp | 470 | ~215K | 0.27s | 0.44s | 0.16s | 0.57s | 0.05s | 1.73s |
| imgui | cpp | 59 | ~55K | 0.10s | 0.13s | 0.05s | 0.33s | 0.02s | 0.71s |
| json | cpp | 50 | ~49K | 0.12s | 0.12s | 0.05s | 0.12s | 0.01s | 0.46s |
| fmt | cpp | 17 | ~20K | 0.04s | 0.04s | 0.02s | 0.03s | 0.01s | 0.17s |
| leveldb | cpp | 129 | ~18K | 0.03s | 0.03s | 0.01s | 0.08s | 0.01s | 0.19s |
| spdlog | cpp | 111 | ~4K | 0.05s | 0.05s | 0.01s | 0.06s | 0.01s | 0.22s |
| 3fs | cpp | (not found) | - | - | - | - | - | - | - |

## Summary

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 5 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3, top-200)

| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|----------|------------|------------|----------|----------|----------------|----------------|
| tensorflow | cpp | 50909 | 109563 | 81 | 83 | 99.8% | 99.9% |
| clickhouse | cpp | 18602 | 29930 | 58 | 47 | 99.7% | 99.8% |
| opencv | cpp | 28868 | 58626 | 131 | 201 | 99.5% | 99.7% |
| pytorch | cpp | 25405 | 60273 | 111 | 150 | 99.6% | 99.8% |
| rocksdb | cpp | 11326 | 22544 | 122 | 154 | 98.9% | 99.3% |
| grpc | cpp | 23224 | 33746 | 39 | 30 | 99.8% | 99.9% |
| protobuf | cpp | 13344 | 21123 | 84 | 92 | 99.4% | 99.6% |
| llama-cpp | cpp | 3635 | 3312 | 46 | 39 | 98.7% | 98.8% |
| imgui | cpp | 1409 | 1831 | 135 | 183 | 90.4% | 90.0% |
| json | cpp | 322 | 465 | 191 | 323 | 40.7% | 30.5% |
| fmt | cpp | 514 | 777 | 161 | 258 | 68.7% | 66.8% |
| leveldb | cpp | 778 | 1176 | 156 | 200 | 79.9% | 83.0% |
| spdlog | cpp | 749 | 1053 | 151 | 244 | 79.8% | 76.8% |
| 3fs | cpp | - | - | - | - | - | - |
