use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

mod annotation;
mod cli;
mod commands;
#[cfg(unix)]
mod daemon;
mod debug;
mod lsp;
#[cfg(unix)]
mod mcp;
mod ripgrep;
mod workspace;

use cli::args::{Cli, Commands};
use cli::classify_error;
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
        // `tyf mcp` owns stdout for the protocol stream, so its logs go to stderr.
        let builder = tracing_subscriber::fmt().with_env_filter("ty_find=debug");
        if matches!(cli.command, Commands::Mcp { .. }) {
            builder.with_writer(std::io::stderr).init();
        } else {
            builder.init();
        }
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
        // A usage error (2) and a capability the installed ty lacks (3) each get
        // their own exit code, so a caller can tell a bad invocation and
        // "upgrade ty" apart from a clean "not found" (which exits 0).
        let report = classify_error(&e);
        eprintln!("{}", styler.error(&report.message));
        #[allow(clippy::exit)]
        std::process::exit(report.exit_code);
    }
}

/// A subcommand-level `--workspace` flag, which outranks the global one.
///
/// Harnesses spawn `tyf mcp --workspace <path>`; the global flag is not
/// `global = true`, so it cannot follow the subcommand.
fn subcommand_workspace(command: &Commands) -> Option<&Path> {
    match command {
        Commands::Mcp { workspace } => workspace.as_deref(),
        _ => None,
    }
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
    let explicit_workspace = subcommand_workspace(&cli.command).or(cli.workspace.as_deref());
    let (workspace_root, detection_method) = resolve_workspace(explicit_workspace, &cwd)?;

    // Log workspace resolution
    if let Some(ref log) = debug_log {
        log.log_workspace_resolution(&cwd, &workspace_root, explicit_workspace, &detection_method);
    }

    let formatter = OutputFormatter::with_detail(cli.format, cli.detail, styler);
    let timeout = cli.timeout.map_or(DEFAULT_TIMEOUT, Duration::from_secs);

    dispatch_command(cli.command, &workspace_root, &formatter, timeout, debug_log.as_ref()).await?;

    Ok(())
}

/// The `calls`-specific options, grouped so the dispatcher stays readable.
struct CallsOptions {
    /// `--in` was passed. `--out` is the explicit spelling of the default, and
    /// clap already rejects the two together, so only this flag needs reading.
    incoming: bool,
    depth: u32,
    external: bool,
}

/// Everything the `calls` handler needs that is not specific to `calls`.
#[derive(Clone, Copy)]
struct DispatchContext<'a> {
    formatter: &'a OutputFormatter,
    timeout: Duration,
    debug_log: Option<&'a Arc<DebugLog>>,
}

async fn dispatch_calls(
    workspace_root: &Path,
    file: Option<&Path>,
    symbols: &[String],
    opts: CallsOptions,
    ctx: DispatchContext<'_>,
) -> Result<()> {
    let direction = if opts.incoming {
        lsp::protocol::CallDirection::Incoming
    } else {
        lsp::protocol::CallDirection::Outgoing
    };
    commands::handle_calls_command(
        workspace_root,
        file,
        symbols,
        direction,
        opts.depth,
        opts.external,
        ctx.formatter,
        ctx.timeout,
        ctx.debug_log.cloned(),
    )
    .await?
    .emit();
    Ok(())
}

// One arm per subcommand, so this grows with the command surface rather than
// with any one command's complexity.
#[allow(clippy::too_many_lines)]
async fn dispatch_command(
    command: Commands,
    workspace_root: &Path,
    formatter: &OutputFormatter,
    timeout: Duration,
    debug_log: Option<&Arc<DebugLog>>,
) -> Result<()> {
    let ctx = DispatchContext { formatter, timeout, debug_log };
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
            .await?
            .emit();
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
            commands::handle_references_command(
                workspace_root,
                file.as_deref(),
                &queries,
                formatter,
                commands::RefsOptions {
                    position: line.zip(column),
                    read_stdin: stdin,
                    include_declaration,
                    references_limit,
                    tests,
                },
                timeout,
                debug_log.cloned(),
            )
            .await?
            .emit();
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
            .await?
            .emit();
        }
        Commands::Calls { symbols, incoming, outgoing: _, depth, external, file } => {
            let opts = CallsOptions { incoming, depth, external };
            dispatch_calls(workspace_root, file.as_deref(), &symbols, opts, ctx).await?;
        }
        Commands::DocumentSymbols { file } => {
            commands::handle_document_symbols_command(
                workspace_root,
                &file,
                formatter,
                timeout,
                debug_log.cloned(),
            )
            .await?
            .emit();
        }
        Commands::Show { file, symbols, doc, references, references_limit, tests, all } => {
            commands::handle_show_command(
                workspace_root,
                file.as_deref(),
                &symbols,
                formatter,
                commands::ShowOptions {
                    references: references || all,
                    references_limit,
                    tests: tests || all,
                    doc: doc || all,
                },
                timeout,
                debug_log.cloned(),
            )
            .await?
            .emit();
        }
        Commands::Mcp { workspace: _ } => {
            // Already folded into `workspace_root` by `run`.
            #[cfg(unix)]
            {
                mcp::serve_stdio(workspace_root.to_path_buf(), timeout).await?;
            }
            #[cfg(not(unix))]
            {
                anyhow::bail!(
                    "The MCP server requires the background daemon, \
                     which is only supported on Unix systems"
                );
            }
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
