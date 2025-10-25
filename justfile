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

release version:
    #!/bin/bash
    set -e

    VERSION="{{version}}"
    TAG="v${VERSION}"

    # Verify version format (e.g., 0.2.0)
    if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "Invalid version format: $VERSION (expected e.g., 0.2.0)"
        exit 1
    fi

    echo "Preparing release v$VERSION"
    echo ""

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
    git push origin main

    echo "Pushing branch and tag to GitHub..."
    git tag -a $TAG -m "Release $TAG"
    git push origin "$TAG"

    echo ""
    echo "Release $VERSION published!"
    echo ""

