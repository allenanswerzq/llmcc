# llmcc Pipeline Design Notes

This note records the design direction we want to preserve while improving llmcc. It is based on the current crate architecture and lessons from reviewing `cocoindex-code` and `codebase-memory-mcp` locally under `learn/`.

The short version:

```text
discover -> collect definitions -> bind/resolve -> build graph -> query/render
```

The important part is not the exact names of the functions. The important part is that each phase has a clear contract, a clear data shape, and a small number of responsibilities.

## 1. Keep The Pipeline Explicit

llmcc should keep the indexing and graph-building flow visible as a pipeline, not hide it inside one large visitor or one large language crate.

The preferred flow is:

```text
file discovery
  -> HIR build
  -> symbol collection
  -> binding / resolution
  -> graph build
  -> graph query / render
```

### File Discovery

Discovery answers: what source files are part of this run?

It should own:

- selecting files by language and extension
- respecting user CLI options
- excluding irrelevant directories and generated output
- producing deterministic file order for tests
- assigning package/module/file metadata before language passes run

It should not own:

- parsing language syntax
- resolving symbols
- creating graph edges
- deciding semantic relationships

Keeping discovery independent makes it possible to improve project layout handling, package grouping, and incremental indexing without touching language visitors.

### HIR Build

HIR build answers: what is the compact tree shape we will analyze?

It should own:

- converting tree-sitter syntax trees into llmcc HIR nodes
- preserving relevant tree-sitter kind and field ids
- applying language build policy such as skipping test-only syntax when needed
- keeping enough source position data for graph blocks and diagnostics

It should not own:

- symbol resolution
- type inference
- graph dependency policy

HIR should stay a stable internal representation. Language crates can interpret it, but should not make callers inspect raw tree-sitter details everywhere.

### Symbol Collection

Collection answers: what symbols and scopes exist?

This phase should discover facts, not resolve references.

It should own:

- declaring packages, modules, files, functions, classes, structs, fields, aliases, type parameters, constants, etc.
- assigning symbol kinds
- creating semantic scopes
- attaching collected symbols to declaration identifiers
- publishing globally visible symbols when the language requires it
- recording ownership facts that are known at declaration time

It should not own:

- choosing which same-name symbol a reference points to
- inferring expression types from usage
- producing graph edges

This split matters because collection can run before every reference is known. It gives binding a complete symbol universe to search.

### Binding / Resolution

Binding answers: what does this reference mean?

It should own:

- resolving identifiers to symbols
- resolving qualified paths
- resolving fields and members through receiver types
- assigning declared types to fields, parameters, returns, variables, and aliases
- resolving aliases and language-specific self/this symbols
- recording type dependencies that are meaningful only after resolution

It should not own:

- creating graph blocks
- deciding output rendering format
- walking the filesystem

Binding should be allowed to use structured helper APIs such as `BindCtxt::lookup_symbol`, `lookup_path_symbol`, and `lookup_member`. It should avoid ad hoc lookup loops unless a language-specific rule genuinely needs one.

### Graph Build

Graph build answers: what blocks and relations should downstream tools see?

It should own:

- materializing HIR nodes into graph blocks
- attaching block ids to symbols
- converting resolved symbol/type metadata into relations
- linking structural ownership relations such as contains, fields, parameters, returns, and methods
- linking semantic relations such as calls, type references, inheritance, implementations, and type dependencies

It should not own:

- parsing raw source
- resolving unresolved names by string search
- language-specific fallback rules that belong in binding

Graph build should consume resolved facts, not rediscover them.

### Query / Render

Query and render answer: how should users or agents consume the graph?

They should own:

- graph traversal
- depth grouping
- PageRank and filtering
- architecture summaries
- DOT rendering
- future query APIs over project graphs

They should not own:

- language-specific symbol resolution
- mutating symbols or scopes

The graph should be useful without DOT. DOT is one presentation; query APIs should become first-class.

## 2. Preserve The Split Between Extraction Facts And Resolved Edges

There are two different kinds of information:

- extraction facts: things directly present in source
- resolved edges: semantic relationships inferred after lookup

Examples of extraction facts:

- function `process` exists
- class `UserService` has a field named `repo`
- import path `models::User` appears
- call syntax `helper()` appears
- alias `UserAlias` is declared

Examples of resolved edges:

- `process` calls `helper` in another file
- field `repo` has type `repository::UserRepository`
- alias `UserAlias` targets `models::User`
- class `Derived` extends `Base`
- module `services` depends on module `models`

The collection phase should capture extraction facts. The binding and graph phases should create resolved edges.

This design keeps the system debuggable:

- If a symbol is missing, inspect collection.
- If a symbol exists but a reference points nowhere, inspect binding.
- If binding is correct but the graph is wrong, inspect graph build/query.

It also keeps parallelism safer. Collection can publish global symbol tables first. Binding can then run per unit with less risk of creating new cross-unit global state halfway through resolution.

## 3. Add Focused Passes Instead Of Stuffing Everything Into Visitors

Language visitors are necessary, but they should not become the only place where every feature lives.

Good visitor responsibilities:

- collect language-specific declarations
- enter language-specific scopes
- bind language-specific identifier forms
- infer language-specific type syntax

Bad visitor responsibilities:

- deciding global graph rendering policy
- running architecture analysis
- doing cross-repo or cross-package summaries
- mixing unrelated feature passes into one traversal

When a feature has its own input and output shape, prefer a focused pass.

Examples:

- call resolution pass
- type dependency pass
- route extraction pass
- import/package map pass
- graph enrichment pass
- dead-code or reachability query pass

Focused passes are easier to test because each pass can have a small contract:

```text
input: resolved project graph
output: extra CALLS edges
```

or:

```text
input: package metadata + imports
output: package/module dependency edges
```

This style also makes it easier to add language features without destabilizing unrelated parts of the pipeline.

## 4. Use A Registry Or Index For Cross-File And Cross-Package Resolution

Cross-file resolution should not be a repeated ad hoc search through all symbols.

The project should maintain indexes that answer questions quickly and consistently:

- which symbols exist by name and kind?
- which symbols are visible from this unit/package/module?
- which modules belong to which package?
- which file owns which scope?
- which aliases point to which target?
- which type owns this member name?

The current `CompileCtxt`, symbol arenas, scopes, and resolver contexts are already moving in this direction. We should keep strengthening that model rather than adding caller-side normalization or repeated string searches.

The registry/index layer should support:

- deterministic same-name resolution
- current-file preference
- current-package preference
- global fallback when a language allows it
- alias and type-chain resolution
- member lookup through owned scopes
- structured qualified-path lookup

This makes language crates simpler. A language binder should say:

```text
resolve this path with these symbol kinds
```

not:

```text
walk every global symbol, split strings, compare segments, then guess
```

## 5. Keep Graph Queries As A First-Class API

DOT output is valuable, but it should not be the only serious interface.

Agents and tools need structured answers:

- what calls this function?
- what types does this function consume and return?
- what modules depend on this module?
- what are the top architectural hubs?
- what changed if this symbol changes?
- what symbols are unreachable?
- what path connects A to B?

Graph query APIs should live close to `ProjectGraph` and `GraphQuery`, not be reconstructed in CLI/rendering code.

Good direction:

- `ProjectGraph::query()` remains the main traversal surface
- downstream crates use query helpers instead of raw relation maps
- renderers consume query results
- CLI/MCP/server layers can expose structured query endpoints later

This gives llmcc room to grow beyond static graph screenshots. The architecture graph can become an interactive knowledge model.

## 6. Make Known Limits Explicit Per Language

Every language crate should document what it supports and what it intentionally does not support yet.

This is not an apology. It is part of a production contract.

For example, the C++ crate should say clearly:

- qualified namespace lookup is supported for common cases
- aliases are resolved for graph type dependencies
- overload handling is structural, not compiler-grade
- templates are parsed structurally but not instantiated
- preprocessor and compile database semantics are not fully modeled

Rust, TypeScript, and future languages should have equivalent notes.

Each language support note should include:

- supported declarations
- supported reference forms
- type inference coverage
- graph edges produced
- known unsupported syntax or semantic limits
- fixture files that lock expected behavior

This helps users trust the output and helps contributors know where to improve next.

## 7. Practical Rules For Future Changes

When adding a feature, ask these questions first:

1. Is this a discovery problem, a collection problem, a binding problem, a graph problem, or a query problem?
2. Can the feature be expressed as a focused pass instead of expanding a visitor?
3. Is there an existing registry/scope/query API that should own the lookup?
4. Does the feature add extraction facts, resolved edges, or both?
5. What fixture proves the behavior at the right layer?
6. What known limit should be documented if the implementation is intentionally partial?

The goal is not to make the pipeline rigid. The goal is to keep changes easy to reason about.

## 8. Relation To Reviewed Repositories

From `cocoindex-code`, llmcc should learn product ergonomics:

- clear CLI commands
- daemon/client split for repeated use
- status and doctor-style diagnostics
- simple MCP wrapping around the same core API

From `codebase-memory-mcp`, llmcc should learn graph-engine structure:

- explicit multi-pass indexing
- extraction before resolution
- registry-backed cross-file lookup
- graph queries as a product surface
- documented language limits

llmcc should not copy either design wholesale. The right direction is:

```text
typed Rust core
explicit pipeline phases
language-specific extraction and binding
shared graph/query APIs
clear CLI and future MCP ergonomics
```

That keeps the implementation understandable while still moving toward production-grade code intelligence.