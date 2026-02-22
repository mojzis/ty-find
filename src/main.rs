use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use std::fmt::Write;
use std::time::Duration;

mod cli;
mod commands;
mod daemon;
mod lsp;
mod workspace;

use cli::args::{Cli, Commands};
use cli::output::OutputFormatter;
use daemon::client::DEFAULT_TIMEOUT;
use workspace::detection::WorkspaceDetector;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt().with_env_filter("ty_find=debug").init();
    }

    if let Err(e) = run(cli).await {
        eprintln!("Error: {}", format_error_chain(&e));
        #[allow(clippy::exit)]
        std::process::exit(1);
    }
}

/// Format the full anyhow error chain for display.
fn format_error_chain(error: &anyhow::Error) -> String {
    let mut chain = error.chain();
    let mut msg = chain.next().expect("error chain is never empty").to_string();
    for cause in chain {
        let _ = write!(msg, "\n  Caused by: {cause}");
    }
    msg
}

async fn run(cli: Cli) -> Result<()> {
    let workspace_root = if let Some(ws) = cli.workspace {
        ws.canonicalize().context("Failed to canonicalize workspace path")?
    } else {
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        WorkspaceDetector::find_workspace_root(&cwd)
            .unwrap_or(cwd)
            .canonicalize()
            .context("Failed to canonicalize workspace path")?
    };

    let formatter = OutputFormatter::new(cli.format);
    let timeout = cli.timeout.map_or(DEFAULT_TIMEOUT, Duration::from_secs);

    match cli.command {
        Commands::Definition { file, line, column } => {
            commands::handle_definition_command(&workspace_root, &file, line, column, &formatter)
                .await?;
        }
        Commands::Find { file, symbols } => {
            commands::handle_find_command(
                &workspace_root,
                file.as_deref(),
                &symbols,
                &formatter,
                timeout,
            )
            .await?;
        }
        Commands::Interactive { file } => {
            commands::handle_interactive_command(&workspace_root, file, &formatter).await?;
        }
        Commands::References { symbols, file, include_declaration } => {
            commands::handle_references_command(
                &workspace_root,
                file.as_deref(),
                &symbols,
                include_declaration,
                &formatter,
                timeout,
            )
            .await?;
        }
        Commands::Hover { file, line, column } => {
            commands::handle_hover_command(
                &workspace_root,
                &file,
                line,
                column,
                &formatter,
                timeout,
            )
            .await?;
        }
        Commands::WorkspaceSymbols { query } => {
            commands::handle_workspace_symbols_command(
                &workspace_root,
                &query,
                &formatter,
                timeout,
            )
            .await?;
        }
        Commands::DocumentSymbols { file } => {
            commands::handle_document_symbols_command(&workspace_root, &file, &formatter, timeout)
                .await?;
        }
        Commands::Inspect { file, symbols, references } => {
            commands::handle_inspect_command(
                &workspace_root,
                file.as_deref(),
                &symbols,
                &formatter,
                timeout,
                references,
            )
            .await?;
        }
        Commands::Daemon { command } => {
            commands::handle_daemon_command(command).await?;
        }
        Commands::GenerateDocs { output_dir } => {
            let cmd = Cli::command();
            cli::generate_docs::generate_docs(&cmd, &output_dir)?;
        }
    }

    Ok(())
}
