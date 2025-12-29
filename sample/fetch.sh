#!/bin/bash
# Fetch all sample repos (shallow clone)
# Can be run from anywhere

set -e

# Get absolute path to script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPOS_DIR="$SCRIPT_DIR/repos"

# Create repos directory if it doesn't exist
mkdir -p "$REPOS_DIR"
cd "$REPOS_DIR"

REPOS=(
    # Core ecosystem
    "BurntSushi/ripgrep"
    "tokio-rs/tokio"
    "serde-rs/serde"
    "clap-rs/clap"
    "tokio-rs/axum"
    "openai/codex"
    "allenanswerzq/llmcc"
    "astral-sh/ruff"
    # Database & data infrastructure
    "lancedb/lancedb"
    "lance-format/lance"
    "apache/opendal"
    "risingwavelabs/risingwave"
    "databendlabs/databend"
    "apache/datafusion"
    "qdrant/qdrant"
)

echo "Fetching sample repositories..."

for repo in "${REPOS[@]}"; do
    name=$(basename "$repo")
    if [ ! -d "$REPOS_DIR/$name" ]; then
        echo "Cloning $repo..."
        git clone --depth 1 "https://github.com/$repo.git" "$REPOS_DIR/$name"
    else
        echo "Skipping $name (already exists)"
    fi
done

echo ""
echo "Done! Fetched repos:"
ls -la "$REPOS_DIR"
