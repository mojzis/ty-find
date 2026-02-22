# workspace-symbols

Search for functions, classes, and variables by name across the whole project. Returns matching symbols with their file locations.

Examples:
  ty-find workspace-symbols -q "UserService"
  ty-find workspace-symbols -q "handle_"

## Usage

```
ty-find workspace-symbols [OPTIONS]
```

## Options

**`-q, --query`**
: 

## Examples

```bash
# Search for symbols across the workspace
ty-find workspace-symbols --query MyClass
```

## See also

- [Commands Overview](overview.md)
