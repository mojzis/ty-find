# Releasing

```bash
# Install cargo-release (one-time setup)
cargo install cargo-release

# Bump patch version (0.1.1 -> 0.1.2), commit, tag, and push
cargo release patch --execute

# Bump minor version (0.1.x -> 0.2.0)
cargo release minor --execute

# Dry run (default, shows what would happen without --execute)
cargo release patch
```

Version is defined in `Cargo.toml` and `pyproject.toml` picks it up automatically via `dynamic = ["version"]`. `release.toml` disables crates.io publish since we distribute via maturin/pip.
