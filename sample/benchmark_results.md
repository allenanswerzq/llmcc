# LLMCC Benchmark Results

Generated on: 2025-12-28 15:51:11

## PageRank Timing (depth=3, top-200)

| Project | Files | LoC | Parse | IR Build | Symbols | Binding | Graph | Link | Total |
|---------|-------|-----|-------|----------|---------|---------|-------|------|-------|
| serde | 58 | 33K | 0.11s | 0.13s | 0.02s | 0.00s | 0.02s | 0.01s | 0.33s |
| datafusion | 980 | 498K | 2.15s | 2.08s | 0.20s | 0.87s | 0.40s | 0.15s | 6.33s |
| codex | 617 | 224K | 0.93s | 0.85s | 0.08s | 0.29s | 0.15s | 0.04s | 2.52s |
| tokio | 456 | 92K | 0.38s | 0.49s | 0.05s | 0.12s | 0.08s | 0.03s | 1.20s |
| llmcc | 45 | 18K | 0.07s | 0.09s | 0.01s | 0.01s | 0.02s | 0.01s | 0.23s |
| opendal | 715 | 94K | 0.45s | 0.62s | 0.06s | 0.15s | 0.12s | 0.03s | 1.55s |
| lancedb | 78 | 30K | 0.16s | 0.14s | 0.02s | 0.04s | 0.02s | 0.01s | 0.42s |
| ripgrep | 77 | 38K | 0.17s | 0.17s | 0.02s | 0.02s | 0.03s | 0.02s | 0.46s |
| lance | 442 | 246K | 1.25s | 1.24s | 0.10s | 0.27s | 0.19s | 0.06s | 3.29s |
| ruff | 1661 | 418K | 1.72s | 2.17s | 0.19s | 1.44s | 0.43s | 0.12s | 6.80s |
| qdrant | 864 | 237K | 1.02s | 1.52s | 0.12s | 0.27s | 0.26s | 0.05s | 3.51s |
| clap | 118 | 59K | 0.15s | 0.20s | 0.03s | 0.06s | 0.03s | 0.01s | 0.51s |
| axum | 109 | 29K | 0.12s | 0.14s | 0.02s | 0.01s | 0.02s | 0.01s | 0.33s |
| risingwave | 2382 | 578K | 2.53s | 2.86s | 0.26s | 2.08s | 0.67s | 0.18s | 9.65s |
| databend | 3130 | 627K | 2.62s | 4.27s | 0.33s | 2.85s | 0.85s | 0.23s | 12.66s |

## Summary

Benchmarked on: Linux zhang 5.15.167.4-microsoft-standard-WSL2 #1 SMP Tue Nov 5 00:21:55 UTC 2024 x86_64 x86_64 x86_64 GNU/Linux

Binary: ../target/release/llmcc

### Project Sizes
- Small (<50 files): 1 projects
- Medium (50-500 files): 7 projects
- Large (>500 files): 7 projects

## PageRank Graph Reduction (depth=3)

| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |
|---------|------------|------------|----------|----------|----------------|----------------|
| serde | 327 | 615 | 9 | 8 | 100.0% | 100.0% |
| datafusion | 3955 | 6754 | 146 | 230 | 100.0% | 100.0% |
| codex | 2739 | 4366 | 154 | 248 | 100.0% | 100.0% |
| tokio | 817 | 1119 | 152 | 195 | 90.0% | 90.0% |
| llmcc | 148 | 271 | 85 | 245 | 50.0% | 10.0% |
| opendal | 1304 | 1632 | 120 | 119 | 100.0% | 100.0% |
| lancedb | 226 | 244 | 127 | 147 | 50.0% | 40.0% |
| ripgrep | 207 | 240 | 153 | 179 | 30.0% | 30.0% |
| lance | 1736 | 2992 | 136 | 202 | 100.0% | 100.0% |
| ruff | 6394 | 14949 | 157 | 244 | 100.0% | 100.0% |
| qdrant | 2565 | 4792 | 159 | 257 | 100.0% | 100.0% |
| clap | 244 | 370 | 168 | 242 | 40.0% | 40.0% |
| axum | 229 | 283 | 144 | 182 | 40.0% | 40.0% |
| risingwave | 5364 | 10941 | 164 | 225 | 100.0% | 100.0% |
| databend | 5677 | 11884 | 150 | 221 | 100.0% | 100.0% |
