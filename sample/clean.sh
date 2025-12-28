#!/bin/bash
# Clean up generated files in sample folder
# Keeps: repos/, scripts (*.sh), and benchmark_results.md

set -e
cd "$(dirname "$0")"

echo "=== Cleaning sample directory ==="

# Remove project output directories (full and pagerank)
for dir in */; do
    case "$dir" in
        repos/|benchmark_logs/)
            # Keep repos, optionally keep benchmark_logs
            ;;
        *)
            if [ -d "$dir" ]; then
                echo "Removing: $dir"
                rm -rf "$dir"
            fi
            ;;
    esac
done

# Remove benchmark logs if requested
if [ "$1" = "--all" ]; then
    echo "Removing: benchmark_logs/"
    rm -rf benchmark_logs/
    echo "Removing: benchmark_results.md"
    rm -f benchmark_results.md
fi

echo ""
echo "Done! Kept: repos/, *.sh scripts"
echo "Use './clean.sh --all' to also remove benchmark_logs/ and benchmark_results.md"
