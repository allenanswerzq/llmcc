#!/bin/bash
# Benchmark llmcc performance on sample projects
# This script runs llmcc with timing enabled and generates benchmark results
# For graph generation, use generate_all.sh instead

set -e

# Get absolute path to script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

LLMCC="${LLMCC:-$PROJECT_ROOT/target/release/llmcc}"
TOP_K=200
BENCHMARK_FILE="$SCRIPT_DIR/benchmark_results.md"
BENCHMARK_DIR="$SCRIPT_DIR/benchmark_logs"

# Check llmcc exists
if [ ! -x "$LLMCC" ]; then
    echo "Error: llmcc not found at $LLMCC"
    echo "Build with: cargo build --release"
    exit 1
fi

# Projects to benchmark: name -> source directory
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
    # Database & data infrastructure
    ["lancedb"]="$SCRIPT_DIR/repos/lancedb"
    ["lance"]="$SCRIPT_DIR/repos/lance"
    ["opendal"]="$SCRIPT_DIR/repos/opendal"
    ["risingwave"]="$SCRIPT_DIR/repos/risingwave"
    ["databend"]="$SCRIPT_DIR/repos/databend"
    ["datafusion"]="$SCRIPT_DIR/repos/datafusion"
    ["qdrant"]="$SCRIPT_DIR/repos/qdrant"
)

# Create benchmark logs directory
mkdir -p "$BENCHMARK_DIR"

# Initialize benchmark file
cat > "$BENCHMARK_FILE" << 'EOF'
# LLMCC Benchmark Results

Generated on: DATE_PLACEHOLDER
EOF
# Replace placeholder with actual date
sed -i "s/DATE_PLACEHOLDER/$(date '+%Y-%m-%d %H:%M:%S')/" "$BENCHMARK_FILE"

echo "=== LLMCC Benchmark ==="
echo "Binary: $LLMCC"
echo "Results: $BENCHMARK_FILE"
echo ""

# Run benchmark on a project
run_benchmark() {
    local name=$1
    local src_dir=$2
    local log_file="$BENCHMARK_DIR/${name}_depth3.log"
    local dot_file="$BENCHMARK_DIR/${name}_depth3.dot"

    echo "  Running depth=3 benchmark..."

    # Run with timing enabled (RUST_LOG=info captures timing info)
    RUST_LOG=info "$LLMCC" -d "$src_dir" --graph --depth 3 -o "$dot_file" > "$log_file" 2>&1

    # Show summary on console
    grep -E "(Parsing total|Total time)" "$log_file" | tail -2 || true
}

# Run benchmark with PageRank filtering
run_benchmark_pagerank() {
    local name=$1
    local src_dir=$2
    local log_file="$BENCHMARK_DIR/${name}_pagerank_depth3.log"
    local dot_file="$BENCHMARK_DIR/${name}_pagerank_depth3.dot"

    echo "  Running depth=3 benchmark with PageRank top-$TOP_K..."

    # Run with timing enabled and PageRank filtering
    RUST_LOG=info "$LLMCC" -d "$src_dir" --graph --depth 3 --pagerank-top-k $TOP_K -o "$dot_file" > "$log_file" 2>&1

    # Show summary on console
    grep -E "(Parsing total|Total time)" "$log_file" | tail -2 || true
}

# Count lines of code in a directory (Rust files only)
count_loc() {
    local src_dir=$1

    if [ ! -d "$src_dir" ]; then
        echo "0"
        return
    fi

    # Count non-empty, non-comment lines in .rs files
    find "$src_dir" -name '*.rs' -type f -print0 2>/dev/null | \
        xargs -0 cat 2>/dev/null | \
        grep -v '^\s*$' | \
        grep -v '^\s*//' | \
        wc -l | \
        tr -d ' '
}

# Count nodes and edges in a DOT file
count_graph_stats() {
    local dot_file=$1

    if [ ! -f "$dot_file" ]; then
        echo "0 0"
        return
    fi

    # Count nodes: lines matching 'n123[label='
    local nodes=$(grep -cE '^\s+n[0-9]+\[label=' "$dot_file" 2>/dev/null || echo "0")

    # Count edges: lines matching '->'
    local edges=$(grep -c '\->' "$dot_file" 2>/dev/null || echo "0")

    echo "$nodes $edges"
}

# Extract timing from log file and append to benchmark
extract_timing() {
    local name=$1
    local log_file=$2

    local src_dir=$3

    if [ ! -f "$log_file" ]; then
        echo "| $name | - | - | - | - | - | - | - | - | - |" >> "$BENCHMARK_FILE"
        return
    fi

    # Count lines of code and format as K (e.g., 92K)
    local loc_raw=$(count_loc "$src_dir")
    if [ "$loc_raw" -ge 1000 ]; then
        loc=$(echo "scale=0; ($loc_raw + 500) / 1000" | bc)K
    else
        loc="$loc_raw"
    fi

    # Join lines and extract timing (log output may have embedded newlines)
    local log_content=$(tr -d '\n' < "$log_file")

    local files=$(echo "$log_content" | grep -oP 'Parsing total \K[0-9]+' 2>/dev/null | head -1 || echo "")
    local parse=$(echo "$log_content" | grep -oP 'Parsing & tree-sitter: \K[0-9.]+s' 2>/dev/null | head -1 || echo "")
    local ir=$(echo "$log_content" | grep -oP 'IR building: \K[0-9.]+s' 2>/dev/null | head -1 || echo "")
    local symbols=$(echo "$log_content" | grep -oP 'Symbol collection: \K[0-9.]+s' 2>/dev/null | head -1 || echo "")
    local binding=$(echo "$log_content" | grep -oP 'Symbol binding: \K[0-9.]+s' 2>/dev/null | head -1 || echo "")
    local graph=$(echo "$log_content" | grep -oP 'Graph building: \K[0-9.]+s' 2>/dev/null | head -1 || echo "")
    local link=$(echo "$log_content" | grep -oP 'Linking units: \K[0-9.]+s' 2>/dev/null | head -1 || echo "")
    local total=$(echo "$log_content" | grep -oP 'Total time: \K[0-9.]+s' 2>/dev/null | head -1 || echo "")

    # Handle empty values
    [ -z "$files" ] && files="-"
    [ -z "$loc" ] && loc="-"
    [ -z "$parse" ] && parse="-"
    [ -z "$ir" ] && ir="-"
    [ -z "$symbols" ] && symbols="-"
    [ -z "$binding" ] && binding="-"
    [ -z "$graph" ] && graph="-"
    [ -z "$link" ] && link="-"
    [ -z "$total" ] && total="-"

    echo "| $name | $files | $loc | $parse | $ir | $symbols | $binding | $graph | $link | $total |" >> "$BENCHMARK_FILE"
}

# PageRank benchmark section
echo "" >> "$BENCHMARK_FILE"
echo "## PageRank Timing (depth=3, top-$TOP_K)" >> "$BENCHMARK_FILE"
echo "" >> "$BENCHMARK_FILE"
echo "| Project | Files | LoC | Parse | IR Build | Symbols | Binding | Graph | Link | Total |" >> "$BENCHMARK_FILE"
echo "|---------|-------|-----|-------|----------|---------|---------|-------|------|-------|" >> "$BENCHMARK_FILE"

# Build array of (loc, name) pairs and sort by LOC descending
declare -a PROJECT_LOC_PAIRS=()
for name in "${!PROJECTS[@]}"; do
    src_dir="${PROJECTS[$name]}"
    if [ -d "$src_dir" ]; then
        loc_raw=$(count_loc "$src_dir")
    else
        loc_raw=0
    fi
    PROJECT_LOC_PAIRS+=("$loc_raw:$name")
done

# Sort by LOC descending (numeric sort on first field)
IFS=$'\n' SORTED_PROJECTS=($(printf '%s\n' "${PROJECT_LOC_PAIRS[@]}" | sort -t: -k1 -nr))
unset IFS

for entry in "${SORTED_PROJECTS[@]}"; do
    name="${entry#*:}"  # Extract name after ':'
    src_dir="${PROJECTS[$name]}"

    if [ ! -d "$src_dir" ]; then
        echo "| $name | (not found) | - | - | - | - | - | - | - |" >> "$BENCHMARK_FILE"
        continue
    fi

    echo ""
    echo "=== Benchmarking $name ==="

    # Run full graph benchmark (for reduction stats)
    run_benchmark "$name" "$src_dir"

    # Run PageRank filtered benchmark
    run_benchmark_pagerank "$name" "$src_dir"
    extract_timing "$name" "$BENCHMARK_DIR/${name}_pagerank_depth3.log" "$src_dir"
done

# Calculate summary statistics
echo "" >> "$BENCHMARK_FILE"
echo "## Summary" >> "$BENCHMARK_FILE"
echo "" >> "$BENCHMARK_FILE"
echo "Benchmarked on: $(uname -a)" >> "$BENCHMARK_FILE"
echo "" >> "$BENCHMARK_FILE"
echo "Binary: $LLMCC" >> "$BENCHMARK_FILE"
echo "" >> "$BENCHMARK_FILE"

# Count projects by size
small=0
medium=0
large=0

for name in "${!PROJECTS[@]}"; do
    log_file="$BENCHMARK_DIR/${name}_depth3.log"
    if [ -f "$log_file" ]; then
        files=$(tr -d '\n' < "$log_file" | grep -oP 'Parsing total \K[0-9]+' 2>/dev/null | head -1 || echo "0")
        if [ -n "$files" ] && [ "$files" -gt 0 ]; then
            if [ "$files" -lt 50 ]; then
                ((small++)) || true
            elif [ "$files" -lt 500 ]; then
                ((medium++)) || true
            else
                ((large++)) || true
            fi
        fi
    fi
done

echo "### Project Sizes" >> "$BENCHMARK_FILE"
echo "- Small (<50 files): $small projects" >> "$BENCHMARK_FILE"
echo "- Medium (50-500 files): $medium projects" >> "$BENCHMARK_FILE"
echo "- Large (>500 files): $large projects" >> "$BENCHMARK_FILE"

# PageRank graph reduction stats
echo "" >> "$BENCHMARK_FILE"
echo "## PageRank Graph Reduction (depth=3)" >> "$BENCHMARK_FILE"
echo "" >> "$BENCHMARK_FILE"
echo "| Project | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |" >> "$BENCHMARK_FILE"
echo "|---------|------------|------------|----------|----------|----------------|----------------|" >> "$BENCHMARK_FILE"

# Use same sorted order as timing table
for entry in "${SORTED_PROJECTS[@]}"; do
    name="${entry#*:}"  # Extract name after ':'
    full_dot="$BENCHMARK_DIR/${name}_depth3.dot"
    pr_dot="$BENCHMARK_DIR/${name}_pagerank_depth3.dot"

    if [ -f "$full_dot" ] && [ -f "$pr_dot" ]; then
        read full_nodes full_edges <<< $(count_graph_stats "$full_dot")
        read pr_nodes pr_edges <<< $(count_graph_stats "$pr_dot")

        # Calculate reduction percentages
        if [ "$full_nodes" -gt 0 ]; then
            node_reduction=$(echo "scale=1; (1 - $pr_nodes / $full_nodes) * 100" | bc)%
        else
            node_reduction="-"
        fi

        if [ "$full_edges" -gt 0 ]; then
            edge_reduction=$(echo "scale=1; (1 - $pr_edges / $full_edges) * 100" | bc)%
        else
            edge_reduction="-"
        fi

        echo "| $name | $full_nodes | $full_edges | $pr_nodes | $pr_edges | $node_reduction | $edge_reduction |" >> "$BENCHMARK_FILE"
    else
        echo "| $name | - | - | - | - | - | - |" >> "$BENCHMARK_FILE"
    fi
done

echo ""
echo "=== Benchmark Complete ==="
echo ""
cat "$BENCHMARK_FILE"
