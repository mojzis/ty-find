#!/usr/bin/env python3
"""Test script to verify ty-find installation works correctly."""

import subprocess
import sys
import shutil

def test_binary_exists():
    """Test that ty-find binary is available."""
    if shutil.which("ty-find"):
        print("✓ ty-find binary found in PATH")
        return True
    else:
        print("✗ ty-find binary not found in PATH")
        return False

def test_help_command():
    """Test that ty-find --help works."""
    try:
        result = subprocess.run(
            ["ty-find", "--help"], 
            capture_output=True, 
            text=True, 
            timeout=10
        )
        if result.returncode == 0:
            print("✓ ty-find --help works")
            return True
        else:
            print(f"✗ ty-find --help failed with code {result.returncode}")
            print(f"  stderr: {result.stderr}")
            return False
    except subprocess.TimeoutExpired:
        print("✗ ty-find --help timed out")
        return False
    except FileNotFoundError:
        print("✗ ty-find command not found")
        return False

def test_python_import():
    """Test that the Python package can be imported."""
    try:
        import ty_find
        print("✓ ty_find Python package can be imported")
        return True
    except ImportError as e:
        print(f"✗ Failed to import ty_find: {e}")
        return False

def main():
    """Run all tests."""
    print("Testing ty-find installation...\n")
    
    tests = [
        test_python_import,
        test_binary_exists,
        test_help_command,
    ]
    
    results = []
    for test in tests:
        results.append(test())
        print()
    
    if all(results):
        print("🎉 All tests passed! ty-find is correctly installed.")
        sys.exit(0)
    else:
        print("❌ Some tests failed. Check the output above.")
        sys.exit(1)

if __name__ == "__main__":
    main()