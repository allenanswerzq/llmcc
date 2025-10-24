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

# Release recipes
# To release Rust crates or Python package, follow the tag-based workflow

# Release a Rust crate: just release-rust llmcc-core 0.2.0
release-rust crate version:
    #!/bin/bash
    set -e
    
    CRATE="{{crate}}"
    VERSION="{{version}}"
    TAG="${CRATE}-v${VERSION}"
    
    # Validate crate name
    case "$CRATE" in
        llmcc-core|llmcc-rust|llmcc-python|llmcc-bindings|llmcc)
            ;;
        *)
            echo "Invalid crate: $CRATE"
            echo "Supported crates: llmcc-core, llmcc-rust, llmcc-python, llmcc-bindings, llmcc"
            exit 1
            ;;
    esac
    
    # Verify version format (e.g., 0.2.0)
    if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "Invalid version format: $VERSION (expected e.g., 0.2.0)"
        exit 1
    fi
    
    echo "📦 Preparing to release $CRATE v$VERSION"
    
    # Determine manifest path
    if [ "$CRATE" = "llmcc" ]; then
        MANIFEST="{{root}}/crates/llmcc/Cargo.toml"
    else
        MANIFEST="{{root}}/crates/${CRATE}/Cargo.toml"
    fi
    
    if [ ! -f "$MANIFEST" ]; then
        echo "❌ Manifest not found: $MANIFEST"
        exit 1
    fi
    
    echo "✅ Found manifest: $MANIFEST"
    
    # Get current version from Cargo.toml
    CURRENT_VERSION=$(grep "^version" "$MANIFEST" | head -1 | sed 's/version = "\(.*\)"/\1/')
    echo "📝 Current version: $CURRENT_VERSION"
    echo "🎯 New version: $VERSION"
    
    # Update version in Cargo.toml
    sed -i.bak 's/^version = .*/version = "'$VERSION'"/' "$MANIFEST"
    rm -f "${MANIFEST}.bak"
    echo "✏️  Updated version in $MANIFEST"
    
    # Build and test locally
    echo "🔨 Building $CRATE..."
    cargo build --release -p "$CRATE" || { echo "❌ Build failed"; exit 1; }
    
    echo "🧪 Testing $CRATE..."
    cargo test --release -p "$CRATE" || { echo "❌ Tests failed"; exit 1; }
    
    # Commit and tag
    git add "$MANIFEST"
    git commit -m "chore: bump $CRATE to v$VERSION" || echo "⚠️  Nothing to commit"
    git push origin main || echo "⚠️  Failed to push (might already be up to date)"
    
    echo "🏷️  Creating tag: $TAG"
    git tag -a "$TAG" -m "Release $CRATE v$VERSION"
    
    echo "🚀 Pushing tag to GitHub..."
    git push origin "$TAG"
    
    echo ""
    echo "✨ Release initiated! The GitHub Actions workflow will:"
    echo "   1. Verify the build"
    echo "   2. Run tests"
    echo "   3. Publish to crates.io"
    echo "   4. Create a GitHub release"
    echo ""
    echo "📊 Monitor progress at: https://github.com/allenanswerzq/llmcc/actions"

# Release Python package: just release-python 0.2.0
release-python version:
    #!/bin/bash
    set -e
    
    VERSION="{{version}}"
    TAG="v${VERSION}"
    
    # Verify version format (e.g., 0.2.0)
    if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "Invalid version format: $VERSION (expected e.g., 0.2.0)"
        exit 1
    fi
    
    echo "📦 Preparing to release llmcc v$VERSION"
    
    # Check if files exist
    if [ ! -f "{{root}}/pyproject.toml" ] || [ ! -f "{{root}}/setup.py" ]; then
        echo "❌ Missing pyproject.toml or setup.py"
        exit 1
    fi
    
    # Get current versions
    CURRENT_PYPROJECT=$(grep "^version" "{{root}}/pyproject.toml" | sed 's/version = "\(.*\)"/\1/')
    CURRENT_SETUP=$(grep "version=" "{{root}}/setup.py" | head -1 | sed 's/.*version="\(.*\)".*/\1/')
    
    echo "📝 Current pyproject.toml version: $CURRENT_PYPROJECT"
    echo "📝 Current setup.py version: $CURRENT_SETUP"
    echo "🎯 New version: $VERSION"
    
    # Update versions in both files
    sed -i.bak 's/^version = .*/version = "'$VERSION'"/' "{{root}}/pyproject.toml"
    rm -f "{{root}}/pyproject.toml.bak"
    echo "✏️  Updated pyproject.toml"
    
    sed -i.bak 's/version=.*/version="'$VERSION'",/' "{{root}}/setup.py"
    rm -f "{{root}}/setup.py.bak"
    echo "✏️  Updated setup.py"
    
    # Commit and tag
    git add "{{root}}/pyproject.toml" "{{root}}/setup.py"
    git commit -m "chore: bump llmcc to v$VERSION" || echo "⚠️  Nothing to commit"
    git push origin main || echo "⚠️  Failed to push (might already be up to date)"
    
    echo "🏷️  Creating tag: $TAG"
    git tag -a "$TAG" -m "Release llmcc v$VERSION"
    
    echo "🚀 Pushing tag to GitHub..."
    git push origin "$TAG"
    
    echo ""
    echo "✨ Release initiated! The GitHub Actions workflow will:"
    echo "   1. Build wheels for Python 3.8-3.12"
    echo "   2. Build source distribution"
    echo "   3. Run tests on multiple platforms"
    echo "   4. Publish to PyPI"
    echo "   5. Create a GitHub release with artifacts"
    echo ""
    echo "📊 Monitor progress at: https://github.com/allenanswerzq/llmcc/actions"
    echo "⏱️  Typical duration: 30-45 minutes"

# Show release status
release-status:
    #!/bin/bash
    echo "📋 Recent git tags:"
    git tag --list --sort=-version:refname | head -10
    echo ""
    echo "📋 GitHub releases:"
    echo "https://github.com/allenanswerzq/llmcc/releases"