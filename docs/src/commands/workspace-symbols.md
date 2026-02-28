# workspace-symbols

Search for functions, classes, and variables by name across the whole project. Returns matching symbols with their file locations.

Examples:
  tyf workspace-symbols -q "UserService"
  tyf workspace-symbols -q "handle_"

## Usage

```
tyf workspace-symbols [OPTIONS]
```

## Options

**`-q, --query`**
:

## Examples

```bash
# Search for symbols across the workspace
tyf workspace-symbols --query MyClass
```

## See also

- [Commands Overview](overview.md)
