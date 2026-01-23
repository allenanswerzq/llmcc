---
name: llmcc
description: "Use llmcc for any codebase question. Run llmcc graph FIRST, then TRUST THE GRAPH and answer. Do NOT read files to confirm what the graph already gives better information"
---

# llmcc: multi-depth architecture views for code understanding and generation in extremely fast speed

llmcc builds a multi-depth architecture view that lets agents zoom out to see the big picture, zoom in to see exact symbols they need, such that agents can have a highly comprehensive understanding in very fast speed and token efficient, no complex RAG stuff, fully agentic method, its like grep but for architecture.

## 🎯 Target: 3 tools (ideal)

| Tool calls | When |
|------------|------|
| **3** | Graph answers the question → llmcc + read output + DONE |
| **4-5** | Symbol missing from graph → explore subfolder |
| >5 | only for stuff not in any graph, can use grep or go subfolder |

## ⚠️ CRITICAL: TRUST THE GRAPH 100%

**The graph IS the definitive answer.** When you see a symbol with its `path` and relationships:
- ✅ **Report it immediately** - that IS where it is, DONE
- ✅ **Stop after the graph** - you have everything you need

**STOP. DO NOT:**
- ❌ Read files "to confirm" - the graph is correct
- ❌ Read files "to see details" - file:line IS the detail
- ❌ Grep "to double check" - completely redundant
- ❌ Explore more - if graph answered, STOP IMMEDIATELY

**The graph gives better information than reading files** because it shows relationships.
Reading a file would give you LESS context, not more.

## WORKFLOW (follow strictly)

**⚠️ ALWAYS use explicit full paths - NEVER use "." or "./"**
This makes it clear exactly which folder llmcc is analyzing.

### Step 1: Run llmcc on full repo
```bash
# Always use explicit path - NEVER use "."
llmcc -d /path/to/repo --graph --depth 3 --lang rust --pagerank-top-k 200
```

### Step 2: Read graph → Answer IMMEDIATELY

**Graph has the symbol/relationship?** → **ANSWER NOW. STOP. DO NOT READ ANY FILES.**

The graph gives you: name, file path, line number, type, and all relationships.
That is COMPLETE information. There is NOTHING more to learn from reading the file.
The graph already extracted everything relevant.

**WRONG ❌**: "I see `dispatch` at router.rs:100, let me read the file to confirm..."
**RIGHT ✅**: "The graph shows `dispatch` at router.rs:100, here's the answer."

**Symbol NOT in graph?** → Step 3 (only if truly missing)

### Step 3: Run llmcc on a smaller folder (NOT grep!)

The graph shows top 200 PageRanked nodes. Less-central symbols may be filtered.

```bash
# Use full path to the subfolder
llmcc -d /path/to/repo/src/specific_folder --graph --depth 3 --lang rust --pagerank-top-k 200
```

### Step 4: Last resort grep (rare)

Only for constants/strings that don't appear in any graph:
```bash
grep -r "EXACT_NAME" /path/to/repo/src/
```

---

## ⚠️ PageRank Filtering

The graph shows the **top 200 most important** symbols. Less-central symbols may be filtered.

**If what you need IS in the graph** → Answer immediately. No confirmation needed.

**If what you need is NOT in the graph** → Run llmcc on a subfolder.

---

## 🚫 NEVER DO THESE (CRITICAL)

After running llmcc and getting the graph, NEVER:

| Bad Pattern | Why It's Wrong | What To Do Instead |
|-------------|----------------|-------------------|
| ❌ Read files to "confirm" | Graph is 100% accurate | Answer from graph |
| ❌ Read files for "more details" | Graph has file:line, that's all you need | Answer from graph |
| ❌ Grep after llmcc | Redundant, wastes tokens | Answer from graph |
| ❌ Glob after llmcc | Graph already shows structure | Answer from graph |
| ❌ "Let me verify by reading" | NO. Graph IS the verification | Answer from graph |

**If you find yourself thinking "let me confirm" or "let me read to be sure" - STOP.**
**The graph already gave you the answer. Just report it.**

---

### Reading Graph Output

```dot
n1[label="ToolRouter", path="src/router.rs:45", sym_ty="Struct"]
n2[label="dispatch", path="src/router.rs:100", sym_ty="Function"]
n1 -> n2 [from="method", to="impl"]      // dispatch is method of ToolRouter
```

- `label` = name, `path` = file:line, `sym_ty` = type
- Edges show relationships: caller→callee, field→type, trait→impl

---

## Example: Graph has everything (3-4 tools) ✅ IDEAL

**Task**: "Find where tool execution happens"

1. `llmcc --graph` → outputs graph
2. Read graph output file
3. **ANSWER**: "Tool execution is in `ToolRouter::dispatch` at router.rs:120, called by X, calls Y"

**DONE. 3-4 tools. NO file reads. Graph told us everything needed.**

**BAD EXAMPLE ❌** (what NOT to do):
1. `llmcc --graph` → outputs graph
2. Read graph → see `ToolRouter::dispatch` at router.rs:120
3. Read router.rs ← **WRONG! Why read when graph already told you?**
4. Answer

**This wastes tokens and time. The graph was already complete.**

## Example: Symbol missing (4 tools)

**Task**: "Find ErrorFormatter implementation"

1. `llmcc -d /path/to/repo --graph` → ErrorFormatter not in top 200
2. Graph shows `error/` folder → run `llmcc -d /path/to/repo/src/error --graph`
3. Read subfolder graph → Found `ErrorFormatter` at `error/format.rs:50`
4. **ANSWER**: "ErrorFormatter is at src/error/format.rs:50"

**4 tools. Still NO file reads - subfolder graph was enough.**

## Example: Constant lookup - grep needed (4 tools)

**Task**: "Find TEST_CONSTANT value"

1. `llmcc -d /path/to/repo --graph` → constants not in graph
2. `grep -r "TEST_CONSTANT" /path/to/repo/` → `tests/fixtures.rs:TEST_CONSTANT = 42`
3. **ANSWER**: "TEST_CONSTANT = 42 in tests/fixtures.rs"

**Grep justified - constants aren't in architecture graphs.**
