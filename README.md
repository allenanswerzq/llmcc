# llmcc

*"Prompts are the modern assembly language, models are the modern CPU."*

llmcc is a universal context builder for any language, any document.

## abstract

llmcc explores automated context generation through symbolic graph analysis. bridging the semantic gap between human-written code/documents and AI model understanding, using modern compiler design principles.

## design

![design](doc/design.svg)

## run

eg. build a project level graph using pagerank pick the most important 100 nodes, this will give a pretty much high level architecutre design for a project

```cargo run --release -- --dir ../codex/codex-rs --project-graph --pagerank --top-k 100```


eg. find all depends of symbol `Codex` under codex-rs/core folder

```cargo run -- --dir ../codex/codex-rs/core --query Codex --recursive```
