"""ty-find: CLI tool for finding Python function definitions using ty's LSP server."""

import os
import sys

def main():
    """Entry point that calls the Rust binary."""
    # This function will be called when someone runs `ty-find` command
    # The actual binary will be installed by maturin alongside this Python package
    
    # Find the binary that maturin installed
    import importlib.util
    import pathlib
    
    # Get the directory where this Python package is installed
    package_dir = pathlib.Path(__file__).parent
    
    # Look for the binary in the same directory or in a bin subdirectory
    binary_name = "ty-find"
    if sys.platform == "win32":
        binary_name += ".exe"
    
    possible_paths = [
        package_dir / binary_name,
        package_dir / "bin" / binary_name,
        package_dir.parent / "bin" / binary_name,
    ]
    
    binary_path = None
    for path in possible_paths:
        if path.exists() and path.is_file():
            binary_path = path
            break
    
    if binary_path is None:
        print("Error: ty-find binary not found", file=sys.stderr)
        sys.exit(1)
    
    # Execute the binary with the same arguments
    os.execv(str(binary_path), [str(binary_path)] + sys.argv[1:])

if __name__ == "__main__":
    main()