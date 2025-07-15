"""Python wrapper for ty-find CLI tool."""

import json
import subprocess
import sys
from pathlib import Path
from typing import List, Optional, Union, Dict, Any

__version__ = "0.1.0"

class TyFindError(Exception):
    """Base exception for ty-find errors."""
    pass

class Definition:
    """Represents a function/symbol definition location."""
    
    def __init__(self, uri: str, line: int, character: int):
        self.uri = uri
        self.line = line
        self.character = character
        
    @property
    def file_path(self) -> str:
        """Get the file path from URI."""
        if self.uri.startswith("file://"):
            return self.uri[7:]
        return self.uri
    
    def __repr__(self) -> str:
        return f"Definition(file='{self.file_path}', line={self.line}, char={self.character})"

class TyFind:
    """Python interface to ty-find CLI tool."""
    
    def __init__(self, workspace: Optional[Union[str, Path]] = None):
        self.workspace = Path(workspace) if workspace else None
        
    def _run_command(self, args: List[str]) -> Dict[str, Any]:
        """Run ty-find command and return parsed JSON result."""
        cmd = ["ty-find"] + args + ["--format", "json"]
        
        if self.workspace:
            cmd.extend(["--workspace", str(self.workspace)])
            
        try:
            result = subprocess.run(
                cmd, 
                capture_output=True, 
                text=True, 
                check=True
            )
            
            if not result.stdout.strip():
                return []
                
            return json.loads(result.stdout)
        except subprocess.CalledProcessError as e:
            raise TyFindError(f"ty-find failed: {e.stderr}")
        except json.JSONDecodeError as e:
            raise TyFindError(f"Failed to parse ty-find output: {e}")
    
    def find_definition(
        self, 
        file_path: Union[str, Path], 
        line: int, 
        column: int
    ) -> List[Definition]:
        """Find definition at specific line and column."""
        args = ["definition", str(file_path), "--line", str(line), "--column", str(column)]
        locations = self._run_command(args)
        
        definitions = []
        for loc in locations:
            definitions.append(Definition(
                uri=loc["uri"],
                line=loc["range"]["start"]["line"] + 1,  # Convert to 1-based
                character=loc["range"]["start"]["character"] + 1
            ))
        
        return definitions
    
    def find_symbol(
        self, 
        file_path: Union[str, Path], 
        symbol: str
    ) -> List[Definition]:
        """Find all definitions of a symbol in a file."""
        args = ["find", str(file_path), symbol]
        locations = self._run_command(args)
        
        definitions = []
        for loc in locations:
            definitions.append(Definition(
                uri=loc["uri"],
                line=loc["range"]["start"]["line"] + 1,
                character=loc["range"]["start"]["character"] + 1
            ))
        
        return definitions

# Convenience functions for direct use
def find_definition(
    file_path: Union[str, Path], 
    line: int, 
    column: int,
    workspace: Optional[Union[str, Path]] = None
) -> List[Definition]:
    """Find definition at specific line and column."""
    finder = TyFind(workspace)
    return finder.find_definition(file_path, line, column)

def find_symbol(
    file_path: Union[str, Path], 
    symbol: str,
    workspace: Optional[Union[str, Path]] = None
) -> List[Definition]:
    """Find all definitions of a symbol in a file."""
    finder = TyFind(workspace)
    return finder.find_symbol(file_path, symbol)

def main():
    """Entry point for the ty-find command."""
    # This would call the actual Rust binary
    import os
    import shlex
    
    # Get the path to the Rust binary
    binary_path = "ty-find"  # Assume it's in PATH
    
    # Execute the Rust binary with the same arguments
    args = [binary_path] + sys.argv[1:]
    os.execvp(binary_path, args)