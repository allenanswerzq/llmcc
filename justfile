# Default shell for all recipes
set shell := ["/bin/bash", "-c"]

root := justfile_directory()
venv := root + "/venv"

ensure-venv:
    if [ ! -d "{{venv}}" ]; then \
        python3 -m venv "{{venv}}"; \
        . "{{venv}}/bin/activate" && pip install -r "{{root}}/requirements.txt"; \
    fi

build-bindings: ensure-venv
    . "{{venv}}/bin/activate" && maturin develop --manifest-path "{{root}}/crates/llmcc-bindings/Cargo.toml"

run-example: build-bindings
    . "{{venv}}/bin/activate" && python "{{root}}/examples/basic.py"

# Release the entire project: just release 0.2.0
release version:
    #!/bin/bash
    set -e

    VERSION="{{version}}"
    TAG="v${VERSION}"

    # Verify version format (e.g., 0.2.0)
    if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "❌ Invalid version format: $VERSION (expected e.g., 0.2.0)"
        exit 1
    fi

    echo "� Releasing llmcc v$VERSION"
    echo ""

    # List of crates to update
    CRATES=(
        "crates/llmcc-core/Cargo.toml"
        "crates/llmcc-rust/Cargo.toml"
        "crates/llmcc-python/Cargo.toml"
        "crates/llmcc-bindings/Cargo.toml"
        "crates/llmcc/Cargo.toml"
    )

    # Update all Rust Cargo.toml files
    echo "📝 Updating Rust crate versions..."
    for manifest in "${CRATES[@]}"; do
        if [ -f "{{root}}/$manifest" ]; then
            sed -i.bak 's/^version = .*/version = "'$VERSION'"/' "{{root}}/$manifest"
            rm -f "{{root}}/${manifest}.bak"
            echo "  ✏️  $manifest"
        else
            echo "  ⚠️  Not found: $manifest"
        fi
    done

    # Update Python package versions
    echo "� Updating Python package versions..."
    if [ -f "{{root}}/pyproject.toml" ]; then
        sed -i.bak 's/^version = .*/version = "'$VERSION'"/' "{{root}}/pyproject.toml"
        rm -f "{{root}}/pyproject.toml.bak"
        echo "  ✏️  pyproject.toml"
    fi

    if [ -f "{{root}}/setup.py" ]; then
        sed -i.bak 's/version=.*/version="'$VERSION'",/' "{{root}}/setup.py"
        rm -f "{{root}}/setup.py.bak"
        echo "  ✏️  setup.py"
    fi

    echo ""
    echo "🔨 Building all crates..."
    cargo build --release 2>&1 | grep -E "^(Compiling|Finished|error)" || true

    echo ""
    echo "🧪 Testing all crates..."
    cargo test --release 2>&1 | grep -E "^(running|test result)" || true

    echo ""
    echo "📦 Committing version bump..."
    git add {{root}}/crates/*/Cargo.toml {{root}}/pyproject.toml {{root}}/setup.py
    git commit -m "chore: release v$VERSION" || echo "  ⚠️  Nothing to commit"

    echo "🚀 Pushing to main branch..."
    git push origin main || echo "  ⚠️  Failed to push (might already be up to date)"

    echo ""
    echo "🏷️  Creating tag: $TAG"
    git tag -a "$TAG" -m "Release llmcc v$VERSION"

    echo "� Pushing tag to GitHub..."
    git push origin "$TAG"

    echo ""
    echo "✨ Release $VERSION initiated!"
    echo ""
    echo "🔄 Workflows triggered:"
    echo "   1️⃣  Rust Release - builds and publishes all crates to crates.io"
    echo "   2️⃣  Python Release - builds wheels and publishes to PyPI"
    echo ""
    echo "📊 Monitor progress:"
    echo "   https://github.com/allenanswerzq/llmcc/actions"
    echo ""
    echo "⏱️  Estimated time:"
    echo "   - Rust: 5-10 minutes"
    echo "   - Python: 30-45 minutes (parallel builds)"
    echo ""
    echo "✅ Release complete when both workflows show success (green ✓)"

# Show release status
release-status:
    #!/bin/bash
    echo "📋 Recent releases:"
    git tag --list --sort=-version:refname | head -10
    echo ""
    echo "� View on GitHub:"
    echo "   https://github.com/allenanswerzq/llmcc/releases"
    echo ""
    echo "📦 PyPI: https://pypi.org/project/llmcc/"
    echo "📦 crates.io:"
    echo "   - https://crates.io/crates/llmcc"
    echo "   - https://crates.io/crates/llmcc-core"
    echo "   - https://crates.io/crates/llmcc-rust"
    echo "   - https://crates.io/crates/llmcc-python"
    echo "   - https://crates.io/crates/llmcc-bindings"