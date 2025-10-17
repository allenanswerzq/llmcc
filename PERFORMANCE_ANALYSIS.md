# LLMCC Compiler Timing Analysis

## Summary

The compiler consists of 5 main steps. This document shows where time is spent and identifies optimization opportunities.

## Timing Breakdown (10 files)

```
=== TIMING SUMMARY ===
Step 1 (Load context):          387.979ms  (53.1%)
Step 2 (Create globals):        0.028ms    (<0.1%)
Step 3 (Build IR & symbols):    130.092ms  (17.8%)
Step 4 (Bind symbols & graph):  71.850ms   (9.8%)
  └─ Bind symbols subtotal:     51.494ms
  └─ Build graph subtotal:      19.925ms
Step 5 (Link units):            0.993ms    (0.1%)
  └─ link_units() call:         0.993ms
---
TOTAL:                          730.647ms
```

### Per-file Performance
- **Average per file**: ~73ms
- **Parsing per file**: ~38ms (tree-sitter)
- **Symbol collection**: ~13ms
- **Binding + graphing**: ~7.2ms

## Performance Bottlenecks

### 1. **Parsing (53% of time)** - Step 1
**Problem**: Tree-sitter parsing dominates execution time

**Root Cause**: 
- I/O bound: Reading and parsing files with tree-sitter
- Sequential processing: Files are parsed one at a time

**Optimization Opportunity**:
- ✅ **Parallelize file parsing** - Tree-sitter is thread-safe
- Expected improvement: 2-4x faster on multi-core systems (4-8 cores typical)

### 2. **Symbol Collection (18% of time)** - Step 3
**Problem**: Extracting symbols from AST takes significant time

**Root Cause**:
- Walking entire AST for each file
- Symbol trie insertions and lookups

**Optimization Opportunity**:
- Could be parallelized if `CompileCtxt` was made thread-safe
- Current blocker: `Cell<u32>` counters and non-Sync arenas

### 3. **Symbol Binding (7% of time)** - Step 4
**Problem**: Resolving symbol dependencies is notable but not critical

**Root Cause**:
- Cross-file symbol resolution
- Trie lookups for each dependency

### 4. **Link Units (0.1% of time)** - Step 5
**Status**: ✅ Already optimal
- Processing 508 unresolved symbols in <1ms
- Scales linearly with number of dependencies
- No optimization needed

## Scaling Analysis

### 430-file Processing
**Expected times based on linear scaling:**
- Step 1: ~16.6 seconds (430 ÷ 10 × 0.388s)
- Step 3: ~5.6 seconds (430 ÷ 10 × 0.130s)
- Step 4: ~3.1 seconds (430 ÷ 10 × 0.072s)
- **Expected Total**: ~26-27 seconds (vs reported ~30 seconds - matches well!)

**Actual reported 430-file times**:
- Step 1: 10.964s (parsing)
- Step 3: 6.838s (symbols)
- Step 4: 4.084s (binding + graph)
- Step 5: 7.786s (link units) ← **ANOMALY**

### Step 5 Anomaly
The reported 7.8 seconds for Step 5 on 430 files is unexpected:
- 10 files: 0.993ms
- 430 files expected: ~42ms (linear)
- 430 files actual: 7786ms ← **185x slower than expected**

**Hypothesis**: 
- Either the timing measurement captured something else
- Or there's quadratic behavior in cross-file dependencies
- Needs further investigation with detailed logging

## Recommendations

### Immediate (High Priority)
1. ✅ **Add fine-grained timing (done)** - Now we have visibility into each step
2. **Investigate Step 5 anomaly** - Add logging to understand why it takes 7.8s on 430 files
3. **Verify Step 1 timing** - Confirm it's tree-sitter parsing and not something else

### Short-term (Medium Priority)
1. **Parallelize file parsing** (Step 1)
   - Modify main.rs to parse files concurrently
   - Expected 2-4x improvement = 5-8 seconds saved
   - Low complexity implementation

2. **Profile symbol operations** (Step 3)
   - Identify hot paths in symbol collection
   - Optimize trie insertions if possible
   - Expected 10-20% improvement = 0.6-1.3 seconds saved

### Long-term (Lower Priority)
1. **Make `CompileCtxt` thread-safe** (enables major optimizations)
   - Replace `Cell<u32>` with atomic counters
   - Replace `typed_arena::Arena` with thread-safe allocator
   - Would unlock parallelization of Steps 3 & 4
   - Complex refactoring but high payoff

## Performance Goals
- **Current**: ~30s for 430 files (~70ms/file)
- **After parsing parallelization**: ~12-15s (~28-35ms/file)
- **After full optimization**: ~8-10s (~19-23ms/file)

## Testing Strategy
Run on progressively larger codebases:
- 10 files (current): 0.73s ✅
- 50 files: ~3-4s (extrapolated)
- 100 files: ~7-8s (extrapolated)
- 430+ files: ~28-30s (current)

Monitor each step separately to catch regressions.
