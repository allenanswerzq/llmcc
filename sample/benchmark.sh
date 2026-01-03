#!/bin/bash
# Benchmark llmcc performance on sample projects
# This script runs llmcc with timing enabled and generates benchmark results
# For graph generation, use generate_all.sh instead

set -e

# Get absolute path to script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Try default release path, fall back to x86_64 target path
if [ -z "$LLMCC" ]; then
    if [ -x "$PROJECT_ROOT/target/release/llmcc" ]; then
        LLMCC="$PROJECT_ROOT/target/release/llmcc"
    elif [ -x "$PROJECT_ROOT/target/x86_64-unknown-linux-gnu/release/llmcc" ]; then
        LLMCC="$PROJECT_ROOT/target/x86_64-unknown-linux-gnu/release/llmcc"
    else
        echo "Error: llmcc not found"
        echo "Tried: $PROJECT_ROOT/target/release/llmcc"
        echo "Tried: $PROJECT_ROOT/target/x86_64-unknown-linux-gnu/release/llmcc"
        echo "Build with: cargo build --release"
        exit 1
    fi
fi

# Get CPU core count
get_cpu_cores() {
    if command -v nproc &> /dev/null; then
        nproc
    elif command -v lscpu &> /dev/null; then
        lscpu | grep "^CPU(s):" | awk '{print $2}'
    else
        echo "unknown"
    fi
}

CPU_CORES=$(get_cpu_cores)

TOP_K=200
BENCHMARK_FILE="$SCRIPT_DIR/benchmark_results_${CPU_CORES}.md"
BENCHMARK_DIR="$SCRIPT_DIR/benchmark_logs"
rm -rf "$BENCHMARK_FILE"
rm -rf "$BENCHMARK_DIR"

# Check llmcc exists
if [ ! -x "$LLMCC" ]; then
    echo "Error: llmcc not found at $LLMCC"
    echo "Build with: cargo build --release"
    exit 1
fi

# Fetch all sample repos if not already present
echo "Checking sample repositories..."
"$SCRIPT_DIR/fetch.sh"
echo ""

# Load shared project definitions
source "$SCRIPT_DIR/projects.sh"

# Create benchmark logs directory
mkdir -p "$BENCHMARK_DIR"

# Collect machine info
get_machine_info() {
    echo "## Machine Info"
    echo ""

    # CPU info
    if command -v lscpu &> /dev/null; then
        local cpu_model=$(lscpu | grep "Model name" | sed 's/Model name:\s*//')
        local cpu_cores=$(lscpu | grep "^CPU(s):" | awk '{print $2}')
        local cpu_threads=$(lscpu | grep "Thread(s) per core" | awk '{print $4}')
        local cpu_cores_physical=$(lscpu | grep "Core(s) per socket" | awk '{print $4}')
        echo "### CPU"
        echo "- **Model:** $cpu_model"
        echo "- **Cores:** $cpu_cores_physical physical, $cpu_cores logical (threads)"
    else
        echo "### CPU"
        echo "- $(uname -p)"
    fi
    echo ""

    # Memory info
    if command -v free &> /dev/null; then
        local mem_total=$(free -h | awk '/^Mem:/ {print $2}')
        local mem_available=$(free -h | awk '/^Mem:/ {print $7}')
        echo "### Memory"
        echo "- **Total:** $mem_total"
        echo "- **Available:** $mem_available"
    fi
    echo ""

    # OS info
    echo "### OS"
    echo "- **Kernel:** $(uname -sr)"
    if [ -f /etc/os-release ]; then
        local os_name=$(grep "^PRETTY_NAME" /etc/os-release | cut -d'"' -f2)
        echo "- **Distribution:** $os_name"
    fi
    echo ""
}

# Initialize benchmark file
cat > "$BENCHMARK_FILE" << 'EOF'
# LLMCC Benchmark Results

Generated on: DATE_PLACEHOLDER

EOF
# Replace placeholder with actual date
sed -i "s/DATE_PLACEHOLDER/$(date '+%Y-%m-%d %H:%M:%S')/" "$BENCHMARK_FILE"

# Add machine info to benchmark file
get_machine_info >> "$BENCHMARK_FILE"

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

    # Run with timing enabled (suppress warnings to reduce noise)
    RUST_LOG=info,llmcc_resolver=error "$LLMCC" -d "$src_dir" --graph --depth 3 -o "$dot_file" > "$log_file" 2>&1

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
    RUST_LOG=info,llmcc_resolver=error "$LLMCC" -d "$src_dir" --graph --depth 3 --pagerank-top-k $TOP_K -o "$dot_file" > "$log_file" 2>&1

    # Show summary on console
    grep -E "(Parsing total|Total time)" "$log_file" | tail -2 || true
}

# Count lines of code in a directory (Rust files only, excluding comments)
count_loc() {
    local src_dir=$1

    if [ ! -d "$src_dir" ]; then
        echo "0"
        return
    fi

    # Use tokei for accurate LOC counting (excludes comments and blanks)
    if command -v tokei &> /dev/null; then
        tokei "$src_dir" -t Rust -o json 2>/dev/null | \
            grep -oP '"code"\s*:\s*\K[0-9]+' | head -1 || echo "0"
    else
        # Fallback: count non-empty, non-comment lines in .rs files
        find "$src_dir" -name '*.rs' -type f -print0 2>/dev/null | \
            xargs -0 cat 2>/dev/null | \
            grep -v '^\s*$' | \
            grep -v '^\s*//' | \
            wc -l | \
            tr -d ' '
    fi
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

# Parse timing values from a log file, outputs space-separated values:
# files parse ir_symbols binding graph link total total_num
parse_timing_from_log() {
    local log_file=$1

    if [ ! -f "$log_file" ]; then
        echo "- - - - - - - 0"
        return
    fi

    # Extract timing values using grep directly on file (more efficient than reading entire file)
    local parse=$(grep -oP 'Parsing & tree-sitter: \K[0-9.]+s' "$log_file" 2>/dev/null | head -1)
    # Try fused IR+symbols first, then fall back to separate timings and sum them
    local ir_symbols=$(grep -oP 'IR build \+ Symbol collection: \K[0-9.]+s' "$log_file" 2>/dev/null | head -1)
    if [ -z "$ir_symbols" ]; then
        # Fall back to separate timings (legacy format)
        local ir=$(grep -oP 'IR building: \K[0-9.]+' "$log_file" 2>/dev/null | head -1)
        local symbols=$(grep -oP 'Symbol collection: \K[0-9.]+' "$log_file" 2>/dev/null | head -1)
        if [ -n "$ir" ] && [ -n "$symbols" ]; then
            local sum=$(echo "$ir + $symbols" | bc)
            ir_symbols="${sum}s"
        fi
    fi
    local binding=$(grep -oP 'Symbol binding: \K[0-9.]+s' "$log_file" 2>/dev/null | head -1)
    local graph=$(grep -oP 'Graph building: \K[0-9.]+s' "$log_file" 2>/dev/null | head -1)
    local link=$(grep -oP 'Linking units: \K[0-9.]+s' "$log_file" 2>/dev/null | head -1)
    local total=$(grep -oP 'Total time: \K[0-9.]+s' "$log_file" 2>/dev/null | head -1)
    local total_num=$(grep -oP 'Total time: \K[0-9.]+' "$log_file" 2>/dev/null | head -1)
    local files=$(grep -oP 'Parsing total \K[0-9]+' "$log_file" 2>/dev/null | head -1)

    # Ensure valid defaults
    [ -z "$parse" ] && parse="-"
    [ -z "$ir_symbols" ] && ir_symbols="-"
    [ -z "$binding" ] && binding="-"
    [ -z "$graph" ] && graph="-"
    [ -z "$link" ] && link="-"
    [ -z "$total" ] && total="-"
    [ -z "$total_num" ] && total_num="0"
    [ -z "$files" ] && files="-"

    echo "$files $parse $ir_symbols $binding $graph $link $total $total_num"
}

# Extract timing from log file and append to benchmark
extract_timing() {
    local name=$1
    local log_file=$2
    local src_dir=$3

    if [ ! -f "$log_file" ]; then
        echo "| $name | - | - | - | - | - | - | - | - |" >> "$BENCHMARK_FILE"
        return
    fi

    # Count lines of code and format as K (e.g., 92K)
    local loc_raw=$(count_loc "$src_dir")
    local loc
    if [ "$loc_raw" -ge 1000 ]; then
        loc=$(echo "scale=0; ($loc_raw + 500) / 1000" | bc)K
    else
        loc="$loc_raw"
    fi
    [ -z "$loc" ] && loc="-"

    read files parse ir_symbols binding graph link total total_num <<< $(parse_timing_from_log "$log_file")

    echo "| $name | $files | $loc | $parse | $ir_symbols | $binding | $graph | $link | $total |" >> "$BENCHMARK_FILE"
}

# PageRank benchmark section
echo "" >> "$BENCHMARK_FILE"
echo "## PageRank Timing (depth=3, top-$TOP_K)" >> "$BENCHMARK_FILE"
echo "" >> "$BENCHMARK_FILE"
echo "| Project | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |" >> "$BENCHMARK_FILE"
echo "|---------|-------|-----|-------|------------|---------|-------|------|-------|" >> "$BENCHMARK_FILE"

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

echo "" >> "$BENCHMARK_FILE"
echo "## Summary" >> "$BENCHMARK_FILE"
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

# Thread scaling benchmark on largest project (databend)
run_scaling_benchmark() {
    local name=$1
    local src_dir=$2
    local num_threads=$3
    local log_file="$BENCHMARK_DIR/${name}_scaling_${num_threads}t.log"
    local dot_file="$BENCHMARK_DIR/${name}_scaling_${num_threads}t.dot"

    RAYON_NUM_THREADS=$num_threads RUST_LOG=info,llmcc_resolver=error "$LLMCC" -d "$src_dir" \
        --graph --depth 3 --pagerank-top-k $TOP_K -o "$dot_file" > "$log_file" 2>&1

    echo "$log_file"
}

extract_scaling_timing() {
    local num_threads=$1
    local log_file=$2
    local baseline_time=$3

    if [ ! -f "$log_file" ]; then
        echo "| $num_threads | - | - | - | - | - | - | - |" >> "$BENCHMARK_FILE"
        echo "0"
        return
    fi

    read files parse ir_symbols binding graph link total total_num <<< $(parse_timing_from_log "$log_file")

    # Ensure total_num is a valid number for bc
    if [ -z "$total_num" ] || ! [[ "$total_num" =~ ^[0-9.]+$ ]]; then
        total_num="0"
    fi

    # Calculate speedup vs baseline (1 thread)
    local speedup="-"
    if [ -n "$baseline_time" ] && [[ "$baseline_time" =~ ^[0-9.]+$ ]] && [ "$total_num" != "0" ] && [ "$total_num" != "0.00" ]; then
        speedup=$(echo "scale=2; $baseline_time / $total_num" | bc 2>/dev/null || echo "-")
        if [ -n "$speedup" ] && [ "$speedup" != "-" ]; then
            speedup="${speedup}x"
        else
            speedup="-"
        fi
    fi

    # Print progress to stderr (visible in terminal)
    echo "$total ($speedup)" >&2

    # Append to benchmark file
    echo "| $num_threads | $parse | $ir_symbols | $binding | $graph | $link | $total | $speedup |" >> "$BENCHMARK_FILE"

    # Return only total_num via stdout
    echo "$total_num"
}

echo ""
echo "=== Thread Scaling Benchmark (databend) ==="

echo "" >> "$BENCHMARK_FILE"
echo "## Thread Scaling (databend, depth=3, top-$TOP_K, $CPU_CORES cores)" >> "$BENCHMARK_FILE"
echo "" >> "$BENCHMARK_FILE"
echo "| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |" >> "$BENCHMARK_FILE"
echo "|---------|-------|------------|---------|-------|------|-------|---------|" >> "$BENCHMARK_FILE"

SCALING_PROJECT="databend"
SCALING_SRC_DIR="${PROJECTS[$SCALING_PROJECT]}"

# Build thread counts dynamically based on available CPU cores
THREAD_COUNTS=(1)
for t in 2 4 8 16 24 32 48 64; do
    if [ "$t" -le "$CPU_CORES" ]; then
        THREAD_COUNTS+=("$t")
    fi
done

baseline_time=""

for threads in "${THREAD_COUNTS[@]}"; do
    echo -n "  Running with $threads thread(s)... "

    log_file=$(run_scaling_benchmark "$SCALING_PROJECT" "$SCALING_SRC_DIR" "$threads")
    total_num=$(extract_scaling_timing "$threads" "$log_file" "$baseline_time")

    # Capture baseline time from first run
    if [ "$threads" -eq 1 ]; then
        baseline_time="$total_num"
    fi
done

echo ""
echo "=== Benchmark Complete ==="
echo ""
cat "$BENCHMARK_FILE"
