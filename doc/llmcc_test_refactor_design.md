# llmcc-test Runner Refactor Design

## Status: Phase 1 Complete

Phase 1 implemented the modular snapshot architecture. The snapshot module is now
separate from runner.rs with individual snapshot types for each verification layer.

---

## Implemented Structure

```
llmcc-test/src/
├── lib.rs                    # Re-exports (includes snapshot module)
├── corpus.rs                 # Case parsing (unchanged)
├── runner.rs                 # Orchestration (simplified, ~1288 lines)
└── snapshot/
    ├── mod.rs               # Snapshot trait + utilities + TableBuilder
    ├── symbols.rs           # Symbol table snapshot (SymbolsSnapshot)
    ├── symbol_deps.rs       # Symbol dependency snapshot (SymbolDepsSnapshot) [stub]
    ├── block_graph.rs       # Block tree S-expr snapshot (BlockGraphSnapshot)
    └── block_relations.rs   # cc.related_map snapshot (BlockRelationsSnapshot) [NEW]
```

---

## Snapshot Trait

```rust
pub trait Snapshot: Sized {
    /// Capture a snapshot from the compilation context.
    fn capture(ctx: SnapshotContext<'_>) -> Self;

    /// Render the snapshot to a string for comparison.
    fn render(&self) -> String;

    /// Normalize text for comparison (handles whitespace, sorting, etc.).
    fn normalize(text: &str) -> String;
}
```

---

## Implemented Snapshots

### SymbolsSnapshot
Verifies symbol collection phase.

```
u0:22 | Function | main | [global]
u0:23 | Variable | x    |
```

### SymbolDepsSnapshot
Currently a stub - symbol dependency tracking needs implementation via block relations.

### BlockGraphSnapshot
Verifies block tree hierarchy from graph_builder.rs.

```
(Root:0
    (Function:1 main
        (Parameters:2)
    )
)
```

### BlockRelationsSnapshot (NEW)
Verifies relationships established by `connect_blocks()`:

```
u0:3 | Struct | Foo
  HasImpl -> [u0:5]
  HasMethod -> [u0:6, u0:7]
u0:5 | Impl |
  ImplFor -> [u0:3]
u0:6 | Method | bar
  MethodOf -> [u0:3]
```

---

## Current State

### Problems with `runner.rs` (now ~1288 lines)

1. **Monolithic Structure**: Single file handling all responsibilities:
   - Case parsing and file I/O
   - Pipeline orchestration (symbols → blocks → graphs)
   - Snapshot/rendering for each expectation type
   - Normalization and diffing
   - S-expression parsing for block graphs
   - Graph DOT rendering

2. **Too Many Expectation Types Mixed Together**:
   - `symbols` - Symbol table verification
   - `symbol-deps` - Symbol dependency relations
   - `blocks` - Block list verification
   - `block-deps` - Block dependency relations (DISABLED - uses old API)
   - `block-graph` - S-expression tree structure
   - `dep-graph` - DOT graph rendering (DISABLED)
   - `arch-graph` - DOT architecture graph (DISABLED)

3. **Stale APIs**: `block-deps`, `dep-graph`, `arch-graph` disabled - return empty/None

4. **Tight Coupling**: Pipeline options, snapshots, and rendering still intertwined

---

## Next Steps (Phase 2)

1. **Migrate runner.rs to use snapshot module**
   - Replace inline `render_symbol_snapshot()` with `SymbolsSnapshot`
   - Replace inline `render_block_graph()` with `BlockGraphSnapshot`
   - Add `block-relations` expectation type

2. **Implement SymbolDepsSnapshot via block relations**
   - Query Uses/UsedBy relations from cc.related_map
   - Map back to symbol IDs

3. **Extract pipeline.rs**
   - Move `PipelineOptions`, `PipelineSummary`, `collect_pipeline()` to separate file
   - Clean separation between orchestration and snapshot capture

4. **Add block-relations corpus tests**
   - Create test cases verifying connect_blocks() output
   - Impl→Struct, Struct→Methods, Calls/CalledBy

---

## Expectation Types (Target State)

### Layer 1: Symbols (`expect:symbols`)
Verifies symbol collection phase.
--- expect:symbol-deps ---
u0:22 -> [u0:1]        # main depends on i32
u0:23 <- [u0:22]       # x is depended by main
```

### Layer 3: Block Graph (`expect:block-graph`)
Verifies block tree structure from graph_builder.

```
--- expect:block-graph ---
(Root:1
    (Func:2 main
        (Parameters:3)
        (Return:4)
        (Stmt:5)))
```

### Layer 4: Block Relations (`expect:block-relations`) [NEW]
Verifies `cc.related_map` populated by `connect_blocks()`.

```
--- expect:block-relations ---
BlockId(2) --ImplFor--> BlockId(1)
BlockId(1) --HasImpl--> BlockId(2)
BlockId(2) --HasMethod--> BlockId(3)
BlockId(3) --MethodOf--> BlockId(2)
```

---

## Corpus File Format (unchanged)

```
===============================================================================
test-case-name
===============================================================================

--- file: src/main.rs ---
fn main() {}

--- expect:symbols ---
...

--- expect:block-relations ---
...
```

---

## New Runner Architecture

### `pipeline.rs`

```rust
pub struct Pipeline<'tcx> {
    cc: &'tcx CompileCtxt<'tcx>,
    project_graph: Option<ProjectGraph<'tcx>>,
}

impl<'tcx> Pipeline<'tcx> {
    /// Run up to symbol collection
    pub fn run_collect<L: LanguageTraitImpl>(&mut self) { ... }

    /// Run up to symbol binding
    pub fn run_bind<L: LanguageTraitImpl>(&mut self) { ... }

    /// Run up to block graph building
    pub fn run_build_graph<L: LanguageTraitImpl>(&mut self) { ... }

    /// Run connect_blocks for relations
    pub fn run_connect_blocks(&mut self) { ... }
}
```

### `snapshot/mod.rs`

```rust
pub trait Snapshot {
    type Context<'a>;

    fn capture<'a>(ctx: Self::Context<'a>) -> Self;
    fn render(&self) -> String;
    fn normalize(text: &str) -> String;
}
```

### `snapshot/block_relations.rs` (NEW)

```rust
pub struct BlockRelationsSnapshot {
    relations: Vec<(BlockId, BlockRelation, BlockId)>,
}

impl Snapshot for BlockRelationsSnapshot {
    type Context<'a> = &'a CompileCtxt<'a>;

    fn capture<'a>(cc: Self::Context<'a>) -> Self {
        // Iterate cc.related_map and collect all relations
        let mut relations = Vec::new();
        for entry in cc.related_map.iter() {
            // ... collect (from, relation, to) tuples
        }
        Self { relations }
    }

    fn render(&self) -> String {
        // Output format:
        // BlockId(2) --ImplFor--> BlockId(1)
        // BlockId(1) --HasField--> BlockId(3)
    }
}
```

---

## Migration Plan

### Phase 1: Extract and Stabilize (Current Focus)
1. Create `snapshot/` module structure
2. Move existing snapshot logic to separate files
3. Keep old runner.rs working

### Phase 2: Add Block Relations
1. Implement `block_relations.rs` snapshot
2. Add `expect:block-relations` expectation type
3. Update corpus files with new expectations

### Phase 3: Remove Stale Code
1. Remove `blocks` and `block-deps` (replaced by `block-relations`)
2. Remove `dep-graph` and `arch-graph` (or move to separate render module)
3. Simplify runner.rs to only orchestration

### Phase 4: Improve Corpus Coverage
1. Add more test cases for each layer
2. Focus on edge cases:
   - Cross-file symbol resolution
   - Impl-for-struct linking
   - Trait implementations
   - Generic types

---

## Example Test Case for Block Relations

```
===============================================================================
impl-for-struct-basic
===============================================================================

--- file: src/main.rs ---
struct Widget {
    id: u32,
}

impl Widget {
    fn new(id: u32) -> Self {
        Widget { id }
    }
}

--- expect:symbols ---
u0:22 | Struct   | Widget | _c::main::Widget |
u0:23 | Field    | id     | _c::main::Widget::id |
u0:24 | Function | new    | _c::main::Widget::new |

--- expect:block-graph ---
(Root:1
    (Class:2 Widget
        (Field:3 id))
    (Impl:4
        (Func:5 new
            (Parameters:6)
            (Return:7))))

--- expect:block-relations ---
BlockId(4) --ImplFor--> BlockId(2)
BlockId(2) --HasImpl--> BlockId(4)
BlockId(4) --HasMethod--> BlockId(5)
BlockId(5) --MethodOf--> BlockId(4)
BlockId(2) --HasField--> BlockId(3)
BlockId(3) --FieldOf--> BlockId(2)
```

---

## Discussion Points

1. **Should we keep `dep-graph` and `arch-graph`?**
- not for now

2. **How to handle primitive symbols in output?**
   - use `--- expect:symbols(no-primitives) ---`

3. **Block ID stability**
   - Current runner runs tests sequentially, should be fine..

4. **S-expression vs tabular format for block-graph?**
   - S-expr shows hierarchy better

5. **Parallel test execution**
   - Current runner runs tests sequentially
   - (nice to have, but hard to do, becuase we assign id globally). Could we run corpus files in parallel?
