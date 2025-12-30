# LLMCC Benchmark Results

Generated on: 2025-12-30 01:45:56

## PageRank Timing (depth=3, top-200)

| Project | Files | LoC | Parse | IR Build | Symbols | Binding | Graph | Link | Total |
|---------|-------|-----|-------|----------|---------|---------|-------|------|-------|
| databend | 3130 | 627K | 0.26s | 1.25s | 0.30s | 3.15s | 0.56s | 0.07s | 6.40s |
| risingwave | 2382 | 578K | 0.23s | 0.65s | 0.23s | 1.96s | 0.44s | 0.06s | 4.22s |
| datafusion | 980 | 498K | 0.26s | 0.72s | 0.25s | 1.67s | 0.29s | 0.06s | 3.66s |
| ruff | 1661 | 418K | 0.21s | 0.56s | 0.22s | 1.61s | 0.29s | 0.05s | 3.41s |
| lance | 442 | 246K | 0.12s | 0.20s | 0.06s | 0.40s | 0.13s | 0.02s | 1.07s |
| qdrant | 864 | 237K | 0.11s | 0.52s | 0.10s | 0.65s | 0.17s | 0.02s | 1.77s |
| codex | 617 | 224K | 0.09s | 0.21s | 0.07s | 0.32s | 0.12s | 0.03s | 0.99s |
| opendal | 715 | 94K | 0.05s | 0.12s | 0.04s | 0.29s | 0.08s | 0.01s | 0.68s |
| tokio | 456 | 92K | 0.04s | 0.13s | 0.03s | 0.12s | 0.06s | 0.01s | 0.44s |
| clap | 118 | 59K | 0.02s | 0.08s | 0.02s | 0.06s | 0.02s | 0.00s | 0.22s |
| ripgrep | 77 | 38K | 0.03s | 0.06s | 0.02s | 0.04s | 0.03s | 0.01s | 0.21s |
| serde | 58 | 33K | 0.02s | 0.05s | 0.01s | 0.06s | 0.02s | 0.00s | 0.18s |
| lancedb | 78 | 30K | 0.04s | 0.04s | 0.01s | 0.04s | 0.02s | 0.00s | 0.17s |
| axum | 109 | 29K | 0.02s | 0.03s | 0.01s | 0.02s | 0.01s | 0.00s | 0.11s |
| llmcc | 45 | 18K | 0.02s | 0.03s | 0.01s | 0.04s | 0.02s | 0.00s | 0.13s |

## Summary

Benchmarked on: Linux zhang 5.15.167.4-microsoft-standard-WSL2 #1 SMP Tue Nov 5 00:21:55 UTC 2024 x86_64 x86_64 x86_64 GNU/Linux

Binary: /home/yibai/llmcc/target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 7 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| databend | 7057 | 15065 | 154 | 227 | 100.0% | 100.0% |
| risingwave | 6623 | 13699 | 164 | 225 | 100.0% | 100.0% |
| datafusion | 4888 | 8980 | 155 | 270 | 100.0% | 100.0% |
| ruff | 7090 | 16763 | 171 | 297 | 100.0% | 100.0% |
| lance | 2216 | 3788 | 137 | 203 | 100.0% | 100.0% |
| qdrant | 3098 | 6844 | 155 | 229 | 100.0% | 100.0% |
| codex | 3186 | 5188 | 156 | 224 | 100.0% | 100.0% |
| opendal | 1392 | 1745 | 134 | 131 | 100.0% | 100.0% |
| tokio | 829 | 1131 | 155 | 198 | 90.0% | 90.0% |
| clap | 330 | 497 | 171 | 240 | 50.0% | 60.0% |
| ripgrep | 438 | 544 | 158 | 188 | 70.0% | 70.0% |
| serde | 327 | 616 | 168 | 287 | 50.0% | 60.0% |
| lancedb | 226 | 243 | 134 | 151 | 50.0% | 40.0% |
| axum | 229 | 283 | 137 | 161 | 50.0% | 50.0% |
| llmcc | 241 | 554 | 187 | 460 | 30.0% | 20.0% |
