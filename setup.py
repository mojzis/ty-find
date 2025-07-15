#!/usr/bin/env python3
"""Setup script for ty-find."""

import os
import subprocess
import sys
from pathlib import Path
from setuptools import setup
from setuptools.command.install import install


class CargoInstall(install):
    """Custom install command that builds and installs the Rust binary."""
    
    def run(self):
        # Run the normal install first
        install.run(self)
        
        # Build the Rust binary
        print("Building ty-find Rust binary...")
        try:
            subprocess.check_call(["cargo", "build", "--release"])
            
            # Copy binary to Python's bin directory
            binary_src = Path("target/release/ty-find")
            if sys.platform == "win32":
                binary_src = Path("target/release/ty-find.exe")
            
            if binary_src.exists():
                # Install to user's local bin or site-packages bin
                bin_dir = Path(sys.prefix) / "bin"
                bin_dir.mkdir(exist_ok=True)
                
                import shutil
                shutil.copy2(binary_src, bin_dir / binary_src.name)
                print(f"Installed ty-find to {bin_dir / binary_src.name}")
            else:
                print("Warning: Rust binary not found after build")
                
        except subprocess.CalledProcessError:
            print("Error: Failed to build Rust binary. Make sure Rust is installed.")
            sys.exit(1)
        except FileNotFoundError:
            print("Error: cargo not found. Please install Rust.")
            sys.exit(1)


setup(
    name="ty-find",
    version="0.1.0",
    description="CLI tool for finding Python function definitions using ty's LSP server",
    long_description=open("README.md").read(),
    long_description_content_type="text/markdown",
    author="Your Name",
    author_email="your.email@example.com",
    url="https://github.com/yourusername/ty-find",
    classifiers=[
        "Development Status :: 4 - Beta",
        "Intended Audience :: Developers",
        "License :: OSI Approved :: MIT License",
        "Programming Language :: Rust",
        "Programming Language :: Python :: 3",
        "Topic :: Software Development :: Tools",
    ],
    python_requires=">=3.8",
    cmdclass={
        "install": CargoInstall,
    },
    # No Python packages to install, just the binary
    packages=[],
)