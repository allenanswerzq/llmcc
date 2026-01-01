# SWE-bench Integration for llmcc

This directory contains tools and scripts to evaluate llmcc's impact on AI coding agent performance using the [SWE-bench](https://www.swebench.com/) benchmark.

## Goal

Prove that providing llmcc architecture context to AI agents **improves their ability to solve real-world GitHub issues**.

## Hypothesis

> AI coding agents with llmcc context will solve more SWE-bench tasks than agents without it, because they can quickly understand codebase architecture instead of fumbling through grep/read operations.

## Benchmark Targets

### Phase 1: Rust (Current)
SWE-bench Multilingual includes 43 Rust tasks from these repos:

| Repository | Tasks | Baseline Resolution |
|------------|-------|---------------------|
| tokio-rs/tokio | 9 | 55.6% |
| tokio-rs/axum | 7 | 57.1% |
| astral-sh/ruff | 7 | 28.6% |
| sharkdp/bat | 8 | 75.0% |
| nushell/nushell | 5 | 100% |
| uutils/coreutils | 5 | 40.0% |
| burntsushi/ripgrep | 2 | 50.0% |
| **Total** | **43** | **58.1%** |

### Phase 2: TypeScript/JavaScript (Future)
After adding TypeScript support to llmcc.

### Phase 3: Python (Future)
The main SWE-bench dataset (500+ tasks).

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    SWE-bench Task                        │
│  (GitHub issue + repo snapshot)                          │
└─────────────────────┬───────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────┐
│                 llmcc Context Injector                   │
│  1. Run: llmcc -d <repo> --depth 3 --pagerank-top-k 200 │
│  2. Convert DOT → Markdown summary                       │
│  3. Inject into agent's system prompt                    │
└─────────────────────┬───────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────┐
│              mini-SWE-agent + Claude                     │
│  (with architecture context in system prompt)            │
└─────────────────────┬───────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────┐
│                   Generated Patch                        │
└─────────────────────────────────────────────────────────┘
```

## Experiment Design

### Variables
- **Independent**: Presence of llmcc context (with/without)
- **Dependent**: Task resolution rate, tool calls count, tokens used

### Metrics
1. **Resolution Rate**: % of tasks where generated patch passes tests
2. **Efficiency**: Average tool calls per task
3. **Cost**: Average tokens/$ per task

### Statistical Significance
- Run each configuration 3x to account for LLM variance
- Report mean ± std for all metrics

## Directory Structure

```
swe-bench/
├── README.md                 # This file
├── requirements.txt          # Python dependencies
├── setup.sh                  # Environment setup script
├── configs/
│   └── rust.yaml            # Rust-specific configuration
├── src/
│   ├── context_injector.py  # llmcc → agent context
│   ├── run_experiment.py    # Main experiment runner
│   └── analyze_results.py   # Results analysis
├── results/
│   └── .gitkeep
└── logs/
    └── .gitkeep
```

## Quick Start

```bash
# 1. Setup environment
./setup.sh

# 2. Run baseline (no llmcc)
python src/run_experiment.py --config configs/rust.yaml --mode baseline

# 3. Run with llmcc context
python src/run_experiment.py --config configs/rust.yaml --mode llmcc

# 4. Analyze results
python src/analyze_results.py --baseline results/baseline --llmcc results/llmcc
```

## References

- [SWE-bench Paper](https://arxiv.org/abs/2310.06770)
- [SWE-bench Multilingual](https://www.swebench.com/multilingual.html)
- [mini-SWE-agent](https://github.com/SWE-agent/mini-swe-agent)
- [SWE-bench GitHub](https://github.com/SWE-bench/SWE-bench)
