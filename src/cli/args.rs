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

    /// Find all references to a symbol (by position or by name)
    ///
    /// Args can be symbol names or `file:line:col` positions (auto-detected).
    /// Use `--stdin` to read positions/symbols from a pipe.
    References {
        /// Symbol names or `file:line:col` positions (auto-detected, parallel)
        #[arg(num_args = 0..)]
        queries: Vec<String>,

        /// File path (required for position mode, optional for symbol mode)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Line number (position mode, requires --file and --column)
        #[arg(short, long, requires = "file", requires = "column")]
        line: Option<u32>,

        /// Column number (position mode, requires --file and --line)
        #[arg(short, long, requires = "file", requires = "line")]
        column: Option<u32>,

        /// Read queries from stdin (one per line: symbol names or `file:line:col`)
        #[arg(long)]
        stdin: bool,

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

    /// Inspect symbols: find definition and hover info (optionally references)
    Inspect {
        /// Symbol name(s) to inspect (supports multiple symbols)
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,

        /// Optional file to narrow the search (uses workspace symbols if omitted)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Also find all references (can be slow on large codebases)
        #[arg(short, long, default_value_t = false)]
        references: bool,
    },

    /// Manage the background ty LSP server daemon
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },

    /// Generate markdown documentation from CLI help text
    #[command(hide = true)]
    GenerateDocs {
        /// Output directory for generated markdown files
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
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
