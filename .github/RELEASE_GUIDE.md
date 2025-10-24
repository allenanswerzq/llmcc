# Release Workflows Guide

This document explains how to use the GitHub Actions release workflows for llmcc's Rust and Python packages.

## Prerequisites

Before you can use the release workflows, you need to set up the following secrets in your GitHub repository:

### For Rust Releases
1. **`CARGO_REGISTRY_TOKEN`** - Your crates.io API token
   - Go to https://crates.io/me
   - Click "API Tokens"
   - Click "New Token"
   - Copy the token and add it as a secret in GitHub

### For Python Releases
1. **`PYPI_API_TOKEN`** - Your PyPI API token
   - Go to https://pypi.org/account/
   - Click "API tokens"
   - Create a new token with "Entire account" scope
   - Copy the token and add it as a secret in GitHub

### Adding Secrets to GitHub
1. Go to your repository on GitHub
2. Click **Settings** → **Secrets and variables** → **Actions**
3. Click **New repository secret**
4. Enter the secret name and value
5. Click **Add secret**

## Rust Release Workflow

The Rust release workflow publishes Rust crates to crates.io. It's triggered by pushing a git tag.

### Supported Crates
- `llmcc-core`
- `llmcc-rust`
- `llmcc-python`
- `llmcc-bindings`
- `llmcc`

### How to Use

```bash
# 1. Update the version in the crate's Cargo.toml
# Edit crates/{crate-name}/Cargo.toml and update version = "0.2.0"

# 2. Commit the version update
git add crates/{crate-name}/Cargo.toml
git commit -m "chore: bump {crate-name} to 0.2.0"
git push origin main

# 3. Create and push a git tag to trigger the release
git tag -a {crate-name}-v0.2.0 -m "Release {crate-name} 0.2.0"
git push origin {crate-name}-v0.2.0
```

### Tag Format
- Format: `{crate-name}-v{version}`
- Examples:
  - `llmcc-core-v0.2.0`
  - `llmcc-bindings-v0.1.5`
  - `llmcc-v1.0.0`

### What the Workflow Does

1. ✅ Detects the tag push
2. ✅ Extracts crate name and version from the tag
3. ✅ Checks out your code
4. ✅ Sets up Rust toolchain
5. ✅ Builds the crate in release mode
6. ✅ Runs tests
7. ✅ Publishes to crates.io
8. ✅ Creates a GitHub release with release notes

**Note**: Make sure to update Cargo.toml and push before creating the tag.

## Python Release Workflow

The Python release workflow builds wheels for multiple Python versions and platforms, then publishes to PyPI. It's triggered by pushing a git tag.

### Supported Configurations
- **Python versions**: 3.8, 3.9, 3.10, 3.11, 3.12
- **Operating systems**: Linux (x86_64, aarch64), macOS (x86_64, arm64), Windows (x86_64)
- **Distribution**: Wheels (.whl) + Source distribution (.tar.gz)

### How to Use

```bash
# 1. Update the version in pyproject.toml and setup.py
# Edit pyproject.toml and update version = "0.2.0"
# Edit setup.py and update version="0.2.0"

# 2. Commit the version update
git add pyproject.toml setup.py
git commit -m "chore: bump llmcc to 0.2.0"
git push origin main

# 3. Create and push a git tag to trigger the release
git tag -a v0.2.0 -m "Release 0.2.0"
git push origin v0.2.0
```

### Tag Format
- Format: `v{version}`
- Examples:
  - `v0.1.0`
  - `v0.2.0`
  - `v1.0.0`

### What the Workflow Does

1. ✅ Detects the tag push
2. ✅ Extracts version from the tag
3. ✅ Builds wheels for all Python versions on all platforms (15 wheels)
4. ✅ Builds a source distribution (sdist)
5. ✅ Runs tests on a subset of configurations
6. ✅ Publishes to PyPI
7. ✅ Creates a GitHub release with wheel artifacts

### Build Matrix
The workflow builds wheels for:
- **All combinations** of 5 Python versions × 3 OSes = 15 wheels per release
- Source distribution (tarball)
- Tests run on: Ubuntu, macOS, Windows × Python 3.8, 3.11, 3.12

**Note**: Make sure to update versions in pyproject.toml and setup.py before creating the tag.

## Release Process Example

### For a Full Release (Rust + Python)

1. **Release Rust crates first** (if you have changes to Rust code):
   ```bash
   # Release llmcc-core
   sed -i 's/^version = .*/version = "0.2.0"/' crates/llmcc-core/Cargo.toml
   git add crates/llmcc-core/Cargo.toml
   git commit -m "chore: bump llmcc-core to 0.2.0"
   git push origin main
   git tag -a llmcc-core-v0.2.0 -m "Release llmcc-core 0.2.0"
   git push origin llmcc-core-v0.2.0

   # Repeat for other crates: llmcc-rust, llmcc-python, llmcc-bindings, llmcc
   ```

2. **Then release Python package**:
   ```bash
   sed -i 's/^version = .*/version = "0.2.0"/' pyproject.toml
   sed -i 's/version=.*/version="0.2.0",/' setup.py
   git add pyproject.toml setup.py
   git commit -m "chore: bump llmcc to 0.2.0"
   git push origin main
   git tag -a v0.2.0 -m "Release 0.2.0"
   git push origin v0.2.0
   ```

### For Python-Only Release

If you only have changes to Python code:
```bash
sed -i 's/^version = .*/version = "0.2.0"/' pyproject.toml
sed -i 's/version=.*/version="0.2.0",/' setup.py
git add pyproject.toml setup.py
git commit -m "chore: bump llmcc to 0.2.0"
git push origin main
git tag -a v0.2.0 -m "Release 0.2.0"
git push origin v0.2.0
```

### For Rust-Only Release

If you only have changes to Rust code:
```bash
# Update the affected crate(s)
CRATE=llmcc-core
VERSION=0.2.0
sed -i 's/^version = .*/version = "'$VERSION'"/' crates/$CRATE/Cargo.toml
git add crates/$CRATE/Cargo.toml
git commit -m "chore: bump $CRATE to $VERSION"
git push origin main
git tag -a $CRATE-v$VERSION -m "Release $CRATE $VERSION"
git push origin $CRATE-v$VERSION
```

## Version Numbering

This project uses [Semantic Versioning](https://semver.org/):
- **MAJOR.MINOR.PATCH** (e.g., `1.0.0`)
- **MAJOR**: Incompatible API changes
- **MINOR**: New functionality (backward compatible)
- **PATCH**: Bug fixes (backward compatible)

Examples:
- `0.1.0` → `0.2.0` (minor bump - new feature)
- `0.1.0` → `0.1.1` (patch bump - bug fix)
- `0.1.0` → `1.0.0` (major bump - breaking change)

## Git Tags and Releases

### Rust Crate Tags
Format: `{crate-name}-v{version}`

Examples:
- `llmcc-core-v0.2.0`
- `llmcc-bindings-v0.1.5`
- `llmcc-v1.0.0`

### Python Package Tags
Format: `v{version}`

Example:
- `v0.2.0`

## Troubleshooting

### Workflow fails during publish

**Problem**: `error: failed to authenticate with the registry`

**Solution**:
1. Verify your `CARGO_REGISTRY_TOKEN` or `PYPI_API_TOKEN` is correct
2. Check if the token has expired
3. Generate a new token and update the secret

### Version already exists on crates.io

**Problem**: `error: crate version already uploaded`

**Solution**:
1. Use a different version number
2. Make sure you're not re-releasing the same version

### Build fails for a specific platform

**Problem**: Workflow fails on macOS or Windows but succeeds on Linux

**Solution**:
1. Check the specific platform's build logs in GitHub Actions
2. Common issues:
   - Missing Rust targets: Already handled automatically
   - Platform-specific Rust code: Check for platform-specific features in `Cargo.toml`
   - Python version compatibility: Check `pyproject.toml` requirements

### PyPI rejects the wheel

**Problem**: `HTTP 403 Forbidden` when publishing

**Solution**:
1. Verify your PyPI token is for the correct PyPI account
2. Check if you have permission to publish to the `llmcc` package
3. Make sure the version is not already published

## Best Practices

1. **Test before releasing**
   - Run tests locally: `cargo test` and `pytest tests/`
   - Make sure all checks pass

2. **Update CHANGELOG**
   - Document what changed in this version
   - Include it in the GitHub release description

3. **Coordinate versions**
   - Keep Python and Rust versions in sync for easier tracking
   - Use matching version numbers for related packages

4. **Review dependencies**
   - Before releasing, check if dependencies were updated
   - Update lockfile if needed: `cargo update --dry-run`

5. **Tag naming consistency**
   - Rust: Always use `crate-name-v{version}` format
   - Python: Always use `v{version}` format

## Monitoring Releases

### Watch for workflow completion
1. Go to **Actions** tab
2. See the workflow status (success ✅ or failure ❌)
3. Click the workflow run to see detailed logs

### Verify package publication
- **Rust**: https://crates.io/crates/llmcc (wait a few minutes after workflow completes)
- **Python**: https://pypi.org/project/llmcc/ (wait a few minutes after workflow completes)

### Check GitHub releases
- Go to your repository
- Click **Releases**
- Verify the new release is listed with artifacts

## Advanced Usage

### Building locally before release
```bash
# For Rust
cargo build --release -p llmcc-core
cargo test -p llmcc-core

# For Python
pip install -e .
pytest tests/
```

### Manual version updates
If needed, you can manually update versions before running the workflow:

Rust crates:
```bash
# Edit crates/{crate-name}/Cargo.toml
# Update: version = "0.2.0"
```

Python package:
```bash
# Edit pyproject.toml and setup.py
# Update version fields
```

## FAQ

**Q: How do I trigger a release?**
A: Push a git tag matching the workflow pattern (e.g., `git tag -a llmcc-core-v0.2.0 -m "..."` and `git push origin llmcc-core-v0.2.0`).

**Q: What if I accidentally pushed a tag?**
A: Delete it locally and from GitHub:
```bash
git tag -d llmcc-core-v0.2.0
git push origin :refs/tags/llmcc-core-v0.2.0
```

**Q: Can I release multiple crates in one go?**
A: No, tag and release each crate individually. The workflows are independent.

**Q: What Python/Rust versions are supported?**
A: See the "Supported Configurations" section for details.

**Q: Can I revert a release?**
A: For PyPI and crates.io, once published, a version cannot be deleted (to prevent dependency issues). Create a new version with a fix.

**Q: How long does a Python release take?**
A: Typically 30-45 minutes due to building wheels for 15 different configurations.

**Q: What happens if the build or tests fail?**
A: The workflow stops and does not publish. Check the GitHub Actions logs to see what went wrong.
