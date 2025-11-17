# Publishing to PyPI

This document explains how to publish ty-find to PyPI using the automated CI/CD workflows.

## Setup (One-Time)

### 1. Set up PyPI Trusted Publishing

Trusted Publishing is the modern, secure way to publish to PyPI without storing tokens.

1. Go to https://pypi.org and create an account (if you don't have one)
2. Create a "pending publisher" for your package:
   - Go to https://pypi.org/manage/account/publishing/
   - Click "Add a new pending publisher"
   - Fill in:
     - **PyPI Project Name:** `ty-find`
     - **Owner:** `mojzis` (your GitHub username)
     - **Repository name:** `ty-find`
     - **Workflow name:** `release.yml`
     - **Environment name:** `pypi`
3. Click "Add"

**Note:** After your first successful release, this will become a permanent publisher configuration.

### 2. Verify GitHub Actions are enabled

- Go to your repository settings → Actions → General
- Ensure "Allow all actions and reusable workflows" is selected
- Save if you made changes

## Publishing a Release

### Option 1: Automated Release (Recommended)

1. **Update version** in `Cargo.toml`:
   ```toml
   [package]
   version = "0.1.0"  # Bump this version
   ```

2. **Create and push a git tag**:
   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

3. **The workflow will automatically:**
   - ✅ Build wheels for Linux, macOS (Intel & Apple Silicon), and Windows
   - ✅ Build source distribution (sdist)
   - ✅ Create a GitHub Release with all artifacts
   - ✅ Publish to PyPI (if trusted publishing is set up)

4. **Monitor the workflow:**
   - Go to the "Actions" tab in your GitHub repository
   - Watch the "Release" workflow run
   - Check for any errors

5. **Verify publication:**
   - Check https://pypi.org/project/ty-find/
   - Try installing: `pip install ty-find`

### Option 2: Manual Trigger

You can also manually trigger a release build without creating a tag:

1. Go to Actions → Release workflow
2. Click "Run workflow"
3. Select the branch
4. Click "Run workflow"

This will build wheels but won't create a GitHub Release or publish to PyPI.

## Versioning Scheme

Follow semantic versioning (semver):
- **MAJOR** version for incompatible API changes
- **MINOR** version for new functionality (backwards compatible)
- **PATCH** version for bug fixes

Examples:
- `0.1.0` - Initial release
- `0.1.1` - Bug fix release
- `0.2.0` - New features added
- `1.0.0` - Stable API release

## Pre-Release Versions

For alpha/beta releases, append a suffix:
```bash
git tag v0.1.0-alpha.1
git tag v0.1.0-beta.1
git tag v0.1.0-rc.1
```

## Troubleshooting

### "Project name already exists"
If someone else already claimed `ty-find` on PyPI, you'll need to:
1. Choose a different name (e.g., `ty-find-cli`, `ty-finder`)
2. Update `pyproject.toml` with the new name
3. Update the PyPI trusted publisher configuration

### "Authentication failed"
If PyPI publication fails:
1. Verify you set up trusted publishing correctly
2. Check that the workflow name is exactly `release.yml`
3. Check that the environment name is exactly `pypi`
4. Ensure the repository owner matches

### "Version already exists"
If you try to publish the same version twice:
1. Bump the version in `Cargo.toml`
2. Create a new tag with the new version
3. PyPI doesn't allow overwriting versions (this is by design)

### Wheels not building for a platform
If a specific platform fails:
1. Check the Actions logs for that platform
2. Common issues:
   - Missing system dependencies
   - Platform-specific Rust compilation errors
3. You can disable a platform by removing it from the matrix in `release.yml`

## Testing Before Release

Before creating an official release, test the build:

```bash
# Build locally
maturin build --release

# Test the wheel
pip install target/wheels/ty_find-*.whl

# Run tests
ty-find --help
ty-find hover test_example.py --line 1 --column 1
```

## After First Release

Once published to PyPI:

1. Update the README to remove "(coming soon)" from install instructions
2. Tell users they can now install with: `pip install ty-find`
3. Consider adding a badge to README:
   ```markdown
   [![PyPI version](https://badge.fury.io/py/ty-find.svg)](https://badge.fury.io/py/ty-find)
   ```

## Continuous Updates

For subsequent releases:
1. Make your changes
2. Update `Cargo.toml` version
3. Create a new git tag
4. Push the tag
5. GitHub Actions handles the rest!
