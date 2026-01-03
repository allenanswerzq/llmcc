#!/bin/bash
# Shared project definitions for llmcc sample scripts
# Source this file from benchmark.sh and generate_all.sh

# Requires SCRIPT_DIR to be set before sourcing
if [ -z "$SCRIPT_DIR" ]; then
    echo "Error: SCRIPT_DIR must be set before sourcing projects.sh"
    exit 1
fi

# Projects to process: name -> source directory
declare -A PROJECTS=(
    # Core ecosystem
    ["ripgrep"]="$SCRIPT_DIR/repos/ripgrep"
    ["tokio"]="$SCRIPT_DIR/repos/tokio"
    ["serde"]="$SCRIPT_DIR/repos/serde"
    ["clap"]="$SCRIPT_DIR/repos/clap"
    ["axum"]="$SCRIPT_DIR/repos/axum"
    ["ruff"]="$SCRIPT_DIR/repos/ruff"
    ["codex"]="$SCRIPT_DIR/repos/codex"
    ["llmcc"]="$SCRIPT_DIR/repos/llmcc"
    # ML & AI
    ["candle"]="$SCRIPT_DIR/repos/candle"
    # Developer tools
    ["rust-analyzer"]="$SCRIPT_DIR/repos/rust-analyzer"
    # Database & data infrastructure
    ["lancedb"]="$SCRIPT_DIR/repos/lancedb"
    ["lance"]="$SCRIPT_DIR/repos/lance"
    ["opendal"]="$SCRIPT_DIR/repos/opendal"
    ["risingwave"]="$SCRIPT_DIR/repos/risingwave"
    ["databend"]="$SCRIPT_DIR/repos/databend"
    ["datafusion"]="$SCRIPT_DIR/repos/datafusion"
    ["qdrant"]="$SCRIPT_DIR/repos/qdrant"
)
