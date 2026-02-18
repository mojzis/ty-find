use anyhow::Result;
use clap::Parser;
use std::path::{Path, PathBuf};

mod cli;
mod daemon;
mod lsp;
mod utils;
mod workspace;

use cli::args::{Cli, Commands, DaemonCommands};
use cli::output::OutputFormatter;
use daemon::client::{ensure_daemon_running, DaemonClient};
use daemon::server::DaemonServer;
use lsp::client::TyLspClient;
use workspace::detection::WorkspaceDetector;
use workspace::navigation::SymbolFinder;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("ty_find=debug")
            .init();
    }

    let workspace_root = if let Some(ws) = cli.workspace {
        ws.canonicalize()?
    } else {
        let cwd = std::env::current_dir()?;
        WorkspaceDetector::find_workspace_root(&cwd)
            .unwrap_or(cwd)
            .canonicalize()?
    };

    let formatter = OutputFormatter::new(cli.format);

    match cli.command {
        Commands::Definition { file, line, column } => {
            handle_definition_command(&workspace_root, &file, line, column, &formatter).await?;
        }
        Commands::Find { file, symbol } => {
            handle_find_command(&workspace_root, &file, &symbol, &formatter).await?;
        }
        Commands::Interactive { file } => {
            handle_interactive_command(&workspace_root, file, &formatter).await?;
        }
        Commands::References {
            file,
            line,
            column,
            include_declaration,
        } => {
            handle_references_command(
                &workspace_root,
                &file,
                line,
                column,
                include_declaration,
                &formatter,
            )
            .await?;
        }
        Commands::Hover { file, line, column } => {
            handle_hover_command(&workspace_root, &file, line, column, &formatter).await?;
        }
        Commands::WorkspaceSymbols { query } => {
            handle_workspace_symbols_command(&workspace_root, &query, &formatter).await?;
        }
        Commands::DocumentSymbols { file } => {
            handle_document_symbols_command(&workspace_root, &file, &formatter).await?;
        }
        Commands::Inspect { file, symbol } => {
            handle_inspect_command(&workspace_root, &file, &symbol, &formatter).await?;
        }
        Commands::Daemon { command } => {
            handle_daemon_command(command).await?;
        }
    }

    Ok(())
}

async fn handle_definition_command(
    workspace_root: &Path,
    file: &Path,
    line: u32,
    column: u32,
    formatter: &OutputFormatter,
) -> Result<()> {
    let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;

    let file_str = file.to_string_lossy();
    client.open_document(&file_str).await?;

    let locations = client
        .goto_definition(&file_str, line.saturating_sub(1), column.saturating_sub(1))
        .await?;

    let query_info = format!("{}:{}:{}", file.display(), line, column);
    println!("{}", formatter.format_definitions(&locations, &query_info));

    Ok(())
}

async fn handle_references_command(
    workspace_root: &Path,
    file: &Path,
    line: u32,
    column: u32,
    include_declaration: bool,
    formatter: &OutputFormatter,
) -> Result<()> {
    ensure_daemon_running().await?;
    let mut client = DaemonClient::connect().await?;

    let result = client
        .execute_references(
            workspace_root.to_path_buf(),
            file.to_string_lossy().to_string(),
            line.saturating_sub(1),
            column.saturating_sub(1),
            include_declaration,
        )
        .await?;

    let query_info = format!("{}:{}:{}", file.display(), line, column);
    println!(
        "{}",
        formatter.format_references(&result.locations, &query_info)
    );

    Ok(())
}

async fn handle_find_command(
    workspace_root: &Path,
    file: &Path,
    symbol: &str,
    formatter: &OutputFormatter,
) -> Result<()> {
    let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;
    let file_str = file.to_string_lossy();
    let finder = SymbolFinder::new(&file_str)?;

    client.open_document(&file_str).await?;

    let positions = finder.find_symbol_positions(symbol);

    if positions.is_empty() {
        println!("Symbol '{}' not found in {}", symbol, file.display());
        return Ok(());
    }

    // Collect all definition locations across all occurrences
    let mut all_locations = Vec::new();
    for (line, column) in positions {
        let locations = client
            .goto_definition(&file.to_string_lossy(), line, column)
            .await?;
        for loc in locations {
            if !all_locations
                .iter()
                .any(|l: &crate::lsp::protocol::Location| {
                    l.uri == loc.uri && l.range.start.line == loc.range.start.line
                })
            {
                all_locations.push(loc);
            }
        }
    }

    let query_info = format!("'{}' in {}", symbol, file.display());
    println!(
        "{}",
        formatter.format_definitions(&all_locations, &query_info)
    );

    Ok(())
}

async fn handle_inspect_command(
    workspace_root: &Path,
    file: &Path,
    symbol: &str,
    formatter: &OutputFormatter,
) -> Result<()> {
    let file_str = file.to_string_lossy();
    let finder = SymbolFinder::new(&file_str)?;

    let positions = finder.find_symbol_positions(symbol);

    if positions.is_empty() {
        println!("Symbol '{}' not found in {}", symbol, file.display());
        return Ok(());
    }

    // Use the first occurrence for hover and references
    let (first_line, first_col) = positions[0];

    ensure_daemon_running().await?;
    let mut client = DaemonClient::connect().await?;

    // Get definitions from all occurrences (deduplicated, like find command)
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
            if !all_definitions
                .iter()
                .any(|l: &crate::lsp::protocol::Location| {
                    l.uri == loc.uri && l.range.start.line == loc.range.start.line
                })
            {
                all_definitions.push(loc);
            }
        }
        // Reconnect for each subsequent request since daemon uses single-request connections
        client = DaemonClient::connect().await?;
    }

    // Get hover info at first occurrence
    let hover_result = client
        .execute_hover(
            workspace_root.to_path_buf(),
            file_str.to_string(),
            first_line,
            first_col,
        )
        .await?;

    // Reconnect for references
    let mut client = DaemonClient::connect().await?;

    // Get references at first occurrence
    let refs_result = client
        .execute_references(
            workspace_root.to_path_buf(),
            file_str.to_string(),
            first_line,
            first_col,
            true,
        )
        .await?;

    println!(
        "{}",
        formatter.format_inspect(
            symbol,
            &all_definitions,
            hover_result.hover.as_ref(),
            &refs_result.locations,
        )
    );

    Ok(())
}

async fn handle_interactive_command(
    workspace_root: &Path,
    initial_file: Option<PathBuf>,
    formatter: &OutputFormatter,
) -> Result<()> {
    let _client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;

    println!("ty-find interactive mode");
    println!("Commands: <file>:<line>:<column>, find <file> <symbol>, quit");

    let stdin = std::io::stdin();
    let _current_file = initial_file;

    loop {
        print!("> ");
        use std::io::Write;
        std::io::stdout().flush().unwrap();

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
                let symbol = parts[2];

                if let Err(e) = handle_find_command(workspace_root, &file, symbol, formatter).await
                {
                    eprintln!("Error: {}", e);
                }
            } else {
                eprintln!("Usage: find <file> <symbol>");
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
                        eprintln!("Error: {}", e);
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

async fn handle_hover_command(
    workspace_root: &Path,
    file: &Path,
    line: u32,
    column: u32,
    formatter: &OutputFormatter,
) -> Result<()> {
    ensure_daemon_running().await?;
    let mut client = DaemonClient::connect().await?;

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
            formatter.format_hover(&hover, &format!("{}:{}:{}", file.display(), line, column))
        );
    } else {
        println!(
            "No hover information found at {}:{}:{}",
            file.display(),
            line,
            column
        );
    }

    Ok(())
}

async fn handle_workspace_symbols_command(
    workspace_root: &Path,
    query: &str,
    formatter: &OutputFormatter,
) -> Result<()> {
    ensure_daemon_running().await?;
    let mut client = DaemonClient::connect().await?;

    let result = client
        .execute_workspace_symbols(workspace_root.to_path_buf(), query.to_string())
        .await?;

    if result.symbols.is_empty() {
        println!("No symbols found matching '{}'", query);
    } else {
        println!(
            "Found {} symbol(s) matching '{}':\n",
            result.symbols.len(),
            query
        );
        println!("{}", formatter.format_workspace_symbols(&result.symbols));
    }

    Ok(())
}

async fn handle_document_symbols_command(
    workspace_root: &Path,
    file: &Path,
    formatter: &OutputFormatter,
) -> Result<()> {
    ensure_daemon_running().await?;
    let mut client = DaemonClient::connect().await?;

    let result = client
        .execute_document_symbols(
            workspace_root.to_path_buf(),
            file.to_string_lossy().to_string(),
        )
        .await?;

    if result.symbols.is_empty() {
        println!("No symbols found in {}", file.display());
    } else {
        println!("Document outline for {}:\n", file.display());
        println!("{}", formatter.format_document_symbols(&result.symbols));
    }

    Ok(())
}

async fn handle_daemon_command(command: DaemonCommands) -> Result<()> {
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
            let socket_path = daemon::client::get_socket_path()?;

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
            DaemonServer::spawn_background()?;

            // Wait for daemon to start
            println!("Starting daemon...");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // Verify it started
            match DaemonClient::connect().await {
                Ok(_) => println!("Daemon started successfully"),
                Err(e) => println!("Failed to start daemon: {}", e),
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
