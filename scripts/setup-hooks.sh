#!/bin/bash
# Git hooks setup script for the project
# Run this script to install git hooks for development

set -e

HOOKS_DIR=".git/hooks"
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

echo "Setting up git hooks..."

# Check if .git directory exists
if [ ! -d ".git" ]; then
    echo "Error: Not in a git repository root directory"
    exit 1
fi

# List of hooks to set up
HOOKS=("pre-commit" "pre-push")

for hook in "${HOOKS[@]}"; do
    HOOK_SOURCE="$SCRIPT_DIR/scripts/git-hooks/$hook"
    HOOK_DEST="$HOOKS_DIR/$hook"

    if [ -f "$HOOK_SOURCE" ]; then
        cp "$HOOK_SOURCE" "$HOOK_DEST"
        chmod +x "$HOOK_DEST"
        echo "✓ Installed $hook hook"
    else
        echo "⚠ Warning: $hook script not found at $HOOK_SOURCE"
    fi
done

echo ""
echo "Git hooks installed successfully!"
echo "The following hooks are now active:"
echo "  - pre-commit: Runs formatting and linting checks"
echo "  - pre-push:   Runs full test suite before pushing"
echo ""
echo "To bypass hooks when needed, use: git commit --no-verify or git push --no-verify"
