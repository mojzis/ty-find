use clap::Parser;
use std::path::PathBuf;
use anyhow::Result;

mod cli;
mod lsp;
mod workspace;
mod utils;

use cli::args::{Cli, Commands};
use cli::output::OutputFormatter;
use lsp::client::TyLspClient;
use workspace::navigation::SymbolFinder;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("ty_find=debug")
            .init();
    }

    let workspace_root = cli.workspace
        .unwrap_or_else(|| std::env::current_dir().unwrap())
        .canonicalize()?;

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
    }

    Ok(())
}

async fn handle_definition_command(
    workspace_root: &PathBuf,
    file: &PathBuf,
    line: u32,
    column: u32,
    formatter: &OutputFormatter,
) -> Result<()> {
    let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;
    
    client.start_response_handler().await?;
    
    let locations = client.goto_definition(
        &file.to_string_lossy(),
        line.saturating_sub(1),
        column.saturating_sub(1),
    ).await?;

    let query_info = format!("{}:{}:{}", file.display(), line, column);
    println!("{}", formatter.format_definitions(&locations, &query_info));

    Ok(())
}

async fn handle_find_command(
    workspace_root: &PathBuf,
    file: &PathBuf,
    symbol: &str,
    formatter: &OutputFormatter,
) -> Result<()> {
    let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;
    let finder = SymbolFinder::new(&file.to_string_lossy())?;
    
    client.start_response_handler().await?;
    
    let positions = finder.find_symbol_positions(symbol);
    
    if positions.is_empty() {
        println!("Symbol '{}' not found in {}", symbol, file.display());
        return Ok(());
    }

    println!("Found {} occurrence(s) of '{}' in {}:\n", positions.len(), symbol, file.display());

    for (line, column) in positions {
        let locations = client.goto_definition(
            &file.to_string_lossy(),
            line,
            column,
        ).await?;

        if !locations.is_empty() {
            let query_info = format!("{}:{}:{}", file.display(), line + 1, column + 1);
            println!("{}", formatter.format_definitions(&locations, &query_info));
        }
    }

    Ok(())
}

async fn handle_interactive_command(
    workspace_root: &PathBuf,
    initial_file: Option<PathBuf>,
    formatter: &OutputFormatter,
) -> Result<()> {
    let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;
    
    client.start_response_handler().await?;
    
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
                
                if let Err(e) = handle_find_command(workspace_root, &file, symbol, formatter).await {
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

                if let (Ok(line), Ok(column)) = (line_part.parse::<u32>(), column_part.parse::<u32>()) {
                    let file = PathBuf::from(file_part);
                    if let Err(e) = handle_definition_command(workspace_root, &file, line, column, formatter).await {
                        eprintln!("Error: {}", e);
                    }
                } else {
                    eprintln!("Invalid line or column number");
                }
            } else {
                eprintln!("Usage: <file>:<line>:<column>");
            }
        } else {
            eprintln!("Unknown command. Use: <file>:<line>:<column>, find <file> <symbol>, or quit");
        }
    }

    println!("Goodbye!");
    Ok(())
}