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

release-stage version:
    #!/bin/bash
    set -e

    VERSION="{{version}}"
    BRANCH="release-v${VERSION}"

    # Verify version format (e.g., 0.2.0)
    if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "Invalid version format: $VERSION (expected e.g., 0.2.0)"
        exit 1
    fi

    echo "Preparing release v$VERSION"
    echo ""

    # Check if branch already exists
    if git show-ref --quiet refs/heads/"$BRANCH"; then
        echo "Branch $BRANCH already exists!"
        exit 1
    fi

    # Create release branch from main
    echo "Creating branch: $BRANCH"
    git checkout main
    git pull origin main
    git checkout -b "$BRANCH"

    # Update workspace version in root Cargo.toml
    echo ""
    echo "Updating workspace version in Cargo.toml..."
    sed -i.bak 's/^version = .*/version = "'$VERSION'"/' "{{root}}/Cargo.toml"
    rm -f "{{root}}/Cargo.toml.bak"
    echo "  ok: Cargo.toml"

    # Update Python package versions
    echo "Updating Python package versions..."
    if [ -f "{{root}}/pyproject.toml" ]; then
        sed -i.bak 's/^version = .*/version = "'$VERSION'"/' "{{root}}/pyproject.toml"
        rm -f "{{root}}/pyproject.toml.bak"
        echo "  ok: pyproject.toml"
    fi

    if [ -f "{{root}}/crates/llmcc-bindings/pyproject.toml" ]; then
        sed -i.bak 's/^version = .*/version = "'$VERSION'"/' "{{root}}/crates/llmcc-bindings/pyproject.toml"
        rm -f "{{root}}/crates/llmcc-bindings/pyproject.toml.bak"
        echo "  ok: crates/llmcc-bindings/pyproject.toml"
    fi

    if [ -f "{{root}}/setup.py" ]; then
        sed -i.bak 's/version=.*/version="'$VERSION'",/' "{{root}}/setup.py"
        rm -f "{{root}}/setup.py.bak"
        echo "  ok: setup.py"
    fi

    echo ""
    echo "Building all crates..."
    cargo build --release 2>&1 | grep -E "^(Compiling|Finished|error)" || true

    echo ""
    echo "Testing all crates..."
    cargo test --release 2>&1 | grep -E "^(running|test result)" || true

    echo ""
    echo "Committing version bump..."
    git add {{root}}/Cargo.toml {{root}}/pyproject.toml {{root}}/crates/llmcc-bindings/pyproject.toml {{root}}/setup.py {{root}}/Cargo.lock
    git commit -m "chore: bump version to $VERSION"
    git push origin "$BRANCH"


release-publish version:
    #!/bin/bash
    set -e

    VERSION="{{version}}"
    TAG="v${VERSION}"
    BRANCH="release-v${VERSION}"

    # Verify version format
    if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "Invalid version format: $VERSION (expected e.g., 0.2.0)"
        exit 1
    fi

    echo "Publishing release v$VERSION"
    echo ""

    # Verify we're on the release branch
    CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
    if [ "$CURRENT_BRANCH" != "$BRANCH" ]; then
        echo "Not on release branch! Current: $CURRENT_BRANCH, Expected: $BRANCH"
        exit 1
    fi

    # Verify no uncommitted changes
    if ! git diff-index --quiet HEAD --; then
        echo "Uncommitted changes detected!"
        git status
        exit 1
    fi

    echo ""
    echo "Creating tag: $TAG"
    git tag -a "$TAG" -m "Release llmcc v$VERSION"

    echo "Pushing branch and tag to GitHub..."
    git push origin "$TAG"

    echo ""
    echo "Release $VERSION published!"
    echo ""
