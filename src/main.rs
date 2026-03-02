use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use std::fmt::Write;
use std::time::Duration;

mod cli;
mod commands;
#[cfg(unix)]
mod daemon;
mod lsp;
mod workspace;

use cli::args::{Cli, Commands};
use cli::output::OutputFormatter;
use cli::style::{Styler, UseColor};
#[cfg(unix)]
use daemon::client::DEFAULT_TIMEOUT;
#[cfg(not(unix))]
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
use workspace::detection::WorkspaceDetector;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt().with_env_filter("ty_find=debug").init();
    }

    let use_color = UseColor::resolve(&cli.color);
    let styler = Styler::new(use_color);

    if let Err(e) = run(cli, styler).await {
        eprintln!("{}", styler.error(&format!("Error: {}", format_error_chain(&e))));
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

async fn run(cli: Cli, styler: Styler) -> Result<()> {
    let workspace_root = if let Some(ws) = cli.workspace {
        ws.canonicalize().context("Failed to canonicalize workspace path")?
    } else {
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        WorkspaceDetector::find_workspace_root(&cwd)
            .unwrap_or(cwd)
            .canonicalize()
            .context("Failed to canonicalize workspace path")?
    };

    let formatter = OutputFormatter::with_detail(cli.format, cli.detail, styler);
    let timeout = cli.timeout.map_or(DEFAULT_TIMEOUT, Duration::from_secs);

    match cli.command {
        Commands::Find { file, symbols, fuzzy } => {
            commands::handle_find_command(
                &workspace_root,
                file.as_deref(),
                &symbols,
                fuzzy,
                &formatter,
                timeout,
            )
            .await?;
        }
        Commands::References {
            queries,
            file,
            line,
            column,
            stdin,
            include_declaration,
            references_limit,
            tests,
        } => {
            let position = line.zip(column);
            commands::handle_references_command(
                &workspace_root,
                file.as_deref(),
                &queries,
                position,
                stdin,
                include_declaration,
                references_limit,
                &formatter,
                timeout,
                tests,
            )
            .await?;
        }
        Commands::Members { file, symbols, all } => {
            commands::handle_members_command(
                &workspace_root,
                file.as_deref(),
                &symbols,
                all,
                &formatter,
                timeout,
            )
            .await?;
        }
        Commands::DocumentSymbols { file } => {
            commands::handle_document_symbols_command(&workspace_root, &file, &formatter, timeout)
                .await?;
        }
        Commands::Inspect { file, symbols, references, references_limit, tests } => {
            commands::handle_inspect_command(
                &workspace_root,
                file.as_deref(),
                &symbols,
                &formatter,
                timeout,
                references,
                references_limit,
                tests,
            )
            .await?;
        }
        Commands::Daemon { command } => {
            #[cfg(unix)]
            {
                commands::handle_daemon_command(command).await?;
            }
            #[cfg(not(unix))]
            {
                let _ = command;
                anyhow::bail!("Daemon commands are only supported on Unix systems");
            }
        }
        Commands::GenerateDocs { output_dir } => {
            let cmd = Cli::command();
            cli::generate_docs::generate_docs(&cmd, &output_dir)?;
        }
    }

    Ok(())
}
