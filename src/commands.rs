use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::args::DaemonCommands;
use crate::cli::output::{InspectEntry, OutputFormatter};
use crate::daemon::client::{ensure_daemon_running, spawn_daemon, DaemonClient};
use crate::daemon::server::DaemonServer;
use crate::lsp::client::TyLspClient;
use crate::lsp::protocol::Location;
use crate::workspace::navigation::SymbolFinder;

/// Deduplicate locations by (uri, start line).
fn dedup_locations(locations: &mut Vec<Location>) {
    let mut seen = HashSet::new();
    locations.retain(|loc| seen.insert((loc.uri.clone(), loc.range.start.line)));
}

pub async fn handle_definition_command(
    workspace_root: &Path,
    file: &Path,
    line: u32,
    column: u32,
    formatter: &OutputFormatter,
) -> Result<()> {
    let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;

    let file_str = file.to_string_lossy();
    client.open_document(&file_str).await?;

    let locations =
        client.goto_definition(&file_str, line.saturating_sub(1), column.saturating_sub(1)).await?;

    let query_info = format!("{}:{line}:{column}", file.display());
    println!("{}", formatter.format_definitions(&locations, &query_info));

    Ok(())
}

pub async fn handle_references_command(
    workspace_root: &Path,
    file: Option<&Path>,
    symbols: &[String],
    include_declaration: bool,
    formatter: &OutputFormatter,
    timeout: Duration,
) -> Result<()> {
    ensure_daemon_running().await?;

    // Resolve each symbol to a (file, line, column) position
    let mut symbol_positions: Vec<(String, String, u32, u32)> = Vec::new();

    if let Some(file) = file {
        let file_str = file.to_string_lossy();
        let finder = SymbolFinder::new(&file_str)?;

        for symbol in symbols {
            let positions = finder.find_symbol_positions(symbol);
            if let Some(&(line, col)) = positions.first() {
                symbol_positions.push((symbol.clone(), file_str.to_string(), line, col));
            } else {
                // Symbol not found in file; still include it so the output shows "no references"
                symbol_positions.push((symbol.clone(), file_str.to_string(), 0, 0));
            }
        }
    } else {
        // Workspace-wide: find each symbol's definition location first
        for symbol in symbols {
            let mut client = DaemonClient::connect_with_timeout(timeout).await?;
            let result = client
                .execute_workspace_symbols_exact(workspace_root.to_path_buf(), symbol.clone())
                .await?;

            if let Some(first) = result.symbols.first() {
                let file_path = first
                    .location
                    .uri
                    .strip_prefix("file://")
                    .unwrap_or(&first.location.uri)
                    .to_string();
                let line = first.location.range.start.line;
                let col = first.location.range.start.character;
                symbol_positions.push((symbol.clone(), file_path, line, col));
            } else {
                symbol_positions.push((symbol.clone(), String::new(), 0, 0));
            }
        }
    }

    // Search for references in parallel using tokio tasks
    let mut handles = Vec::new();
    for (symbol, file_path, line, col) in symbol_positions {
        let ws = workspace_root.to_path_buf();
        let include_decl = include_declaration;
        handles.push(tokio::spawn(async move {
            if file_path.is_empty() {
                return (symbol, Vec::new());
            }
            let result = async {
                let mut client = DaemonClient::connect_with_timeout(timeout).await?;
                client.execute_references(ws, file_path, line, col, include_decl).await
            }
            .await;

            match result {
                Ok(r) => (symbol, r.locations),
                Err(_) => (symbol, Vec::new()),
            }
        }));
    }

    let mut results: Vec<(String, Vec<Location>)> = Vec::new();
    for handle in handles {
        let (symbol, locations) = handle.await?;
        results.push((symbol, locations));
    }

    println!("{}", formatter.format_references_results(&results));

    Ok(())
}

pub async fn handle_find_command(
    workspace_root: &Path,
    file: Option<&Path>,
    symbols: &[String],
    formatter: &OutputFormatter,
    timeout: Duration,
) -> Result<()> {
    let mut results: Vec<(String, Vec<Location>)> = Vec::new();

    if let Some(file) = file {
        let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;
        let file_str = file.to_string_lossy();
        let finder = SymbolFinder::new(&file_str)?;
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
        for symbol in symbols {
            let locations = find_symbol_via_workspace(workspace_root, symbol, timeout).await?;
            results.push((symbol.clone(), locations));
        }
    }

    println!("{}", formatter.format_find_results(&results));

    Ok(())
}

/// Find a symbol's location(s) using workspace symbols search.
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

    // Fallback: fuzzy search (no exact_name filter)
    let mut client = DaemonClient::connect_with_timeout(timeout).await?;
    let result =
        client.execute_workspace_symbols(workspace_root.to_path_buf(), symbol.to_string()).await?;
    Ok(result.symbols.into_iter().map(|s| s.location).collect())
}

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
        .map(|r| {
            (r.symbol.as_str(), r.definitions.as_slice(), r.hover.as_ref(), r.references.as_slice())
        })
        .collect();

    println!("{}", formatter.format_inspect_results(&entries));

    Ok(())
}

struct InspectResult {
    symbol: String,
    definitions: Vec<Location>,
    hover: Option<crate::lsp::protocol::Hover>,
    references: Vec<Location>,
}

async fn inspect_single_symbol(
    workspace_root: &Path,
    file: Option<&Path>,
    symbol: &str,
    timeout: Duration,
    include_references: bool,
) -> Result<InspectResult> {
    // Step 1: Find the symbol's location(s)
    let (definition_file, def_line, def_col, all_definitions) = if let Some(file) = file {
        let file_str = file.to_string_lossy();
        let finder = SymbolFinder::new(&file_str)?;
        let positions = finder.find_symbol_positions(symbol);

        if positions.is_empty() {
            return Ok(InspectResult {
                symbol: symbol.to_string(),
                definitions: Vec::new(),
                hover: None,
                references: Vec::new(),
            });
        }

        let (first_line, first_col) = positions[0];

        let mut all_definitions = Vec::new();
        for (line, column) in &positions {
            let mut client = DaemonClient::connect_with_timeout(timeout).await?;
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

        (file_str.to_string(), first_line, first_col, all_definitions)
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
                definitions: Vec::new(),
                hover: None,
                references: Vec::new(),
            });
        }

        let first = &matched[0];
        let file_path = first.location.uri.strip_prefix("file://").unwrap_or(&first.location.uri);
        let def_line = first.location.range.start.line;
        let def_col = first.location.range.start.character;
        let all_definitions: Vec<Location> = matched.iter().map(|s| s.location.clone()).collect();

        (file_path.to_string(), def_line, def_col, all_definitions)
    };

    // Steps 2 & 3: Get hover info (and optionally references) via single daemon call
    let mut client = DaemonClient::connect_with_timeout(timeout).await?;
    let inspect = client
        .execute_inspect(
            workspace_root.to_path_buf(),
            definition_file,
            def_line,
            def_col,
            include_references,
        )
        .await?;

    Ok(InspectResult {
        symbol: symbol.to_string(),
        definitions: all_definitions,
        hover: inspect.hover,
        references: inspect.references,
    })
}

pub async fn handle_interactive_command(
    workspace_root: &Path,
    initial_file: Option<PathBuf>,
    formatter: &OutputFormatter,
) -> Result<()> {
    use std::io::Write as _;

    let _client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;

    println!("ty-find interactive mode");
    println!("Commands: <file>:<line>:<column>, find <file> <symbol>, quit");

    let stdin = std::io::stdin();
    drop(initial_file);

    loop {
        print!("> ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        stdin.read_line(&mut input)?;
        let input = input.trim();

        if input == "quit" || input == "q" {
            break;
        }

        if input.starts_with("find ") {
            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.len() >= 3 {
                let file = PathBuf::from(parts[1]);
                let symbols: Vec<String> = parts[2..].iter().map(|s| (*s).to_string()).collect();

                if let Err(e) = handle_find_command(
                    workspace_root,
                    Some(&file),
                    &symbols,
                    formatter,
                    crate::daemon::client::DEFAULT_TIMEOUT,
                )
                .await
                {
                    eprintln!("Error: {e}");
                }
            } else {
                eprintln!("Usage: find <file> <symbol> [symbol2 ...]");
            }
        } else if let Some(pos) = input.rfind(':') {
            if let Some(second_pos) = input[..pos].rfind(':') {
                let file_part = &input[..second_pos];
                let line_part = &input[second_pos + 1..pos];
                let column_part = &input[pos + 1..];

                if let (Ok(line), Ok(column)) =
                    (line_part.parse::<u32>(), column_part.parse::<u32>())
                {
                    let file = PathBuf::from(file_part);
                    if let Err(e) =
                        handle_definition_command(workspace_root, &file, line, column, formatter)
                            .await
                    {
                        eprintln!("Error: {e}");
                    }
                } else {
                    eprintln!("Invalid line or column number");
                }
            } else {
                eprintln!("Usage: <file>:<line>:<column>");
            }
        } else {
            eprintln!(
                "Unknown command. Use: <file>:<line>:<column>, find <file> <symbol>, or quit"
            );
        }
    }

    println!("Goodbye!");
    Ok(())
}

pub async fn handle_hover_command(
    workspace_root: &Path,
    file: &Path,
    line: u32,
    column: u32,
    formatter: &OutputFormatter,
    timeout: Duration,
) -> Result<()> {
    ensure_daemon_running().await?;
    let mut client = DaemonClient::connect_with_timeout(timeout).await?;

    let result = client
        .execute_hover(
            workspace_root.to_path_buf(),
            file.to_string_lossy().to_string(),
            line.saturating_sub(1),
            column.saturating_sub(1),
        )
        .await?;

    if let Some(hover) = result.hover {
        println!(
            "{}",
            formatter.format_hover(&hover, &format!("{}:{line}:{column}", file.display()))
        );
    } else {
        println!("No hover information found at {}:{line}:{column}", file.display());
    }

    Ok(())
}

pub async fn handle_workspace_symbols_command(
    workspace_root: &Path,
    query: &str,
    formatter: &OutputFormatter,
    timeout: Duration,
) -> Result<()> {
    ensure_daemon_running().await?;
    let mut client = DaemonClient::connect_with_timeout(timeout).await?;

    let result =
        client.execute_workspace_symbols(workspace_root.to_path_buf(), query.to_string()).await?;

    if result.symbols.is_empty() {
        println!("No symbols found matching '{query}'");
    } else {
        println!("Found {} symbol(s) matching '{query}':\n", result.symbols.len());
        println!("{}", formatter.format_workspace_symbols(&result.symbols));
    }

    Ok(())
}

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
