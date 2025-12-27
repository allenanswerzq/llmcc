# Design: Replace BlockParameters with BlockParameter

## Goal

Change the block structure from a single `BlockParameters` container to individual `BlockParameter` blocks, with each parameter being a direct child of the function.

## Key Insight: No BlockType Needed

For complex types (structs, enums, traits), the `type_ref` field connects directly to the **defining block** (e.g., `BlockClass` for a struct). This happens during `connect_blocks` phase via symbol resolution.

For primitive types (`i32`, `bool`, `str`, etc.), we just store the type name as a string - no block reference needed.

### Example 1: Regular Function

```rust
struct ComplexType<T> {
    y: i32,
}

fn add(x: i32, t: ComplexType<T>) -> i32 {
    return x + y;
}
```

**Block Graph Output:**
```
(root:1 lib
  (class:2 ComplexType
    (field:3 y)
  )
  (func:4 add
    (parameter:5 x) @type i32
    (parameter:6 t) @type:2 ComplexType
    (return:7) @type i32
  )
)
```

### Example 2: Method with Self Parameter

```rust
struct Point {
    x: i32,
    y: i32,
}

impl Point {
    fn distance(&self, other: &Point) -> i32 {
        // ...
    }
}
```

**Block Graph Output:**
```
(root:1 lib
  (class:2 Point
    (field:3 x)
    (field:4 y)
  )
  (impl:5 Point
    (func:6 distance
      (parameter:7 self) @type:2 &Point    // Self resolved to Point (class:2)
      (parameter:8 other) @type:2 &Point   // Also references class:2
      (return:9) @type i32
    )
  )
)
```

**Key insight:** `&self` is resolved to `&Point` (the concrete type), not `&Self`. The symbol binding process knows which struct/enum the `impl` block belongs to, so we can resolve `Self` to the actual type.

**Key observations:**
- `@type i32` - primitive type, just the name (no block reference)
- `@type:2 ComplexType` - complex type, includes block ID reference to `class:2`
- `@type:2 &Point` - reference to complex type, still references the defining block

---

## Data Model

### BlockParameter

```rust
pub struct BlockParameter<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
    /// For complex types: BlockId of the defining block (class/enum/trait)
    /// For primitive types: None (just use type_name)
    pub type_ref: RwLock<Option<BlockId>>,
    /// The type name as it appears in source (e.g., "i32", "ComplexType<T>")
    pub type_name: RwLock<String>,
}
```

### BlockReturn (enhanced)

```rust
pub struct BlockReturn<'blk> {
    pub base: BlockBase<'blk>,
    /// For complex types: BlockId of the defining block (class/enum/trait)
    /// For primitive types: None
    pub type_ref: RwLock<Option<BlockId>>,
    /// The type name as it appears in source (e.g., "i32", "Option<T>")
    pub type_name: RwLock<String>,
}
```

**Note:** For `BlockReturn`, the type info comes from:
1. The function's symbol has a return type (via `type_of` or similar)
2. Or we extract the type name directly from the return_type AST node's text
3. Then look up the type symbol to get `block_id` for complex types

### BlockFunc (modified)

```rust
pub struct BlockFunc<'blk> {
    pub base: BlockBase<'blk>,
    pub name: String,
    // Changed from Option<BlockId> (single Parameters block) to Vec<BlockId> (list of Parameter blocks)
    pub parameters: RwLock<Vec<BlockId>>,
    pub returns: RwLock<Option<BlockId>>,
    pub stmts: RwLock<Vec<BlockId>>,
}
```

---

## Output Format

### Display Rules

1. **Primitive types** (built-in, no defining block):
   ```
   (parameter:5 x) @type i32
   ```

2. **Complex types** (user-defined, has defining block):
   ```
   (parameter:6 t) @type:2 ComplexType
   ```
   - `:2` is the BlockId of the defining block (`class:2 ComplexType`)

3. **Return type**:
   ```
   (return:7) @type i32
   (return:7) @type:2 ComplexType
   ```

---

## Implementation Phases

### Phase 1: Add BlockParameter (keep BlockParameters temporarily)

1. Add `BlockParameter` struct to `block.rs`
2. Add `Parameter` to `BlockKind` enum
3. Add `Parameter` variant to `BasicBlock` enum
4. Add `blk_parameter` to `BlockArena`
5. Update `token_map.toml`: add `block_kind = "Parameter"` for `parameter` node

### Phase 2: Modify BlockFunc

1. Change `parameters: Option<BlockId>` → `parameters: RwLock<Vec<BlockId>>`
2. Add `add_parameter(id: BlockId)` method
3. Add `parameters() -> Vec<BlockId>` getter

### Phase 3: Graph Builder Updates

1. Add `BlockKind::Parameter` case to `create_block()`
2. Update `populate_block_fields()` for `BlockKind::Parameter` children of Func
3. Extract type name from AST node during block creation

### Phase 4: Type Resolution via Symbol Binding

The symbol binding process happens **before** graph building. The symbol chain works as:

```
Parameter's HirNode
  └── symbol (SymId) ──┐
                       v
              Parameter's Symbol
                  └── type_of: Option<SymId> ──┐
                                               v
                                    Type's Symbol (e.g., "ComplexType")
                                        ├── name: "ComplexType"
                                        └── block_id: Option<BlockId> ──> class:2 (the defining block)
```

When building a `BlockParameter`, we follow this chain:

```rust
impl<'blk> BlockParameter<'blk> {
    pub fn new(
        id: BlockId,
        node: HirNode<'blk>,
        parent: Option<BlockId>,
        children: Vec<BlockId>,
        unit: CompileUnit<'blk>,  // Need access to symbol table
    ) -> Self {
        let base = BlockBase::new(id, node, BlockKind::Parameter, parent, children);
        let name = base.opt_get_name().unwrap_or("").to_string();

        // Get type info by following the symbol chain
        let (type_name, type_ref) = if let Some(param_sym) = node.opt_symbol() {
            // param_sym.type_of() returns Option<SymId> - the TYPE symbol
            if let Some(type_sym_id) = param_sym.type_of() {
                if let Some(type_sym) = unit.cc.symbols.get(type_sym_id) {
                    // Get the type name from the type symbol
                    let type_name = unit.cc.interner.resolve_owned(type_sym.name)
                        .unwrap_or_default();
                    // Get the block_id of the type's defining block (if it exists)
                    let type_ref = type_sym.block_id();
                    (type_name, type_ref)
                } else {
                    (String::new(), None)
                }
            } else {
                (String::new(), None)
            }
        } else {
            (String::new(), None)
        };

        Self {
            base,
            name,
            type_name: RwLock::new(type_name),
            type_ref: RwLock::new(type_ref),
        }
    }
}
```

**Key insight:**
- `param_sym.type_of()` → `SymId` of the **type symbol** (not a block!)
- `type_sym.block_id()` → `BlockId` of the **defining block** (for complex types)
- For primitives like `i32`, `type_sym.block_id()` will be `None`
- For `Self` type (in impl blocks), the symbol binding has already resolved it to the concrete type (e.g., `Point`), so we get the actual struct/enum's block_id

**Self resolution:** The binding process handles `Self` specially:
- In `impl Point { fn foo(&self) }`, the `self` parameter's type symbol is already resolved to `Point`
- We don't see "Self" in the output - we see the concrete type name
- The `type_ref` points to `class:2` (the Point struct's block)

### Phase 5: Output Formatting

Update `format_block()` to display `@type` with optional block reference:

```rust
pub fn format_block(&self, unit: CompileUnit<'blk>) -> String {
    // ... base formatting ...

    if let BasicBlock::Parameter(param) = self {
        let type_name = param.type_name();
        if let Some(type_id) = param.get_type_ref() {
            // Complex type: show block reference
            return format!("{}:{} {} @type:{} {}", kind, block_id, name, type_id, type_name);
        } else if !type_name.is_empty() {
            // Primitive type: just show name
            return format!("{}:{} {} @type {}", kind, block_id, name, type_name);
        }
    }
    // ...
}
```

### Phase 6: Cleanup

1. Remove `BlockParameters` struct
2. Remove `Parameters` from `BlockKind`
3. Remove `Parameters` variant from `BasicBlock`
4. Remove `blk_parameters` from `BlockArena`
5. Remove old `Parameter` helper struct
6. Remove `block_kind = "Parameters"` from `token_map.toml`

---

## Changes Summary

| File | Add | Remove | Modify |
|------|-----|--------|--------|
| `block.rs` | `BlockParameter` | `BlockParameters`, `Parameter` helper | `BlockKind`, `BasicBlock`, `BlockFunc`, `BlockReturn` |
| `graph_builder.rs` | `Parameter` case | `Parameters` case | `create_block()`, `populate_block_fields()` |
| `token_map.toml` | `parameter` block_kind | `parameters` block_kind | - |
| `connect.rs` (or equivalent) | type resolution logic | - | - |
| Test files | - | - | Update expected output format |

---

## Primitive Types List

Types that are primitive (no block reference needed):
- Numeric: `i8`, `i16`, `i32`, `i64`, `i128`, `isize`, `u8`, `u16`, `u32`, `u64`, `u128`, `usize`, `f32`, `f64`
- Boolean: `bool`
- Character: `char`
- String: `str`, `String` (String is technically std lib, but treat as primitive)
- Unit: `()`
- References to primitives: `&i32`, `&mut bool`, etc.

All other types should have a defining block in the codebase.

---

## Remove BlockType

Since we're not creating separate `BlockType` blocks, we should also remove:
- `BlockType` struct from `block.rs`
- `Type` from `BlockKind`
- `Type` variant from `BasicBlock`
- `blk_type` from `BlockArena`
- `BlockKind::Type` case from `graph_builder.rs`

The type information is fully captured by:
1. `type_name: String` - the textual representation
2. `type_ref: Option<BlockId>` - link to defining block (for complex types)
