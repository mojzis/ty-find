use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ty-find")]
#[command(about = "Find Python function definitions using ty's LSP server")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,

    #[arg(short, long)]
    pub verbose: bool,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Go to definition at a specific file location
    Definition {
        file: PathBuf,

        #[arg(short, long)]
        line: u32,

        #[arg(short, long)]
        column: u32,
    },

    /// Find a symbol by name in a file
    Find { file: PathBuf, symbol: String },

    /// Interactive REPL for exploring definitions
    Interactive { file: Option<PathBuf> },

    /// Show hover information at a specific file location
    Hover {
        file: PathBuf,

        #[arg(short, long)]
        line: u32,

        #[arg(short, long)]
        column: u32,
    },

    /// Find all references to a symbol at a specific file location
    References {
        file: PathBuf,

        #[arg(short, long)]
        line: u32,

        #[arg(short, long)]
        column: u32,

        /// Include the declaration in the results
        #[arg(long, default_value_t = true)]
        include_declaration: bool,
    },

    /// Search for symbols across the workspace
    WorkspaceSymbols {
        #[arg(short, long)]
        query: String,
    },

    /// List all symbols in a file
    DocumentSymbols { file: PathBuf },

    /// Manage the background ty LSP server daemon
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
}

#[derive(Subcommand)]
pub enum DaemonCommands {
    /// Start the background LSP server
    Start {
        /// Run the daemon in the foreground (used internally by the spawned process)
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the background LSP server
    Stop,
    /// Show the daemon's running status
    Status,
}

#[derive(Clone, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
    Csv,
    Paths,
}
