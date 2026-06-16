# llmcc-cpp

C/C++ language support for llmcc architecture and dependency graphs.

This crate is a production best-effort analyzer for graph extraction, not a C++ compiler frontend. It uses tree-sitter syntax plus llmcc symbol collection, binding, and type inference to recover project structure and common type dependencies.

## Supported Semantics

- Translation-unit discovery for common C/C++ source and header extensions.
- Global, file, namespace, class, struct, union, enum, function, method, field, parameter, variable, and alias symbols.
- Cross-file qualified namespace lookup for paths such as `models::User`.
- Basic inheritance edges from class/struct base clauses, including qualified base names.
- Field, parameter, and return type binding for primitive, identifier, qualified, and common wrapper type nodes.
- Type aliases from `using Alias = Type` and `typedef Type Alias`, including alias-chain resolution for graph type references.
- Structural overload separation for same-name free functions by parameter signature, with call binding that prefers matching arity and exact primitive argument types.
- Member lookup through resolved receiver types for ordinary field/method expressions.
- Architecture graph dependencies through direct qualified types and alias-mediated types.

## Intentional Limits

- No full preprocessor, macro expansion, include-path, or `compile_commands.json` semantics.
- No compiler-grade overload resolution, implicit conversions, ADL, access checking, or SFINAE. Current overload handling is structural: arity plus exact primitive type matches, without conversion ranking.
- Template support is structural and best-effort; it does not instantiate templates or fully evaluate dependent names.
- Standard-library types are resolved only when symbols are available in the analyzed input or built-in primitives.
- Results are designed for architecture/dependency graphs, not for proving C++ type correctness.

When correctness requires exact C++ compiler semantics, integrate clang tooling and use llmcc output as a graph-oriented projection of that semantic data.