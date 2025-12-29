#!/bin/bash
# Generate architecture graphs for all sample projects
# For performance benchmarks, use benchmark.sh instead

set -e

# Get absolute path to script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

LLMCC="${LLMCC:-$PROJECT_ROOT/target/release/llmcc}"

# Depth-specific PageRank top-K values (nodes)
# Smaller K for aggregated levels to keep graphs readable
TOP_K_CRATE="${TOP_K_CRATE:-15}"      # depth 1: top 15 crates
TOP_K_MODULE="${TOP_K_MODULE:-30}"    # depth 2: top 30 modules
TOP_K_FILE="${TOP_K_FILE:-200}"       # depth 3: top 200 nodes

# SVG generation settings
SKIP_SVG="${SKIP_SVG:-false}"
SVG_SIZE_THRESHOLD="${SVG_SIZE_THRESHOLD:-500000}"  # 500KB
SVG_TIMEOUT="${SVG_TIMEOUT:-20}"

# Check llmcc exists
if [ ! -x "$LLMCC" ]; then
    echo "Error: llmcc not found at $LLMCC"
    echo "Build with: cargo build --release"
    exit 1
fi

echo "=== LLMCC Graph Generation ==="
echo "Binary: $LLMCC"
echo "PageRank top-k: crate=$TOP_K_CRATE, module=$TOP_K_MODULE, file=$TOP_K_FILE"
echo "Skip SVG: $SKIP_SVG"
echo "SVG size threshold: ${SVG_SIZE_THRESHOLD} bytes"
echo "SVG timeout: ${SVG_TIMEOUT}s"
echo ""

# Ensure repos are fetched
echo "=== Fetching repositories ==="
"$SCRIPT_DIR/fetch.sh"

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
    # Database & data infrastructure
    ["lancedb"]="$SCRIPT_DIR/repos/lancedb"
    ["lance"]="$SCRIPT_DIR/repos/lance"
    ["opendal"]="$SCRIPT_DIR/repos/opendal"
    ["risingwave"]="$SCRIPT_DIR/repos/risingwave"
    ["databend"]="$SCRIPT_DIR/repos/databend"
    ["datafusion"]="$SCRIPT_DIR/repos/datafusion"
    ["qdrant"]="$SCRIPT_DIR/repos/qdrant"
)

# Depth level names
declare -A DEPTH_NAMES=(
    [0]="depth_0_project"
    [1]="depth_1_crate"
    [2]="depth_2_module"
    [3]="depth_3_file"
)

generate_graphs() {
    local name=$1
    local src_dir=$2
    local output_dir=$3
    local use_pagerank=$4  # "true" or ""

    mkdir -p "$output_dir"

    for depth in 0 1 2 3; do
        local depth_name="${DEPTH_NAMES[$depth]}"
        local dot_file="$output_dir/${depth_name}.dot"
        local pagerank_flag=""

        # Use depth-specific top-K for PageRank filtered graphs
        if [ "$use_pagerank" = "true" ]; then
            case $depth in
                1) pagerank_flag="--pagerank-top-k $TOP_K_CRATE" ;;
                2) pagerank_flag="--pagerank-top-k $TOP_K_MODULE" ;;
                3) pagerank_flag="--pagerank-top-k $TOP_K_FILE" ;;
                *) pagerank_flag="" ;;  # depth 0 (project) - no filtering
            esac
        fi

        echo "  Generating $depth_name... $pagerank_flag"
        $LLMCC -d "$src_dir" --graph --depth $depth $pagerank_flag -o "$dot_file" 2>&1
    done

    # Skip SVG generation entirely if SKIP_SVG=true
    if [ "$SKIP_SVG" = "true" ]; then
        echo "  ⏭️  SVG generation skipped (SKIP_SVG=true)"
        return
    fi

    # Generate SVGs
    if command -v dot &> /dev/null; then
        echo "  Generating SVG files..."
        for dotfile in "$output_dir"/*.dot; do
            local svgfile="${dotfile%.dot}.svg"
            local dotname=$(basename "$dotfile")
            local filesize=$(stat -c%s "$dotfile" 2>/dev/null || echo 0)

            # Skip if file exceeds size threshold
            if [ "$filesize" -gt "$SVG_SIZE_THRESHOLD" ]; then
                echo "    ⏭️  Skipping $dotname (${filesize} bytes > ${SVG_SIZE_THRESHOLD} threshold)"
                echo "<!-- SVG skipped: ${filesize} bytes -->" > "$svgfile"
            else
                if timeout "$SVG_TIMEOUT" dot -Tsvg "$dotfile" -o "$svgfile" 2>/dev/null; then
                    echo "    ✅ $dotname (${filesize} bytes)"
                else
                    echo "    ⚠️  Timeout: $dotname"
                    echo "<!-- SVG timeout: ${SVG_TIMEOUT}s -->" > "$svgfile"
                fi
            fi
        done
    else
        echo "  Warning: 'dot' not found. Skipping SVG generation."
    fi
}

# Process each project
for name in "${!PROJECTS[@]}"; do
    src_dir="${PROJECTS[$name]}"

    # Skip if source doesn't exist
    if [ ! -d "$src_dir" ]; then
        echo "=== Skipping $name (not found: $src_dir) ==="
        continue
    fi

    echo ""
    echo "=== Processing $name ==="

    # Full graphs (no PageRank filtering)
    echo "  [Full graphs]"
    generate_graphs "$name" "$src_dir" "$SCRIPT_DIR/$name" ""

    # PageRank filtered (depth-specific top-K)
    echo "  [PageRank filtered]"
    generate_graphs "$name" "$src_dir" "$SCRIPT_DIR/${name}-pagerank" "true"
done

echo ""
echo "=== Summary ==="
for name in "${!PROJECTS[@]}"; do
    if [ -d "$SCRIPT_DIR/$name" ]; then
        full_nodes=$(grep -cE '^\s+n[0-9]+\[label=' "$SCRIPT_DIR/$name/depth_3_file.dot" 2>/dev/null || echo "N/A")
        pr_nodes=$(grep -cE '^\s+n[0-9]+\[label=' "$SCRIPT_DIR/${name}-pagerank/depth_3_file.dot" 2>/dev/null || echo "N/A")
        echo "$name: $full_nodes nodes (full) -> $pr_nodes nodes (pagerank)"
    fi
done

echo ""
echo "Done! Run $SCRIPT_DIR/benchmark.sh for performance benchmarks."
