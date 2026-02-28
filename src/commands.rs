use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
use crate::cli::args::DaemonCommands;
use crate::cli::output::{InspectEntry, OutputFormatter};
#[cfg(unix)]
use crate::daemon::client::{ensure_daemon_running, spawn_daemon, DaemonClient};
#[cfg(unix)]
use crate::daemon::protocol::BatchReferencesQuery;
#[cfg(unix)]
use crate::daemon::server::DaemonServer;
use crate::lsp::client::TyLspClient;
use crate::lsp::protocol::Location;
use crate::workspace::navigation::SymbolFinder;

/// Deduplicate locations by (uri, start line).
fn dedup_locations(locations: &mut Vec<Location>) {
    let mut seen = HashSet::new();
    locations.retain(|loc| seen.insert((loc.uri.clone(), loc.range.start.line)));
}

/// Find the column where `name` appears on a given 0-indexed line of a file.
///
/// Workspace-symbol responses return the range of the full declaration
/// (e.g. the `class` keyword), but hover/references need the cursor on the
/// *name* itself. This helper reads the source line and locates the name.
fn find_name_column(file_path: &str, line_0: u32, name: &str) -> Option<u32> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("find_name_column: cannot read {file_path}: {e}");
            return None;
        }
    };
    let Some(src_line) = content.lines().nth(line_0 as usize) else {
        tracing::debug!(
            "find_name_column: line {line_0} out of range in {file_path} ({} lines)",
            content.lines().count()
        );
        return None;
    };
    if let Some(col) = src_line.find(name) {
        tracing::debug!(
            "find_name_column: found '{name}' at col {col} on line {line_0} of {file_path}"
        );
        u32::try_from(col).ok()
    } else {
        tracing::debug!("find_name_column: '{name}' not found on line {line_0}: {:?}", src_line);
        None
    }
}

/// Try to parse a string as `file:line:col`. Returns `None` if it doesn't match.
fn parse_file_position(input: &str) -> Option<(String, u32, u32)> {
    let last_colon = input.rfind(':')?;
    let col: u32 = input[last_colon + 1..].parse().ok()?;
    let rest = &input[..last_colon];
    let second_colon = rest.rfind(':')?;
    let line: u32 = rest[second_colon + 1..].parse().ok()?;
    let file = &rest[..second_colon];
    if file.is_empty() {
        return None;
    }
    Some((file.to_string(), line, col))
}

/// A resolved reference query ready to send to the daemon.
#[cfg(unix)]
struct ResolvedQuery {
    /// Display label for output grouping
    label: String,
    /// File path for the LSP references request
    file: String,
    /// 0-based line
    line: u32,
    /// 0-based column
    column: u32,
}

/// Resolve symbol names to LSP positions via file search or workspace symbols.
#[cfg(unix)]
async fn resolve_symbols_to_queries(
    symbols: &[String],
    file: Option<&Path>,
    workspace_root: &Path,
    timeout: Duration,
) -> Result<Vec<ResolvedQuery>> {
    let mut resolved = Vec::new();

    if let Some(file) = file {
        let file_str = file.to_string_lossy();
        let finder = SymbolFinder::new(&file_str).await?;

        for symbol in symbols {
            let positions = finder.find_symbol_positions(symbol);
            if positions.is_empty() {
                resolved.push(ResolvedQuery {
                    label: symbol.clone(),
                    file: String::new(),
                    line: 0,
                    column: 0,
                });
            } else {
                for &(ln, col) in &positions {
                    resolved.push(ResolvedQuery {
                        label: symbol.clone(),
                        file: file_str.to_string(),
                        line: ln,
                        column: col,
                    });
                }
            }
        }
    } else {
        let mut client = DaemonClient::connect_with_timeout(timeout).await?;
        for symbol in symbols {
            let result = client
                .execute_workspace_symbols_exact(workspace_root.to_path_buf(), symbol.clone())
                .await?;

            if result.symbols.is_empty() {
                resolved.push(ResolvedQuery {
                    label: symbol.clone(),
                    file: String::new(),
                    line: 0,
                    column: 0,
                });
            } else {
                for sym_info in &result.symbols {
                    let file_path = sym_info
                        .location
                        .uri
                        .strip_prefix("file://")
                        .unwrap_or(&sym_info.location.uri)
                        .to_string();
                    let line = sym_info.location.range.start.line;
                    // Workspace-symbol range.start points at the declaration
                    // keyword; hover/references need the symbol *name* column.
                    let column = find_name_column(&file_path, line, &sym_info.name)
                        .unwrap_or(sym_info.location.range.start.character);
                    resolved.push(ResolvedQuery {
                        label: symbol.clone(),
                        file: file_path,
                        line,
                        column,
                    });
                }
            }
        }
    }

    Ok(resolved)
}

/// Send resolved queries to the daemon in a single batch RPC and merge results by label.
#[cfg(unix)]
async fn execute_references_batch(
    resolved: Vec<ResolvedQuery>,
    workspace_root: &Path,
    include_declaration: bool,
    timeout: Duration,
) -> Result<Vec<(String, Vec<Location>)>> {
    // Split into queries the daemon can handle (have a file) and empty ones
    let mut empty_labels: Vec<String> = Vec::new();
    let mut batch_queries: Vec<BatchReferencesQuery> = Vec::new();

    for q in resolved {
        if q.file.is_empty() {
            empty_labels.push(q.label);
        } else {
            batch_queries.push(BatchReferencesQuery {
                label: q.label,
                file: PathBuf::from(q.file),
                line: q.line,
                column: q.column,
            });
        }
    }

    let mut merged: Vec<(String, Vec<Location>)> = Vec::new();

    // Add empty results for unresolved symbols
    for label in empty_labels {
        if !merged.iter().any(|(s, _)| s == &label) {
            merged.push((label, Vec::new()));
        }
    }

    // Send the batch to the daemon in one call
    if !batch_queries.is_empty() {
        let mut client = DaemonClient::connect_with_timeout(timeout).await?;
        let result = client
            .execute_batch_references(
                workspace_root.to_path_buf(),
                batch_queries,
                include_declaration,
            )
            .await?;

        for entry in result.entries {
            if let Some(existing) = merged.iter_mut().find(|(s, _)| s == &entry.label) {
                existing.1.extend(entry.locations);
            } else {
                merged.push((entry.label, entry.locations));
            }
        }
    }

    for (_, locations) in &mut merged {
        dedup_locations(locations);
    }
    Ok(merged)
}

/// Collect query strings from CLI args and optionally stdin.
fn collect_queries(queries: &[String], read_stdin: bool) -> Result<Vec<String>> {
    let mut all = queries.to_vec();
    if read_stdin {
        let stdin = std::io::stdin();
        for line in std::io::BufRead::lines(stdin.lock()) {
            let trimmed = line?.trim().to_string();
            if !trimmed.is_empty() {
                all.push(trimmed);
            }
        }
    }
    Ok(all)
}

/// Classify queries as positions or symbols and resolve to LSP coordinates.
#[cfg(unix)]
async fn classify_and_resolve(
    all_queries: &[String],
    file: Option<&Path>,
    workspace_root: &Path,
    timeout: Duration,
) -> Result<Vec<ResolvedQuery>> {
    let mut resolved: Vec<ResolvedQuery> = Vec::new();
    let mut symbols: Vec<String> = Vec::new();

    for q in all_queries {
        if let Some((f, l, c)) = parse_file_position(q) {
            resolved.push(ResolvedQuery {
                label: q.clone(),
                file: f,
                line: l.saturating_sub(1),
                column: c.saturating_sub(1),
            });
        } else {
            symbols.push(q.clone());
        }
    }

    if !symbols.is_empty() {
        resolved.extend(resolve_symbols_to_queries(&symbols, file, workspace_root, timeout).await?);
    }

    Ok(resolved)
}

#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
pub async fn handle_references_command(
    workspace_root: &Path,
    file: Option<&Path>,
    queries: &[String],
    position: Option<(u32, u32)>,
    read_stdin: bool,
    include_declaration: bool,
    formatter: &OutputFormatter,
    timeout: Duration,
) -> Result<()> {
    ensure_daemon_running().await?;

    // Explicit --file -l -c: single position mode
    if let (Some(file), Some((line, col))) = (file, position) {
        let mut client = DaemonClient::connect_with_timeout(timeout).await?;
        let result = client
            .execute_references(
                workspace_root.to_path_buf(),
                file.to_string_lossy().to_string(),
                line.saturating_sub(1),
                col.saturating_sub(1),
                include_declaration,
            )
            .await?;

        let query_info = format!("{}:{line}:{col}", file.display());
        println!("{}", formatter.format_references(&result.locations, &query_info));
        return Ok(());
    }

    let all_queries = collect_queries(queries, read_stdin)?;
    if all_queries.is_empty() {
        anyhow::bail!(
            "Provide symbol names, file:line:col positions, or --file with --line/--column.\n\
             Position mode:  tyf refs -f file.py -l 10 -c 5\n\
             Symbol mode:    tyf refs my_func my_class\n\
             Mixed/pipe:     tyf refs file.py:10:5 my_func\n\
             Stdin:          ... | tyf refs --stdin"
        );
    }

    let resolved = classify_and_resolve(&all_queries, file, workspace_root, timeout).await?;
    let merged =
        execute_references_batch(resolved, workspace_root, include_declaration, timeout).await?;

    println!("{}", formatter.format_references_results(&merged));

    Ok(())
}

#[cfg(not(unix))]
#[allow(clippy::too_many_arguments)]
pub async fn handle_references_command(
    _workspace_root: &Path,
    _file: Option<&Path>,
    _queries: &[String],
    _position: Option<(u32, u32)>,
    _read_stdin: bool,
    _include_declaration: bool,
    _formatter: &OutputFormatter,
    _timeout: Duration,
) -> Result<()> {
    anyhow::bail!(
        "The 'refs' command requires the background daemon, which is only supported on Unix systems"
    )
}

pub async fn handle_find_command(
    workspace_root: &Path,
    file: Option<&Path>,
    symbols: &[String],
    fuzzy: bool,
    formatter: &OutputFormatter,
    timeout: Duration,
) -> Result<()> {
    // --fuzzy mode: use workspace/symbol pure fuzzy query
    if fuzzy {
        #[cfg(not(unix))]
        {
            let _ = (workspace_root, symbols, timeout);
            anyhow::bail!(
                "The --fuzzy flag requires the background daemon, which is only \
                 supported on Unix systems."
            );
        }
        #[cfg(unix)]
        {
            ensure_daemon_running().await?;
            let mut client = DaemonClient::connect_with_timeout(timeout).await?;

            for symbol in symbols {
                let result = client
                    .execute_workspace_symbols(workspace_root.to_path_buf(), symbol.clone())
                    .await?;

                if result.symbols.is_empty() {
                    println!("No symbols found matching '{symbol}'");
                } else {
                    if symbols.len() > 1 {
                        println!("=== {symbol} ({} match(es)) ===\n", result.symbols.len());
                    }
                    println!("{}", formatter.format_workspace_symbols(&result.symbols));
                }
            }
            return Ok(());
        }
    }

    let mut results: Vec<(String, Vec<Location>)> = Vec::new();

    if let Some(file) = file {
        let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;
        let file_str = file.to_string_lossy();
        let finder = SymbolFinder::new(&file_str).await?;
        client.open_document(&file_str).await?;

        for symbol in symbols {
            let positions = finder.find_symbol_positions(symbol);

            if positions.is_empty() {
                results.push((symbol.clone(), Vec::new()));
                continue;
            }

            let mut all_locations = Vec::new();
            for (line, column) in positions {
                let locations =
                    client.goto_definition(&file.to_string_lossy(), line, column).await?;
                all_locations.extend(locations);
            }
            dedup_locations(&mut all_locations);

            results.push((symbol.clone(), all_locations));
        }
    } else {
        #[cfg(not(unix))]
        {
            let _ = (workspace_root, symbols, timeout);
            anyhow::bail!(
                "Finding symbols without --file requires the background daemon, which is only \
                 supported on Unix systems. Use --file to search within a specific file instead."
            );
        }
        #[cfg(unix)]
        {
            for symbol in symbols {
                let locations = find_symbol_via_workspace(workspace_root, symbol, timeout).await?;
                results.push((symbol.clone(), locations));
            }
        }
    }

    println!("{}", formatter.format_find_results(&results));

    Ok(())
}

/// Find a symbol's location(s) using workspace symbols search.
#[cfg(unix)]
async fn find_symbol_via_workspace(
    workspace_root: &Path,
    symbol: &str,
    timeout: Duration,
) -> Result<Vec<Location>> {
    ensure_daemon_running().await?;
    let mut client = DaemonClient::connect_with_timeout(timeout).await?;

    // Use exact_name filter so the daemon only returns symbols with matching names,
    // avoiding serialization of thousands of fuzzy matches.
    let result = client
        .execute_workspace_symbols_exact(workspace_root.to_path_buf(), symbol.to_string())
        .await?;

    // If exact matches found, use them; otherwise fall back to fuzzy search.
    if !result.symbols.is_empty() {
        return Ok(result.symbols.into_iter().map(|s| s.location).collect());
    }

    // Fallback: fuzzy search (no exact_name filter), reuse the same connection
    let result =
        client.execute_workspace_symbols(workspace_root.to_path_buf(), symbol.to_string()).await?;
    Ok(result.symbols.into_iter().map(|s| s.location).collect())
}

#[cfg(unix)]
pub async fn handle_inspect_command(
    workspace_root: &Path,
    file: Option<&Path>,
    symbols: &[String],
    formatter: &OutputFormatter,
    timeout: Duration,
    include_references: bool,
) -> Result<()> {
    ensure_daemon_running().await?;

    let mut results: Vec<InspectResult> = Vec::new();
    for symbol in symbols {
        let result =
            inspect_single_symbol(workspace_root, file, symbol, timeout, include_references)
                .await?;
        results.push(result);
    }

    let entries: Vec<InspectEntry<'_>> = results
        .iter()
        .map(|r| InspectEntry {
            symbol: r.symbol.as_str(),
            kind: r.kind.as_ref(),
            definitions: r.definitions.as_slice(),
            hover: r.hover.as_ref(),
            references: r.references.as_slice(),
            references_requested: include_references,
        })
        .collect();

    println!("{}", formatter.format_inspect_results(&entries));

    Ok(())
}

#[cfg(not(unix))]
pub async fn handle_inspect_command(
    _workspace_root: &Path,
    _file: Option<&Path>,
    _symbols: &[String],
    _formatter: &OutputFormatter,
    _timeout: Duration,
    _include_references: bool,
) -> Result<()> {
    anyhow::bail!(
        "The 'inspect' command requires the background daemon, which is only supported on Unix systems"
    )
}

#[cfg(unix)]
struct InspectResult {
    symbol: String,
    kind: Option<crate::lsp::protocol::SymbolKind>,
    definitions: Vec<Location>,
    hover: Option<crate::lsp::protocol::Hover>,
    references: Vec<Location>,
}

#[cfg(unix)]
async fn inspect_single_symbol(
    workspace_root: &Path,
    file: Option<&Path>,
    symbol: &str,
    timeout: Duration,
    include_references: bool,
) -> Result<InspectResult> {
    // Step 1: Find the symbol's location(s)
    let (mut client, definition_file, def_line, def_col, all_definitions, symbol_kind) =
        if let Some(file) = file {
            let file_str = file.to_string_lossy();
            let finder = SymbolFinder::new(&file_str).await?;
            let positions = finder.find_symbol_positions(symbol);

            if positions.is_empty() {
                return Ok(InspectResult {
                    symbol: symbol.to_string(),
                    kind: None,
                    definitions: Vec::new(),
                    hover: None,
                    references: Vec::new(),
                });
            }

            let (first_line, first_col) = positions[0];

            let mut client = DaemonClient::connect_with_timeout(timeout).await?;
            let mut all_definitions = Vec::new();
            for (line, column) in &positions {
                let result = client
                    .execute_definition(
                        workspace_root.to_path_buf(),
                        file_str.to_string(),
                        *line,
                        *column,
                    )
                    .await?;
                if let Some(loc) = result.location {
                    all_definitions.push(loc);
                }
            }
            dedup_locations(&mut all_definitions);

            // File-based search doesn't provide symbol kind
            (client, file_str.to_string(), first_line, first_col, all_definitions, None)
        } else {
            // Use exact_name filter to avoid transferring thousands of fuzzy matches
            let mut client = DaemonClient::connect_with_timeout(timeout).await?;
            let result = client
                .execute_workspace_symbols_exact(workspace_root.to_path_buf(), symbol.to_string())
                .await?;

            let matched = &result.symbols;

            if matched.is_empty() {
                return Ok(InspectResult {
                    symbol: symbol.to_string(),
                    kind: None,
                    definitions: Vec::new(),
                    hover: None,
                    references: Vec::new(),
                });
            }

            let first = &matched[0];
            let file_path =
                first.location.uri.strip_prefix("file://").unwrap_or(&first.location.uri);
            let def_line = first.location.range.start.line;
            let ws_col = first.location.range.start.character;
            // Workspace-symbol range.start points at the declaration keyword
            // (e.g. "class"), but hover/references need the symbol *name* column.
            let name_col = find_name_column(file_path, def_line, &first.name);
            let def_col = name_col.unwrap_or(ws_col);
            tracing::debug!(
                "inspect: workspace-symbol col={ws_col}, name_col={name_col:?}, using col={def_col} for '{}'",
                first.name
            );
            let all_definitions: Vec<Location> =
                matched.iter().map(|s| s.location.clone()).collect();

            (
                client,
                file_path.to_string(),
                def_line,
                def_col,
                all_definitions,
                Some(first.kind.clone()),
            )
        };

    // Steps 2 & 3: Get hover info (and optionally references) via single daemon call
    tracing::debug!(
        "inspect: querying hover/refs at {definition_file}:{def_line}:{def_col} for '{symbol}'"
    );
    let inspect = client
        .execute_inspect(
            workspace_root.to_path_buf(),
            definition_file,
            def_line,
            def_col,
            include_references,
        )
        .await?;

    tracing::debug!(
        "inspect: hover={}, refs={}",
        if inspect.hover.is_some() { "present" } else { "NONE" },
        inspect.references.len()
    );

    Ok(InspectResult {
        symbol: symbol.to_string(),
        kind: symbol_kind,
        definitions: all_definitions,
        hover: inspect.hover,
        references: inspect.references,
    })
}

#[cfg(unix)]
pub async fn handle_document_symbols_command(
    workspace_root: &Path,
    file: &Path,
    formatter: &OutputFormatter,
    timeout: Duration,
) -> Result<()> {
    ensure_daemon_running().await?;
    let mut client = DaemonClient::connect_with_timeout(timeout).await?;

    let result = client
        .execute_document_symbols(workspace_root.to_path_buf(), file.to_string_lossy().to_string())
        .await?;

    if result.symbols.is_empty() {
        println!("No symbols found in {}", file.display());
    } else {
        println!("Document outline for {}:\n", file.display());
        println!("{}", formatter.format_document_symbols(&result.symbols));
    }

    Ok(())
}

#[cfg(not(unix))]
pub async fn handle_document_symbols_command(
    _workspace_root: &Path,
    _file: &Path,
    _formatter: &OutputFormatter,
    _timeout: Duration,
) -> Result<()> {
    anyhow::bail!(
        "The 'list' command requires the background daemon, which is only supported on Unix systems"
    )
}

#[cfg(unix)]
pub async fn handle_daemon_command(command: DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Start { foreground } => {
            if foreground {
                // We are the spawned child process — actually run the daemon server
                let socket_path = DaemonServer::get_socket_path();
                let server = DaemonServer::new(socket_path);
                server.start().await?;
                return Ok(());
            }

            // Check if daemon is already running
            let socket_path = crate::daemon::client::get_socket_path()?;

            if socket_path.exists() {
                match DaemonClient::connect().await {
                    Ok(_) => {
                        println!("Daemon is already running");
                        return Ok(());
                    }
                    Err(_) => {
                        // Socket exists but connection failed, clean up stale socket
                        let _ = std::fs::remove_file(&socket_path);
                    }
                }
            }

            // Spawn daemon in background
            spawn_daemon()?;

            // Wait for daemon to start
            println!("Starting daemon...");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // Verify it started
            match DaemonClient::connect().await {
                Ok(_) => println!("Daemon started successfully"),
                Err(e) => println!("Failed to start daemon: {e}"),
            }
        }

        DaemonCommands::Stop => match DaemonClient::connect().await {
            Ok(mut client) => {
                client.shutdown().await?;
                println!("Daemon stopped successfully");
            }
            Err(_) => {
                println!("Daemon is not running");
            }
        },

        DaemonCommands::Status => match DaemonClient::connect().await {
            Ok(mut client) => {
                let status = client.ping().await?;
                println!("Daemon: running");
                println!("Uptime: {}s", status.uptime);
                println!("Active workspaces: {}", status.active_workspaces);
                println!("Cache size: {}", status.cache_size);
            }
            Err(_) => {
                println!("Daemon: not running");
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_position_valid() {
        assert_eq!(parse_file_position("file.py:10:5"), Some(("file.py".to_string(), 10, 5)));
        assert_eq!(
            parse_file_position("src/foo/bar.py:1:1"),
            Some(("src/foo/bar.py".to_string(), 1, 1))
        );
        assert_eq!(
            parse_file_position("/absolute/path.py:100:20"),
            Some(("/absolute/path.py".to_string(), 100, 20))
        );
    }

    #[test]
    fn test_parse_file_position_symbol_names() {
        assert_eq!(parse_file_position("my_function"), None);
        assert_eq!(parse_file_position("MyClass"), None);
        assert_eq!(parse_file_position("foo_bar_baz"), None);
    }

    #[test]
    fn test_parse_file_position_edge_cases() {
        // Only one colon
        assert_eq!(parse_file_position("file.py:10"), None);
        // Empty file part
        assert_eq!(parse_file_position(":10:5"), None);
        // Non-numeric parts
        assert_eq!(parse_file_position("file.py:abc:5"), None);
        assert_eq!(parse_file_position("file.py:10:abc"), None);
    }

    #[test]
    fn test_find_name_column_class() {
        // "class Animal:" — "Animal" starts at column 6
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        std::fs::write(&file, "class Animal:\n    pass\n").unwrap();
        assert_eq!(find_name_column(file.to_str().unwrap(), 0, "Animal"), Some(6));
    }

    #[test]
    fn test_find_name_column_function() {
        // "def create_dog(name):" — "create_dog" starts at column 4
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        std::fs::write(&file, "def create_dog(name):\n    pass\n").unwrap();
        assert_eq!(find_name_column(file.to_str().unwrap(), 0, "create_dog"), Some(4));
    }

    #[test]
    fn test_find_name_column_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        std::fs::write(&file, "x = 1\n").unwrap();
        assert_eq!(find_name_column(file.to_str().unwrap(), 0, "Animal"), None);
    }

    #[test]
    fn test_find_name_column_nonexistent_file() {
        assert_eq!(find_name_column("/nonexistent/file.py", 0, "Animal"), None);
    }
}
