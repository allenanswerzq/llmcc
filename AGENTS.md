# claude.md

**tradeoff:** these guidelines bias toward caution over speed. for trivial tasks, use judgment.

## 1. think before coding

**don't assume. don't hide confusion. surface tradeoffs.**

before implementing:
- state your assumptions explicitly. if uncertain, ask.
- if multiple interpretations exist, present them - don't pick silently.
- if a simpler approach exists, say so. push back when warranted.
- if something is unclear, stop. name what's confusing. ask.

## 2. simplicity first

**minimum code that solves the problem. nothing speculative.**

- no features beyond what was asked.
- no abstractions for single-use code.
- no "flexibility" or "configurability" that wasn't requested.
- no error handling for impossible scenarios.
- simple and effective is the default. do not add clever helper concepts, transitional abstractions, or convenience apis when the domain can be represented directly.
- llmcc's goal is an architecture map for llm agents, not compiler-level type precision. prioritize important project entities and relationships: packages, files, modules, classes, structs, traits/interfaces, functions/methods, imports/exports, calls, inheritance/implements, and meaningful type/use edges. keep primitive details like `int`, `string`, `bool`, qualifiers, numeric suffixes, and exact overload trivia local and minimal; only improve them when a concrete graph-level behavior depends on it.
- this project is new: do not preserve backward compatibility by default. prefer clean breaking changes over compatibility wrappers, legacy aliases, duplicate old apis, or old string escape hatches unless explicitly requested.
- when a type is refactored into a real domain model, finish the whole cleanup: remove obsolete constructors, duplicate string methods, old aliases, custom serde, raw-string escape paths, and caller-side normalization helpers.
- do not make callers remember invariant steps. constructors/parsers should normalize and validate once, then the stored value should already be canonical.
- do not use `panic!`, `assert!`, or `unwrap()` in domain/model accessors to enforce data invariants. normalize or validate at constructors/parsers/boundaries, or return `result`/`option` when invalid data is genuinely possible.
- do not implement domain accessors by parsing stored strings and panicking on failure; store a typed canonical value or return a `result` from the boundary that can fail.
- names must have one obvious meaning. do not add a second method just to preserve the old meaning; use an existing domain field or remove the old concept when it is no longer needed.
- prefer language/library derives over hand-written boilerplate: use `serde` for json shapes and `strum` for enum string conversions.
- use idiomatic rust conversion traits: implement `from`/`tryfrom` for one-to-one conversions instead of ad hoc `into_core`/`to_core` methods. use a named method only when conversion needs extra context or returns a partial/domain fragment.
- prefer typed enums and small domain types over repeated raw string constants. keep raw api/provider strings isolated at boundaries.
- do not add dtos or wrapper helpers if `serde` attributes (`skip`, `rename`, custom enum `serialize`) can express the output directly.
- tests should call domain apis that express the invariant under test. do not make tests inspect storage representation with ad hoc iterator/filter/count logic; if a recurring assertion needs that, add a small domain method first.
- do not add trivial tests that only restate constructors, derives, field assignment, or obvious pass-through behavior. add tests only when they protect a real invariant, regression, parsing/normalization rule, graph behavior, or published contract.
- tests should not read public model fields directly when a domain accessor can express the assertion; add the accessor and test through it.
- tests should not use `.as_ref()`/`.as_deref()` to reach through optional public fields when a domain accessor would express the api better; add the accessor and test through it.
- tests should not use partial string assertions like `.contains(...)` for stable expected strings; assert the full expected value so failures show the exact contract.
- keep useful comments, not noisy comments: explain intent, invariants, and boundaries; do not delete all comments just to be concise.
- if you write 200 lines and it could be 50, rewrite it.

ask yourself: "would a senior engineer say this is overcomplicated?" if yes, simplify.

## 3. surgical changes

**touch only what you must. clean up only your own mess.**

when editing existing code:
- don't "improve" adjacent code, comments, or formatting.
- don't refactor things that aren't broken.
- match existing style, even if you'd do it differently.
- if you notice unrelated dead code, mention it - don't delete it.

when your changes create orphans:
- remove imports/variables/functions that your changes made unused.
- don't remove pre-existing dead code unless asked.
- after refactors, do a cleanup pass for now-redundant adapters, compatibility wrappers, manual conversion helpers, and duplicated string tables.
- remove legacy apis introduced only for compatibility; do not keep deprecated wrappers around unless the user explicitly asks for a migration window.
- before reporting a refactor as done, reread the entire touched type/module and remove the same smell everywhere in that scope; do not wait for the user to point out each duplicate.

refactor shape:
- split large files by responsibility only when it reduces cognitive load: schema/types, transport, normalization, indexing, and public api are good boundaries.
- keep conversion flow obvious: raw input -> typed internal representation -> serialized public output.
- public output structs should match the published columns/api contract; avoid storing fields just to skip them during serialization.

the test: every changed line should trace directly to the user's request.

## 4. goal-driven execution

**define success criteria. loop until verified.**

transform tasks into verifiable goals:
- "add validation" → "write tests for invalid inputs, then make them pass"
- "fix the bug" → "write a test that reproduces it, then make it pass"
- "refactor x" → "ensure tests pass before and after"
- "clean up rust refactor" → "run `cargo fmt`, `cargo check`, and preferably `cargo clippy --all-targets -- -d warnings` for touched crates"

for multi-step tasks, state a brief plan:
```
1. [step] → verify: [check]
2. [step] → verify: [check]
3. [step] → verify: [check]
```

strong success criteria let you loop independently. weak criteria ("make it work") require constant clarification.

---

**these guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.