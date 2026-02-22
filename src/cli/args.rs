use clap::builder::styling::{AnsiColor, Styles};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().bold());

#[derive(Parser)]
#[command(name = "ty-find")]
#[command(about = "Find Python function definitions using ty's LSP server")]
#[command(version)]
#[command(styles = STYLES)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,

    #[arg(short, long)]
    pub verbose: bool,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    /// Timeout in seconds for daemon operations (default: 30)
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<u64>,
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

    /// Find symbol definitions by name (searches workspace if no file given)
    Find {
        /// Symbol name(s) to find (supports multiple symbols)
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,

        /// Optional file to search in (uses workspace symbols if omitted)
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

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

    /// Inspect symbols: find definition, hover info, and references in one shot
    Inspect {
        /// Symbol name(s) to inspect (supports multiple symbols)
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,

        /// Optional file to narrow the search (uses workspace symbols if omitted)
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

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
