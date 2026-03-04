---
name: llmcc
description: "Use llmcc graph as a MAP to navigate codebases efficiently. For location questions, answer from graph. For comprehension questions, use graph to identify files, then read those files for implementation details."
---

# llmcc: Architecture graphs for fast codebase navigation

llmcc builds architecture graphs that show symbols (types, functions) and their relationships. Use it as a map to navigate codebases efficiently instead of blind grep exploration.

## Depth Levels

| Depth | Perspective | Best for |
|-------|-------------|----------|
| 0 | Project | multi-workspace / repo-to-repo relationships |
| 1 | Library/Crate | ownership boundaries, public API flow |
| 2 | Module | subsystem structure, refactor planning |
| 3 | File + symbol | implementation details, edit planning |

Most tasks use depth 3. Use depth 1-2 for architecture overview.

## How Graph Answers Questions Directly

The graph shows top 200 PageRanked symbols with their relationships (edges).

### Task: "What depends on X?" / "What breaks if I remove X?"
**Graph answers this DIRECTLY - DO NOT GREP**

Look at edges pointing TO symbols in X:
```dot
n5[label="CoreEngine", path="core/engine.rs:10"]  # in core crate
n12[label="Protocol", path="protocol/lib.rs:5"]    # in protocol crate
n5 -> n12 [from="uses", to="type"]                 # core DEPENDS ON protocol
```

The edge `n5 -> n12` means CoreEngine uses Protocol. Count incoming edges to Protocol's symbols = count of dependents. **No grep needed.**

### Task: "Where is X defined?"
**Graph answers this DIRECTLY - DO NOT GREP**

```dot
n3[label="ToolRouter", path="src/router.rs:45", sym_ty="Struct"]
```

Answer: "ToolRouter is at src/router.rs:45". Done. No file read needed.

### Task: "How does X work?" / "Trace flow from A to B"
**Graph provides TARGETS - then read 2-4 files only**

1. Find X in graph, note its file path
2. Follow edges to find related symbols (callers, callees)
3. Read ONLY those 2-4 files
4. Answer immediately - don't keep exploring

## Workflow

ALWAYS use explicit full paths - NEVER use "." or "./"

### Step 1: Run llmcc on full repo

```bash
llmcc -d /path/to/repo --graph --depth 3 --lang <language> --pagerank-top-k 200
```

Language detection:
- .rs files: --lang rust
- .ts/.js files: --lang typescript
- .cpp/.cc/.h files: --lang cpp

### Step 2: Answer from graph OR read targeted files

**CRITICAL DECISION TREE:**

```
Is the question about LOCATION or DEPENDENCIES?
  YES → Answer from graph edges/paths. DO NOT read files. DO NOT grep. STOP.
  NO → Continue below

Is the question about HOW something works (implementation)?
  YES → Graph tells you which 2-4 files to read. Read those ONLY. Then STOP.
  NO → You probably don't need llmcc for this task.
```

**For dependency questions ("what uses X", "what breaks if X removed"):**
1. Look at graph edges pointing TO symbols in X
2. Each edge source = a dependent. List them.
3. ANSWER IMMEDIATELY. You're done.
4. DO NOT grep to "verify" - the graph IS the verification

**For implementation questions ("how does X work"):**
1. Find X in graph, note the file path
2. Find 2-3 connected symbols, note their paths
3. Read those 2-3 files ONLY
4. ANSWER. Stop exploring.

### Step 3: If symbol not in graph

The graph shows top 200 PageRanked nodes. Less-central symbols may be filtered.

Option A - Run llmcc on subfolder:
```bash
llmcc -d /path/to/repo/src/specific_folder --graph --depth 3 --lang rust --pagerank-top-k 200
```

Option B - Grep as last resort (for constants, strings, peripheral symbols):
```bash
grep -r "EXACT_NAME" /path/to/repo/src/
```

## Reading Graph Output

```dot
n1[label="ToolRouter", path="src/router.rs:45", sym_ty="Struct"]
n2[label="dispatch", path="src/router.rs:100", sym_ty="Function"]
n1 -> n2 [from="method", to="impl"]
```

- label = name, path = file:line, sym_ty = type
- Edges show relationships: caller to callee, field to type, trait to impl

## Anti-patterns - NEVER DO THESE

| Pattern | Why It Destroys Value |
|---------|----------------------|
| Run llmcc then grep for same info | Graph already has it! You just doubled the cost |
| Grep each folder for "uses X" | Graph edges show uses. Zero greps needed |
| "Verify" graph by reading files | Graph is ground truth from AST parsing |
| Read 10+ files | You missed the point - graph tells you the 2-4 that matter |
| Grep `X::` in every crate | Graph edges already show which crates use X |

**The #1 failure mode:** Running llmcc, getting the graph, then falling back to grep-based exploration anyway. This is WORSE than not using llmcc at all (you pay the graph cost AND the grep cost).

Key insight: Graph is for NAVIGATION (picking targets), file reads are for UNDERSTANDING (getting details).
STOP as soon as you can answer. Don't explore for exploration's sake.

## Examples

### Dependency question (2-3 tools) - NO GREP ALLOWED

Task: "What would break if I removed the protocol crate?
1. `llmcc --graph` outputs graph with edges
2. Read graph - find all edges pointing TO protocol/* symbols:
   ```
   core::Engine -> protocol::Message      # core depends on protocol
   cli::Args -> protocol::Config          # cli depends on protocol
   tui::View -> protocol::Event           # tui depends on protocol
   ```
3. ANSWER: "core, cli, and tui crates depend on protocol. Removing it breaks those."

**DONE in 3 tools.** No grep. No file reads. Graph edges ARE the dependency list.

### Location question (2-3 tools) - NO GREP ALLOWED

Task: "Find where tool execution happens"

1. `llmcc --graph` outputs graph
2. Read graph: `n5[label="ToolRouter::dispatch", path="router.rs:120"]`
3. ANSWER: "Tool execution is in ToolRouter::dispatch at router.rs:120"

**DONE in 3 tools.** Graph has file:line. No need to read the file.

### Comprehension question (4-6 tools) - READ ONLY GRAPH-IDENTIFIED FILES

Task: "How does context window management work?"

1. `llmcc --graph` shows: ContextManager at history.rs:18, TruncationPolicy at truncate.rs:14
2. Graph edges show: Session → ContextManager → TruncationPolicy
3. Read history.rs (has ContextManager) and truncate.rs (has TruncationPolicy) - **2 files only**
4. ANSWER: explain the flow based on those 2 files

**DONE in 5-6 tools.** Graph identified the 2 key files. Read only those. Stop.

### Symbol not in main graph (5-6 tools)

Task: "Find ErrorFormatter implementation"

1. llmcc --graph on full repo - ErrorFormatter not in top 200
2. Graph shows error/ folder exists - run llmcc on /path/to/repo/src/error
3. Subfolder graph shows ErrorFormatter at error/format.rs:50
4. ANSWER: "ErrorFormatter is at src/error/format.rs:50"

### Constant lookup - grep needed (3-4 tools)

Task: "Find TEST_CONSTANT value"

1. llmcc --graph - constants not in architecture graph
2. grep -r "TEST_CONSTANT" /path/to/repo/ shows tests/fixtures.rs:TEST_CONSTANT = 42
3. ANSWER: "TEST_CONSTANT = 42 in tests/fixtures.rs"

Grep justified - constants are not in architecture graphs.
