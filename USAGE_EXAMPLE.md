# How to Add ty-find to Your Python Project

## Step 1: Add to pyproject.toml

Add this to your `pyproject.toml`:

```toml
[dependency-groups]
dev = [
    "pytest>=6.0",
    "black", 
    "isort",
    "mypy",
    "ty-find @ git+https://github.com/mojzis/ty-find.git",
]
```

Or if you're using the older format:

```toml
[project.optional-dependencies]
dev = [
    "pytest>=6.0",
    "black",
    "isort", 
    "mypy",
    "ty-find @ git+https://github.com/mojzis/ty-find.git",
]
```

## Step 2: Install dependencies

```bash
# With uv (recommended)
uv sync --group dev

# Or with pip
pip install -e ".[dev]"
```

## Step 3: Use ty-find

```bash
# Find definition at specific location
ty-find definition src/main.py --line 25 --column 10

# Find all occurrences of a symbol
ty-find find src/calculator.py calculate_sum

# Interactive mode
ty-find interactive

# Different output formats
ty-find definition src/main.py -l 25 -c 10 --format json
```

## What happens during installation

When you run `uv sync` or `pip install`:

1. **maturin** (the build backend) detects this is a Rust project
2. It automatically builds the Rust binary using `cargo`
3. The binary gets packaged into a Python wheel
4. The `ty-find` command becomes available in your environment
5. You can use it immediately without any additional setup

## Requirements

- **Rust toolchain** must be installed on the system doing the installation
- **ty** must be installed (`pip install ty`) to actually use the tool
- The installation will fail gracefully if Rust is not available

This approach ensures that when your team runs `uv sync`, they automatically get the `ty-find` tool built and ready to use!