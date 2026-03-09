# daemon

Manage the background LSP server (auto-starts on first use)

## Usage

```
tyf daemon
```

## Subcommands

**`start`**
: Start the background LSP server

**`stop`**
: Stop the background LSP server

**`restart`**
: Stop and restart the background LSP server

**`status`**
: Show the daemon's running status

## Examples

```bash
# Start the background daemon
tyf daemon start

# Restart the daemon (e.g. after upgrading tyf)
tyf daemon restart

# Check daemon status
tyf daemon status

# Stop the daemon
tyf daemon stop
```

## See also

- [Commands Overview](overview.md)
