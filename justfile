# Default shell for all recipes
set shell := ["/bin/bash", "-c"]

root := justfile_directory()

uv-sync:
    PIP_NO_BINARY="mypy" uv sync --extra dev

build-bindings: uv-sync
    uv run maturin develop --manifest-path "{{root}}/crates/llmcc-bindings/Cargo.toml"

run-py: build-bindings verify-wheel
    uv run pytest "{{root}}/tests/test_python_api.py" -k "TestAPIExistence"

verify-wheel:
    env PYO3_PYTHON="$(python3 -c 'import sys; print(sys.executable)')" \
        uv run maturin build --release
    uv run python "{{root}}/scripts/verify_wheel.py"

test: run-py cargo-format cargo-test cargo-clippy cargo-release

cargo-format:
    cargo fmt

cargo-test:
    cargo test --workspace

cargo-clippy:
    cargo clippy --all-targets --workspace -- -D warnings

cargo-release:
    cargo build --release

clippy:
    cargo clippy --all-targets --workspace -- -D warnings

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

    echo "Updating workspace dependency versions..."
    tmpfile=$(mktemp)
    sed -E "s/^(llmcc(-[^=]*)? = \{[^}]*version = \")([^\"]+)/\1$VERSION/" "{{root}}/Cargo.toml" > "$tmpfile"
    if cmp -s "$tmpfile" "{{root}}/Cargo.toml"; then
        echo "  warning: no llmcc-* dependency versions updated"
        rm -f "$tmpfile"
    else
        mv "$tmpfile" "{{root}}/Cargo.toml"
        echo "  ok: updated llmcc-* dependency version entries"
    fi

    # Update Python package versions
    echo "Updating Python package versions..."
    if [ -f "{{root}}/pyproject.toml" ]; then
        sed -i.bak 's/^version = .*/version = "'$VERSION'"/' "{{root}}/pyproject.toml"
        rm -f "{{root}}/pyproject.toml.bak"
        git add "{{root}}/pyproject.toml"
        echo "  ok: pyproject.toml"
    fi
    if [ -f "{{root}}/llmcc/__init__.py" ]; then
        sed -i.bak 's/^__version__ = .*/__version__ = "'$VERSION'"/' "{{root}}/llmcc/__init__.py"
        rm -f "{{root}}/llmcc/__init__.py.bak"
        git add "{{root}}/llmcc/__init__.py"
        echo "  ok: llmcc/__init__.py"
    fi

    if [ -f "{{root}}/crates/llmcc-bindings/pyproject.toml" ]; then
        sed -i.bak 's/^version = .*/version = "'$VERSION'"/' "{{root}}/crates/llmcc-bindings/pyproject.toml"
        rm -f "{{root}}/crates/llmcc-bindings/pyproject.toml.bak"
        git add "{{root}}/crates/llmcc-bindings/pyproject.toml"
        echo "  ok: crates/llmcc-bindings/pyproject.toml"
    fi

    if [ -f "{{root}}/setup.py" ]; then
        sed -i.bak 's/version=.*/version="'$VERSION'",/' "{{root}}/setup.py"
        git add "{{root}}/setup.py"
        rm -f "{{root}}/setup.py.bak"
        echo "  ok: setup.py"
    fi

    env \
        PYO3_PYTHON="$(python3 -c 'import sys; print(sys.executable)')" \
        RUSTFLAGS="$(if [[ '$OSTYPE' == 'darwin'* ]]; then echo '-C link-arg=-undefined -C link-arg=dynamic_lookup'; fi)" \
        cargo build --release --workspace
    git add {{root}}/Cargo.toml
    git add {{root}}/Cargo.lock

    echo ""
    echo "Committing version bump..."
    git commit -m "chore: bump version to $VERSION"
    git push origin main

    echo "Pushing branch and tag to GitHub..."
    git tag -a $TAG -m "Release $TAG"
    git push origin "$TAG"

    echo ""
    echo "Release $VERSION published!"
    echo ""
