# llmcc-test

JSON-backed graph test runner for llmcc.

The old `.llmcc` text corpus backend has been removed from this crate. Test suites are JSON files that describe virtual source files, the target language, the desired graph depth, and the expected `llmcc-format` graph document.

## Suite Shape

```json
{
  "schema": "llmcc.test",
  "schema_version": 1,
  "cases": [
    {
      "id": "cpp-call-json",
      "language": "cpp",
      "depth": "file",
      "files": [
        {
          "path": "sample/src/lib.cpp",
          "contents": r#"
              void helper() {}
              void run() { helper(); }
            "#
        }
      ],
      "expect": {
        "schema": "llmcc.graph",
        "schema_version": 1,
        "depth": "file",
        "nodes": [],
        "edges": []
      }
    }
  ]
}
```

`expect` may be omitted when creating a new case. Run with `--update` to bless the current `llmcc-format` graph document.

## Commands

```powershell
cargo run -p llmcc-test -- run tests\json
cargo run -p llmcc-test -- run tests\json\cpp_call.json --update
cargo run -p llmcc-test -- list tests\json
```
