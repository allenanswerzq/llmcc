#!/bin/bash
# Generate architecture graphs for all sample projects
# For performance benchmarks, use benchmark.sh instead

set -e

# Get absolute path to script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

LLMCC="${LLMCC:-$PROJECT_ROOT/target/release/llmcc}"

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
echo "Skip SVG: $SKIP_SVG"
echo "SVG size threshold: ${SVG_SIZE_THRESHOLD} bytes"
echo "SVG timeout: ${SVG_TIMEOUT}s"
echo ""

# Ensure repos are fetched
echo "=== Fetching repositories ==="
"$SCRIPT_DIR/fetch.sh"

# Load shared project definitions
source "$SCRIPT_DIR/projects.sh"

# Depth level names
declare -A DEPTH_NAMES=(
    [0]="depth_0_project"
    [1]="depth_1_crate"
    [2]="depth_2_module"
    [3]="depth_3_file"
)

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

# Compute top-K values based on LoC
# Larger codebases get larger top-K to keep graphs readable
compute_top_k() {
    local loc=$1
    local depth=$2

    # Base multipliers for each depth level
    # depth 1 (crate): smallest, depth 3 (file): largest
    case $depth in
        1)  # Crate level
            if [ "$loc" -gt 400000 ]; then
                echo 25
            elif [ "$loc" -gt 200000 ]; then
                echo 20
            elif [ "$loc" -gt 50000 ]; then
                echo 15
            else
                echo 10
            fi
            ;;
        2)  # Module level
            if [ "$loc" -gt 400000 ]; then
                echo 50
            elif [ "$loc" -gt 200000 ]; then
                echo 40
            elif [ "$loc" -gt 50000 ]; then
                echo 30
            else
                echo 20
            fi
            ;;
        3)  # File level
            if [ "$loc" -gt 400000 ]; then
                echo 300
            elif [ "$loc" -gt 200000 ]; then
                echo 250
            elif [ "$loc" -gt 50000 ]; then
                echo 200
            else
                echo 150
            fi
            ;;
        *)
            echo 0
            ;;
    esac
}

generate_graphs() {
    local name=$1
    local src_dir=$2
    local output_dir=$3
    local use_pagerank=$4  # "true" or ""
    local loc=$5           # lines of code for top-k calculation

    mkdir -p "$output_dir"

    for depth in 0 1 2 3; do
        local depth_name="${DEPTH_NAMES[$depth]}"
        local dot_file="$output_dir/${depth_name}.dot"
        local pagerank_flag=""

        # Use LoC-based top-K for PageRank filtered graphs
        if [ "$use_pagerank" = "true" ]; then
            local top_k=$(compute_top_k "$loc" "$depth")
            if [ "$top_k" -gt 0 ]; then
                pagerank_flag="--pagerank-top-k $top_k"
            fi
        fi

        # For large projects at module level (depth 2), add clustering and short labels
        local layout_flags=""
        if [ "$depth" -eq 2 ] && [ "$loc" -gt 50000 ]; then
            layout_flags="--cluster-by-crate --short-labels"
        fi

        echo "  Generating $depth_name... $pagerank_flag $layout_flags"
        $LLMCC -d "$src_dir" --graph --depth $depth $pagerank_flag $layout_flags -o "$dot_file" 2>&1
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

# Calculate LoC for all projects and sort by size (largest first)
echo ""
echo "=== Calculating LoC for all projects ==="
declare -A PROJECT_LOC
for name in "${!PROJECTS[@]}"; do
    src_dir="${PROJECTS[$name]}"
    if [ -d "$src_dir" ]; then
        loc=$(count_loc "$src_dir")
        PROJECT_LOC[$name]=$loc
        echo "  $name: ${loc} lines"
    else
        PROJECT_LOC[$name]=0
    fi
done

# Sort projects by LoC (descending)
SORTED_PROJECTS=$(for name in "${!PROJECT_LOC[@]}"; do
    echo "${PROJECT_LOC[$name]} $name"
done | sort -rn | awk '{print $2}')

echo ""
echo "=== Processing order (by LoC, largest first) ==="
for name in $SORTED_PROJECTS; do
    loc=${PROJECT_LOC[$name]}
    if [ "$loc" -gt 0 ]; then
        loc_k=$(echo "scale=0; $loc / 1000" | bc)
        echo "  $name: ${loc_k}K LoC"
    fi
done

# Process each project in LoC order
for name in $SORTED_PROJECTS; do
    src_dir="${PROJECTS[$name]}"
    loc=${PROJECT_LOC[$name]}

    # Skip if source doesn't exist
    if [ ! -d "$src_dir" ]; then
        echo "=== Skipping $name (not found: $src_dir) ==="
        continue
    fi

    loc_k=$(echo "scale=0; $loc / 1000" | bc)
    echo ""
    echo "=== Processing $name (${loc_k}K LoC) ==="

    # Full graphs (no PageRank filtering)
    echo "  [Full graphs]"
    generate_graphs "$name" "$src_dir" "$SCRIPT_DIR/$name" "" "$loc"

    # PageRank filtered (LoC-based top-K)
    echo "  [PageRank filtered]"
    generate_graphs "$name" "$src_dir" "$SCRIPT_DIR/${name}-pagerank" "true" "$loc"
done

echo ""
echo "=== Summary ==="
for name in $SORTED_PROJECTS; do
    if [ -d "$SCRIPT_DIR/$name" ]; then
        loc=${PROJECT_LOC[$name]}
        loc_k=$(echo "scale=0; $loc / 1000" | bc)
        full_nodes=$(grep -cE '^\s+n[0-9]+\[label=' "$SCRIPT_DIR/$name/depth_3_file.dot" 2>/dev/null || echo "N/A")
        pr_nodes=$(grep -cE '^\s+n[0-9]+\[label=' "$SCRIPT_DIR/${name}-pagerank/depth_3_file.dot" 2>/dev/null || echo "N/A")
        echo "$name (${loc_k}K): $full_nodes nodes (full) -> $pr_nodes nodes (pagerank)"
    fi
done

echo ""
echo "Done! Run $SCRIPT_DIR/benchmark.sh for performance benchmarks."
