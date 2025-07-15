# ty-find

A command-line tool for finding Python function definitions using ty's LSP server.

## Installation

### Prerequisites
- [ty](https://github.com/astral-sh/ty) type checker installed
- Rust toolchain (for building from source)

### For Python Projects

Add to your `pyproject.toml`:

```toml
# For pip/setuptools projects
[project.optional-dependencies]
dev = [
    # Other dev dependencies...
    "ty-find @ git+https://github.com/yourusername/ty-find.git",
]

# For uv projects (recommended)
[dependency-groups]
dev = [
    # Other dev dependencies...
    "ty-find @ git+https://github.com/yourusername/ty-find.git",
]
```

Then install:
```bash
# With uv (recommended - will automatically build Rust binary)
uv sync --group dev

# Or with pip
pip install -e ".[dev]"

# Or install ty-find directly
pip install "ty-find @ git+https://github.com/yourusername/ty-find.git"
```

### From Source (Rust)
```bash
git clone https://github.com/yourusername/ty-find.git
cd ty-find
cargo install --path .
```

## Usage

### Find definition at specific position
```bash
ty-find definition myfile.py --line 10 --column 5
```

### Find all definitions of a symbol
```bash
ty-find find myfile.py function_name
```

### Interactive mode
```bash
ty-find interactive
> myfile.py:10:5
> find myfile.py function_name
> quit
```

### Output formats
```bash
ty-find definition myfile.py -l 10 -c 5 --format json
ty-find definition myfile.py -l 10 -c 5 --format csv
ty-find definition myfile.py -l 10 -c 5 --format paths
```

## Examples

### Basic usage
```bash
# Find where 'calculate_total' is defined
ty-find find src/calculator.py calculate_total

# Find definition at line 25, column 10
ty-find definition src/main.py --line 25 --column 10
```

### With workspace specification
```bash
ty-find definition src/app.py -l 15 -c 8 --workspace /path/to/project
```