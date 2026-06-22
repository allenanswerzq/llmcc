# llmcc-java

Java language support for llmcc.

This crate follows the same shape as the existing language crates: a tree-sitter parser-backed token map, local primitive names, collection, binding, and graph construction through `llmcc-core` primitives.

The first pass is architecture-focused. It collects classes, records, interfaces, enums, methods, and method invocation targets so `llmcc-format` can produce useful project maps for agents.
