#!/bin/bash
set -euo pipefail

# Build llmcc binary for current platform and copy to npm/bin
# For cross-platform builds, use GitHub Actions

VERSION=$(grep '^version = ' Cargo.toml | head -1 | cut -d'"' -f2)
echo "Building llmcc v$VERSION for npm distribution"

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$OS" in darwin) OS="darwin" ;; linux) OS="linux" ;; mingw*|msys*|cygwin*) OS="win32" ;; esac
case "$ARCH" in x86_64|amd64) ARCH="x64" ;; aarch64|arm64) ARCH="arm64" ;; esac

BINARY_NAME="llmcc-${OS}-${ARCH}"
if [ "$OS" = "win32" ]; then
    BINARY_NAME="${BINARY_NAME}.exe"
fi

echo "Platform: ${OS}-${ARCH}"
echo "Binary: ${BINARY_NAME}"
echo ""

# Build
echo "Building release binary..."
cargo build --release

# Copy to npm/bin
mkdir -p npm/bin
if [ "$OS" = "win32" ]; then
    cp target/release/llmcc.exe "npm/bin/${BINARY_NAME}"
else
    cp target/release/llmcc "npm/bin/${BINARY_NAME}"
    chmod +x "npm/bin/${BINARY_NAME}"
fi

echo ""
echo "✓ Build complete!"
echo "  Binary: npm/bin/${BINARY_NAME}"
echo ""
echo "To test locally:"
echo "  cd npm && npm link"
echo "  llmcc --help"
echo ""
echo "To publish:"
echo "  1. Create GitHub release v${VERSION} with binaries for all platforms"
echo "  2. cd npm && npm publish"
