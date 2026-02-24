use clap::builder::styling::{AnsiColor, Styles};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().bold());

const AFTER_HELP: &str = "\x1b[1;32mQuick Reference:\x1b[0m
  \x1b[1;36mLook up symbols by name\x1b[0m (most common — just give a name, no file needed):
    ty-find inspect MyClass              Full overview: definition + type info + usages
    ty-find find calculate_sum           Jump to where a symbol is defined

  \x1b[1;36mExplore code at a specific location\x1b[0m (when you know the file + line):
    ty-find hover app.py -l 10 -c 5     Show type signature and docs
    ty-find definition app.py -l 10 -c 5 Jump to where this symbol is defined
    ty-find references app.py -l 10 -c 5 Find all usages across the codebase

  \x1b[1;36mBrowse project structure:\x1b[0m
    ty-find document-symbols app.py      List all definitions in a file
    ty-find workspace-symbols -q \"User\"  Search for symbols across the project";

#[derive(Parser)]
#[command(name = "ty-find")]
#[command(about = "Navigate Python code with type-aware precision (powered by ty's LSP server)")]
#[command(version)]
#[command(styles = STYLES)]
#[command(after_help = AFTER_HELP)]
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
    // -- Look up symbols by name (most common) --
    /// Get the full picture of a symbol: definition, type signature, and usages
    #[command(
        long_about = "Get the full picture of a symbol — where it's defined, its type signature, \
        and optionally all usages. Searches the whole project by name, no file path needed.\n\n\
        Examples:\n  \
        ty-find inspect MyClass\n  \
        ty-find inspect calculate_sum UserService    # multiple symbols at once\n  \
        ty-find inspect MyClass --references         # also show all usages\n  \
        ty-find inspect MyClass --file src/models.py # narrow to one file"
    )]
    Inspect {
        /// Symbol name(s) to inspect (supports multiple symbols)
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,

        /// Narrow the search to a specific file (searches whole project if omitted)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Also find all references (can be slow on large codebases)
        #[arg(short, long, default_value_t = false)]
        references: bool,
    },

    /// Jump to where a function, class, or variable is defined by name
    #[command(
        long_about = "Jump to where a function, class, or variable is defined. Searches the \
        whole project by name — no need to know which file it's in.\n\n\
        Examples:\n  \
        ty-find find calculate_sum\n  \
        ty-find find calculate_sum multiply divide   # multiple symbols at once\n  \
        ty-find find handler --file src/routes.py    # narrow to one file"
    )]
    Find {
        /// Symbol name(s) to find (supports multiple symbols)
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,

        /// Narrow the search to a specific file (searches whole project if omitted)
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

    // -- Explore code at a specific location --
    /// Show type signature and documentation at a specific file location
    #[command(
        long_about = "Show the type signature and documentation for the symbol at a specific \
        position in a file. Useful for understanding what a variable holds, what a function \
        returns, or what a class provides.\n\n\
        Examples:\n  \
        ty-find hover src/main.py -l 45 -c 12\n  \
        ty-find --format json hover src/main.py -l 45 -c 12   # JSON for scripting"
    )]
    Hover {
        file: PathBuf,

        #[arg(short, long)]
        line: u32,

        #[arg(short, long)]
        column: u32,
    },

    /// Jump to definition from a specific file location (line + column)
    #[command(
        long_about = "Jump to where a symbol is defined, given its exact location in a file. \
        Use this when you already know the file, line, and column (e.g., from an editor). \
        For name-based search, use 'find' or 'inspect' instead.\n\n\
        Examples:\n  \
        ty-find definition myfile.py -l 10 -c 5"
    )]
    Definition {
        file: PathBuf,

        #[arg(short, long)]
        line: u32,

        #[arg(short, long)]
        column: u32,
    },

    /// Find every place a symbol is used across the codebase
    #[command(
        long_about = "Find every place a symbol is used across the codebase. Useful before \
        renaming or removing code to understand the impact.\n\n\
        Examples:\n  \
        ty-find references myfile.py -l 10 -c 5\n  \
        ty-find references my_func my_class\n  \
        ty-find references file.py:10:5 my_func\n  \
        ... | ty-find references --stdin"
    )]
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

    // -- Browse project structure --
    /// List all functions, classes, and variables defined in a file
    #[command(
        long_about = "List all functions, classes, and variables defined in a file — like a \
        table of contents for your code.\n\n\
        Examples:\n  \
        ty-find document-symbols src/services/user.py"
    )]
    DocumentSymbols { file: PathBuf },

    /// Search for symbols by name across the whole project
    #[command(
        long_about = "Search for functions, classes, and variables by name across the whole \
        project. Returns matching symbols with their file locations.\n\n\
        Examples:\n  \
        ty-find workspace-symbols -q \"UserService\"\n  \
        ty-find workspace-symbols -q \"handle_\""
    )]
    WorkspaceSymbols {
        #[arg(short, long)]
        query: String,
    },

    // -- Other --
    /// Interactive REPL for exploring definitions
    Interactive { file: Option<PathBuf> },

    /// Manage the background LSP server (auto-starts on first use)
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
