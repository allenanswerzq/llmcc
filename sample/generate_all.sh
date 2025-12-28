#!/bin/bash
# Generate architecture graphs for all sample projects
# For performance benchmarks, use benchmark.sh instead

set -e
cd "$(dirname "$0")"

LLMCC="${LLMCC:-../target/release/llmcc}"
TOP_K=200

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
echo "PageRank top-k: $TOP_K"
echo "Skip SVG: $SKIP_SVG"
echo "SVG size threshold: ${SVG_SIZE_THRESHOLD} bytes"
echo "SVG timeout: ${SVG_TIMEOUT}s"
echo ""

# Ensure repos are fetched
echo "=== Fetching repositories ==="
./repos/fetch.sh

# Projects to process: name -> source directory
declare -A PROJECTS=(
    # Core ecosystem
    ["ripgrep"]="./repos/ripgrep"
    ["tokio"]="./repos/tokio"
    ["serde"]="./repos/serde"
    ["clap"]="./repos/clap"
    ["axum"]="./repos/axum"
    # From GitHub
    ["codex"]="./repos/codex"
    ["llmcc"]="./repos/llmcc"
    # Database & data infrastructure
    ["lancedb"]="./repos/lancedb"
    ["lance"]="./repos/lance"
    ["opendal"]="./repos/opendal"
    ["risingwave"]="./repos/risingwave"
    ["databend"]="./repos/databend"
    ["datafusion"]="./repos/datafusion"
    ["qdrant"]="./repos/qdrant"
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
    local pagerank_flag=$4

    mkdir -p "$output_dir"

    for depth in 0 1 2 3; do
        local depth_name="${DEPTH_NAMES[$depth]}"
        local dot_file="$output_dir/${depth_name}.dot"

        echo "  Generating $depth_name..."
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

    # Full graphs
    echo "  [Full graphs]"
    generate_graphs "$name" "$src_dir" "./$name" ""

    # PageRank filtered
    echo "  [PageRank top-$TOP_K]"
    generate_graphs "$name" "$src_dir" "./${name}-pagerank" "--pagerank-top-k $TOP_K"
done

echo ""
echo "=== Summary ==="
for name in "${!PROJECTS[@]}"; do
    if [ -d "./$name" ]; then
        full_nodes=$(grep -cE '^\s+n[0-9]+\[label=' "./$name/depth_3_file.dot" 2>/dev/null || echo "N/A")
        pr_nodes=$(grep -cE '^\s+n[0-9]+\[label=' "./${name}-pagerank/depth_3_file.dot" 2>/dev/null || echo "N/A")
        echo "$name: $full_nodes nodes (full) -> $pr_nodes nodes (pagerank)"
    fi
done

echo ""
echo "Done! Run ./benchmark.sh for performance benchmarks."
