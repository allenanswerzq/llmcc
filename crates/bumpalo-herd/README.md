# llmcc-bumpalo

Thread-safe bumpalo wrapper with pre-allocation support.

## Origin

This is a fork of [bumpalo-herd](https://crates.io/crates/bumpalo-herd) with configurable initial chunk sizes to reduce malloc pressure in high-throughput scenarios.

## Key Differences from bumpalo-herd

- Configurable initial chunk size per `Herd` (default: 16MB vs original 1MB)
- `Herd::with_chunk_size()` constructor for custom allocation sizes
- Optimized for llmcc's code parsing workloads

## Usage

```rust
use llmcc_bumpalo::Herd;

// Default 16MB chunks
let herd = Herd::new();

// Custom 4MB chunks
let herd = Herd::with_chunk_size(4 * 1024 * 1024);
```

## License

Apache-2.0 (same as original bumpalo-herd)
