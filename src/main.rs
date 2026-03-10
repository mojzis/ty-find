use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

mod cli;
mod commands;
#[cfg(unix)]
mod daemon;
mod debug;
mod lsp;
mod ripgrep;
mod workspace;

use cli::args::{Cli, Commands};
use cli::output::OutputFormatter;
use cli::style::{Styler, UseColor};
#[cfg(unix)]
use daemon::client::DEFAULT_TIMEOUT;
#[cfg(not(unix))]
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
use debug::DebugLog;
use workspace::detection::WorkspaceDetector;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt().with_env_filter("ty_find=debug").init();
    }

    let use_color = UseColor::resolve(&cli.color);
    let styler = Styler::new(use_color);

    // Create debug log early so we can print its path even on error
    let debug_log = if cli.debug {
        match DebugLog::create() {
            Ok(log) => Some(Arc::new(log)),
            Err(e) => {
                eprintln!("Warning: failed to create debug log: {e}");
                None
            }
        }
    } else {
        None
    };

    let result = run(cli, styler, debug_log.clone()).await;

    // Always print debug log path (even on error)
    if let Some(ref log) = debug_log {
        log.flush();
        eprintln!("Debug log: {}", log.path().display());
    }

    if let Err(e) = result {
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

/// Resolve the workspace root directory and describe the detection method.
fn resolve_workspace(explicit: Option<&Path>, cwd: &Path) -> Result<(PathBuf, String)> {
    if let Some(ws) = explicit {
        let root = ws.canonicalize().context("Failed to canonicalize workspace path")?;
        return Ok((root, "explicit --workspace flag".to_string()));
    }

    if let Some(detected) = WorkspaceDetector::find_workspace_root(cwd) {
        let method = WorkspaceDetector::describe_detection(&detected);
        let root = detected.canonicalize().context("Failed to canonicalize workspace path")?;
        Ok((root, method))
    } else {
        let root = cwd.canonicalize().context("Failed to canonicalize workspace path")?;
        Ok((root, "no project markers found, using CWD".to_string()))
    }
}

async fn run(cli: Cli, styler: Styler, debug_log: Option<Arc<DebugLog>>) -> Result<()> {
    // Log CLI args
    if let Some(ref log) = debug_log {
        let args: Vec<String> = std::env::args().collect();
        log.log_cli_args(&args);
    }

    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let (workspace_root, detection_method) = resolve_workspace(cli.workspace.as_deref(), &cwd)?;

    // Log workspace resolution
    if let Some(ref log) = debug_log {
        log.log_workspace_resolution(
            &cwd,
            &workspace_root,
            cli.workspace.as_deref(),
            &detection_method,
        );
    }

    let formatter = OutputFormatter::with_detail(cli.format, cli.detail, styler);
    let timeout = cli.timeout.map_or(DEFAULT_TIMEOUT, Duration::from_secs);

    dispatch_command(cli.command, &workspace_root, &formatter, timeout, debug_log.as_ref()).await?;

    Ok(())
}

async fn dispatch_command(
    command: Commands,
    workspace_root: &Path,
    formatter: &OutputFormatter,
    timeout: Duration,
    debug_log: Option<&Arc<DebugLog>>,
) -> Result<()> {
    match command {
        Commands::Find { file, symbols, fuzzy } => {
            commands::handle_find_command(
                workspace_root,
                file.as_deref(),
                &symbols,
                fuzzy,
                formatter,
                timeout,
                debug_log.cloned(),
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
                workspace_root,
                file.as_deref(),
                &queries,
                position,
                stdin,
                include_declaration,
                references_limit,
                formatter,
                timeout,
                tests,
                debug_log.cloned(),
            )
            .await?;
        }
        Commands::Members { file, symbols, all } => {
            commands::handle_members_command(
                workspace_root,
                file.as_deref(),
                &symbols,
                all,
                formatter,
                timeout,
                debug_log.cloned(),
            )
            .await?;
        }
        Commands::DocumentSymbols { file } => {
            commands::handle_document_symbols_command(
                workspace_root,
                &file,
                formatter,
                timeout,
                debug_log.cloned(),
            )
            .await?;
        }
        Commands::Show { file, symbols, doc, references, references_limit, tests, all } => {
            let show_doc = doc || all;
            let show_refs = references || all;
            let show_tests = tests || all;
            commands::handle_show_command(
                workspace_root,
                file.as_deref(),
                &symbols,
                formatter,
                timeout,
                show_refs,
                references_limit,
                show_tests,
                show_doc,
                debug_log.cloned(),
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
