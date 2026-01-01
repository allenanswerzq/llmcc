#!/bin/bash
# Setup script for SWE-bench integration
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "=== SWE-bench Integration Setup ==="
echo ""

# Check Python version
python3 --version || { echo "Error: Python 3 required"; exit 1; }

# Create virtual environment if not exists
if [ ! -d "$SCRIPT_DIR/.venv" ]; then
    echo "Creating virtual environment..."
    python3 -m venv "$SCRIPT_DIR/.venv"
fi

# Activate virtual environment
source "$SCRIPT_DIR/.venv/bin/activate"

# Upgrade pip
pip install --upgrade pip

# Install dependencies
echo "Installing dependencies..."
pip install -r "$SCRIPT_DIR/requirements.txt"

# Check Docker
if ! command -v docker &> /dev/null; then
    echo "Warning: Docker not found. SWE-bench evaluation requires Docker."
    echo "Install: https://docs.docker.com/engine/install/"
fi

# Build llmcc if not already built
if [ ! -x "$PROJECT_ROOT/target/release/llmcc" ]; then
    echo "Building llmcc..."
    cd "$PROJECT_ROOT"
    cargo build --release
fi

# Create output directories
mkdir -p "$SCRIPT_DIR/results"
mkdir -p "$SCRIPT_DIR/logs"
mkdir -p "$SCRIPT_DIR/cache"

# Download SWE-bench Multilingual Rust subset info
echo "Fetching SWE-bench Multilingual dataset info..."
python3 << 'EOF'
from datasets import load_dataset

# Load just the metadata to verify access
try:
    ds = load_dataset('SWE-bench/SWE-bench_Multilingual', split='test')
    
    # Filter for Rust tasks
    rust_repos = ['tokio-rs/tokio', 'tokio-rs/axum', 'astral-sh/ruff', 
                  'sharkdp/bat', 'nushell/nushell', 'uutils/coreutils', 
                  'burntsushi/ripgrep']
    
    rust_tasks = [t for t in ds if t['repo'].replace('__', '/') in rust_repos 
                  or any(r.replace('/', '__') in t['repo'] for r in rust_repos)]
    
    print(f"✓ Found {len(rust_tasks)} Rust tasks in SWE-bench Multilingual")
    
    # Show breakdown
    from collections import Counter
    repo_counts = Counter(t['repo'] for t in rust_tasks)
    for repo, count in sorted(repo_counts.items()):
        print(f"  - {repo}: {count} tasks")
        
except Exception as e:
    print(f"Note: Could not load dataset yet: {e}")
    print("You may need to authenticate with HuggingFace: huggingface-cli login")
EOF

echo ""
echo "=== Setup Complete ==="
echo ""
echo "To activate the environment:"
echo "  source $SCRIPT_DIR/.venv/bin/activate"
echo ""
echo "To run experiments:"
echo "  python src/run_experiment.py --help"
