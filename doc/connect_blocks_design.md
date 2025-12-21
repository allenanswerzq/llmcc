# connect_blocks Design Document

## Overview

The goal of `connect_blocks` is to populate `cc.related_map` (a `BlockRelationMap` using `DashMap`) with all block-to-block relationships discovered across the entire project graph. This is a **linking phase** that runs after all unit graphs are built.

## Data Flow

```
Input:
  - ProjectGraph.units: Vec<UnitGraph>  (each has root BlockId)
  - ProjectGraph.cc: &CompileCtxt
    - cc.block_arena: all BasicBlocks
    - cc.block_indexes: BlockId -> (unit_index, name, kind)
    - cc.arena: HirNodes, Symbols, Scopes

Output:
  - cc.related_map: populated with all relationships
    - BlockId -> HashMap<BlockRelation, Vec<BlockId>>
```

## Key Data Access Patterns

| Need | How to Access |
|------|---------------|
| Get block by ID | `unit.bb(block_id)` → `BasicBlock` |
| Get block's children | `block.children()` → `&[BlockId]` |
| Get block's HirNode | `block.node()` → `&HirNode` |
| Get Symbol from HirNode | `node.opt_symbol()` → `Option<&Symbol>` |
| Get Symbol's block_id | `symbol.block_id()` → `Option<BlockId>` |
| Find function by name | `cc.find_blocks_by_name(name)` |
| Add relationship | `cc.related_map.add_relation_impl(from, rel, to)` |

## Algorithm: Pre-order DFS per Unit (Parallel)

```
connect_blocks():
    // Process each unit in parallel (independent)
    units.par_iter().for_each(|unit|:
        let unit_ctx = CompileUnit { cc, index: unit.unit_index }
        let root_block = unit_ctx.bb(unit.root)

        // Pre-order DFS traversal starting from root
        dfs_connect(unit_ctx, root_block, None)
    )

dfs_connect(unit: CompileUnit, block: BasicBlock, parent: Option<BlockId>):
    let block_id = block.id()

    // 1. Link structural parent/child relationship
    if let Some(parent_id) = parent:
        cc.related_map.add_relation_impl(parent_id, Contains, block_id)
        cc.related_map.add_relation_impl(block_id, ContainedBy, parent_id)

    // 2. Link kind-specific relationships
    match block:
        BasicBlock::Func(func) =>
            link_func_relations(unit, block_id, func)
        BasicBlock::Class(class) =>
            link_class_relations(unit, block_id, class)
        BasicBlock::Impl(impl_block) =>
            link_impl_relations(unit, block_id, impl_block)
        BasicBlock::Trait(trait_block) =>
            link_trait_relations(unit, block_id, trait_block)
        BasicBlock::Call(call) =>
            link_call_relations(unit, block_id, call)
        BasicBlock::Field(field) =>
            link_field_relations(unit, block_id, field)
        _ => ()  // Root, Stmt, Enum, Const, Parameters, Return - no special linking

    // 3. Recurse into children (pre-order: visit this node before children)
    for child_id in block.children():
        let child = unit.bb(*child_id)
        dfs_connect(unit, child, Some(block_id))
```

## Kind-Specific Linking Functions

### `link_func_relations(unit, block_id, func)`

```
// Parameters
if let Some(params_id) = func.get_parameters():
    add(block_id, HasParameters, params_id)

// Return type
if let Some(ret_id) = func.get_returns():
    add(block_id, HasReturn, ret_id)

// Statements -> handled by structural (children)

// Calls -> Find call blocks in children, resolve callees
for call_id in find_calls_in_children(block):
    let call_block = unit.bb(call_id)
    if let Some(callee_id) = resolve_callee(call_block):
        add(block_id, Calls, callee_id)
        add(callee_id, CalledBy, block_id)
```

### `link_class_relations(unit, block_id, class)`

```
for field_id in class.get_fields():
    add(block_id, HasField, field_id)
    add(field_id, FieldOf, block_id)

for method_id in class.get_methods():
    add(block_id, HasMethod, method_id)
    add(method_id, MethodOf, block_id)
```

### `link_impl_relations(unit, block_id, impl_block)`

```
// Methods
for method_id in impl_block.get_methods():
    add(block_id, HasMethod, method_id)
    add(method_id, MethodOf, block_id)

// Target type (impl for SomeType)
if let Some(target_id) = resolve_impl_target(impl_block):
    add(block_id, ImplFor, target_id)
    add(target_id, HasImpl, block_id)

// Trait (impl SomeTrait for ...)
if let Some(trait_id) = resolve_impl_trait(impl_block):
    add(block_id, Implements, trait_id)
    add(trait_id, ImplementedBy, block_id)
```

### `link_trait_relations(unit, block_id, trait_block)`

```
for method_id in trait_block.get_methods():
    add(block_id, HasMethod, method_id)
    add(method_id, MethodOf, block_id)
```

### `link_call_relations(unit, block_id, call)`

```
// The call site itself - resolve what function it calls
// Use the HirNode's identifier symbol to find the target function
let node = call.base.node
if let Some(symbol) = node.ident_symbol(&unit):
    if let Some(target_block_id) = symbol.block_id():
        add(block_id, Calls, target_block_id)
        add(target_block_id, CalledBy, block_id)
```

### `link_field_relations(unit, block_id, field)`

```
// Field -> type relationship (Uses)
if let Some(type_symbol) = resolve_field_type(field):
    if let Some(type_block_id) = type_symbol.block_id():
        add(block_id, Uses, type_block_id)
        add(type_block_id, UsedBy, block_id)
```

## Symbol Resolution Strategy

**Important**: Symbol resolution is already done by `bind.rs` (in `llmcc-rust`) before `connect_blocks` runs!

The binding phase (`BinderVisitor`) walks the AST and:
1. Resolves identifiers to their defining `Symbol` via `ident.set_symbol(symbol)`
2. Sets up scope chains for lookup
3. Links type references (e.g., `fn_sym.set_type_of(return_type.id())`)

By the time we run `connect_blocks`, each `HirIdent` already has its resolved `Symbol` (if resolvable).

### What connect_blocks does (simplified)

Since symbols are pre-resolved, we just follow the links:

```rust
fn resolve_callee(unit: CompileUnit, call: &BlockCall) -> Option<BlockId> {
    // Symbols are already resolved by bind.rs!
    // Just follow: HirNode -> HirIdent -> Symbol -> BlockId
    call.base.node
        .ident_symbol(&unit)?     // Get the pre-resolved symbol
        .block_id()               // Get the block it belongs to
}
```

### Data flow (pre-existing from bind phase)

```
HirIdent.symbol ──────► Symbol (set by bind.rs)
                           │
                           ▼
                      Symbol.block_id ──► BlockId (set by graph_builder.rs)
```

### What we DON'T need to do

- ❌ Name-based lookup fallback (bind.rs already did this)
- ❌ Scope chain traversal (bind.rs already did this)
- ❌ Cross-unit symbol resolution (bind.rs already did this)

### What we DO need to do

- ✅ Walk blocks in pre-order DFS
- ✅ For each block, extract its relationships from block structure
- ✅ Follow pre-resolved symbol → block_id links
- ✅ Store relationships in `cc.related_map`

## Parallelization

- **Unit-level parallelism**: Each `UnitGraph` is processed independently using `rayon::par_iter()`
- **DashMap safety**: `cc.related_map` uses `DashMap` so concurrent writes from different units are safe
- **No cross-unit dependencies during traversal**: Each unit's DFS is independent

## Edge Cases

1. **Unresolved symbols**: Skip linking if symbol has no `block_id` (external crate, unresolved)
2. **Self-referential calls**: Recursion is valid, `add_relation_impl` handles duplicates
3. **Cyclic relationships**: Not a problem - we're building an edge list, not traversing cycles

## BlockRelation Types

Currently defined in `block.rs`:

```rust
pub enum BlockRelation {
    Unknown,

    // Structural Relations
    Contains,      // Parent contains child (Root→Func, Class→Method, etc.)
    ContainedBy,   // Child is contained by parent

    // Function/Method Relations
    HasParameters, // Func/Method → Parameters block
    HasReturn,     // Func/Method → Return block
    Calls,         // Func/Method → Func/Method it calls
    CalledBy,      // Func/Method is called by another

    // Type Relations
    HasField,      // Class/Enum → Field blocks
    FieldOf,       // Field → Class/Enum that owns it
    ImplFor,       // Impl → Type it implements for
    HasImpl,       // Type → Impl blocks for this type
    HasMethod,     // Impl/Trait → Method blocks
    MethodOf,      // Method → Impl/Trait/Class that owns it
    Implements,    // Type → Trait it implements
    ImplementedBy, // Trait → Types that implement it

    // Generic Reference
    Uses,          // Uses a type/const/function
    UsedBy,        // Is used by
}
```

## Implementation Order

1. Implement `connect_blocks` main loop with parallel unit iteration
2. Implement `dfs_connect` with structural linking
3. Add `link_func_relations` (most common case)
4. Add `link_call_relations` (for call graph)
5. Add `link_class_relations`, `link_impl_relations`, `link_trait_relations`
6. Add `link_field_relations` (type uses)

---

## Open Questions

### Q1: Helper Function Location
Should I put the helper functions (`link_func_relations`, etc.) as:
- free to do whatever for simplity

### Q2: Call Relationship Semantics
For the `Calls`/`CalledBy` relationship, should it be:
- Both

### Q3: Incremental vs All-at-once
Do you want all linking types implemented in one go, or start minimal (structural + calls) first?
- ALL

### Q4: Related Map vs Block-local Storage
uses `cc.related_map`

### Q5: Bidirectional Always?
Currently planning to always store both directions (e.g., both `Calls` and `CalledBy`). Should we:
- Always store both.
