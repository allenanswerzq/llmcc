# npm Package Distribution

This directory contains the npm package configuration for distributing `llmcc` via npm.

## Installation

```bash
npm install -g llmcc
```

## How It Works

1. On `npm install`, the postinstall script downloads the pre-built binary for your platform from GitHub releases
2. The shell wrapper (`bin/llmcc`) detects your OS/architecture and executes the correct binary

## Package Structure

```
npm/
├── package.json          # Main package configuration
├── bin/
│   ├── llmcc            # Shell wrapper (Unix)
│   └── llmcc.cmd        # Batch wrapper (Windows)
├── scripts/
│   └── postinstall.js   # Downloads binary from GitHub releases
└── README.md
```

## Supported Platforms

| Platform | Binary Name |
|----------|-------------|
| macOS ARM64 (Apple Silicon) | llmcc-darwin-arm64 |
| macOS x64 (Intel) | llmcc-darwin-x64 |
| Linux ARM64 | llmcc-linux-arm64 |
| Linux x64 | llmcc-linux-x64 |
| Windows x64 | llmcc-win32-x64.exe |

## Publishing

### Prerequisites
1. Build binaries for all platforms (via GitHub Actions)
2. Create a GitHub release with binaries attached
3. npm token configured

### Release Process

1. Update version in `Cargo.toml` and `npm/package.json`
2. Build binaries: `just npm-build`
3. Create GitHub release `v0.2.51` with binaries attached
4. Publish to npm:
   ```bash
   cd npm
   npm publish
   ```

### GitHub Actions (Automated)

The workflow at `.github/workflows/npm-publish.yml` will:
1. Build binaries for all platforms
2. Create GitHub release with binaries
3. Publish to npm

## Local Development

```bash
# Build for current platform
cargo build --release

# Copy binary to npm/bin for testing
cp target/release/llmcc npm/bin/llmcc-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m | sed 's/x86_64/x64/' | sed 's/aarch64/arm64/')

# Test locally
cd npm && npm link
llmcc --help
```
