use clap::builder::styling::{AnsiColor, Styles};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().bold());

const HELP_TEMPLATE: &str = "\
{name} \u{2014} {about}

{usage-heading} {usage}

Symbol Lookup:
  inspect      Definition, type signature, and usages of a symbol by name
  find         Find where a symbol is defined by name (--fuzzy for partial matching)
  refs         All usages of a symbol across the codebase (by name or file:line:col)

Browsing:
  list         All functions, classes, and variables defined in a file

Infrastructure:
  daemon       Manage the background LSP server (auto-starts on first use)

{options}";

#[derive(Parser)]
#[command(name = "tyf")]
#[command(about = "Type-aware Python code navigation (powered by ty)")]
#[command(version)]
#[command(styles = STYLES)]
#[command(help_template = HELP_TEMPLATE)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Project root (default: auto-detect)
    #[arg(long, value_name = "PATH")]
    pub workspace: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    /// Output detail level: condensed (token-efficient, default) or full (verbose)
    #[arg(long, value_enum, default_value_t = OutputDetail::Condensed)]
    pub detail: OutputDetail,

    /// Timeout in seconds for daemon operations (default: 30)
    #[arg(long, value_name = "SECS")]
    pub timeout: Option<u64>,
}

#[derive(Subcommand)]
pub enum Commands {
    // -- Symbol Lookup --
    /// Definition, type signature, and usages of a symbol by name
    #[command(
        long_about = "Definition, type signature, and usages of a symbol \u{2014} where it's defined, \
        its type signature, and optionally all usages. Searches the whole project by name, \
        no file path needed.\n\n\
        Examples:\n  \
        tyf inspect MyClass\n  \
        tyf inspect calculate_sum UserService    # multiple symbols at once\n  \
        tyf inspect MyClass --references         # also show all usages\n  \
        tyf inspect MyClass --file src/models.py # narrow to one file"
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

    /// Find where a symbol is defined by name (--fuzzy for partial matching)
    #[command(long_about = "Find where a function, class, or variable is defined. Searches the \
        whole project by name \u{2014} no need to know which file it's in.\n\n\
        Use --fuzzy for partial/prefix matching (returns richer symbol information \
        including kind and container name).\n\n\
        Examples:\n  \
        tyf find calculate_sum\n  \
        tyf find calculate_sum multiply divide   # multiple symbols at once\n  \
        tyf find handler --file src/routes.py    # narrow to one file\n  \
        tyf find handle_ --fuzzy                 # fuzzy/prefix match")]
    Find {
        /// Symbol name(s) to find (supports multiple symbols)
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,

        /// Narrow the search to a specific file (searches whole project if omitted)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Use fuzzy/prefix matching via workspace symbols (richer output with kind + container)
        #[arg(long, default_value_t = false)]
        fuzzy: bool,
    },

    /// All usages of a symbol across the codebase
    #[command(
        name = "refs",
        long_about = "All usages of a symbol across the codebase. Useful before \
        renaming or removing code to understand the impact.\n\n\
        Examples:\n  \
        tyf refs myfile.py -l 10 -c 5\n  \
        tyf refs my_func my_class\n  \
        tyf refs file.py:10:5 my_func\n  \
        ... | tyf refs --stdin"
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

    // -- Browsing --
    /// All functions, classes, and variables defined in a file
    #[command(
        name = "list",
        long_about = "All functions, classes, and variables defined in a file \u{2014} like a \
        table of contents for your code.\n\n\
        Examples:\n  \
        tyf list src/services/user.py"
    )]
    DocumentSymbols { file: PathBuf },

    // -- Infrastructure --
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

#[derive(Clone, Default, ValueEnum)]
pub enum OutputDetail {
    /// Minimal output optimized for token efficiency (default)
    #[default]
    Condensed,
    /// Verbose output with numbered lists, section headers, and full labels
    Full,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// Verify that every global option defined on `Cli` appears in `--help` output.
    /// This catches accidentally hidden flags (e.g. a stray `#[arg(hide = true)]`).
    #[test]
    fn help_shows_all_global_options() {
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        cmd.write_help(&mut buf).unwrap();
        let help = String::from_utf8(buf).unwrap();

        let expected_flags = &[
            "--workspace",
            "--verbose",
            "--format",
            "--detail",
            "--timeout",
            "--help",
            "--version",
        ];

        for flag in expected_flags {
            assert!(
                help.contains(flag),
                "Expected flag {flag} missing from help output.\nHelp text:\n{help}"
            );
        }
    }

    /// Verify that `--detail` documents both value variants.
    #[test]
    fn help_shows_detail_variants() {
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        cmd.write_long_help(&mut buf).unwrap();
        let help = String::from_utf8(buf).unwrap();

        assert!(
            help.contains("condensed"),
            "Help should mention the 'condensed' variant.\nHelp text:\n{help}"
        );
        assert!(
            help.contains("full"),
            "Help should mention the 'full' variant.\nHelp text:\n{help}"
        );
    }

    /// Verify that all subcommands appear in help (except hidden ones like generate-docs).
    #[test]
    fn help_shows_all_subcommands() {
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        cmd.write_help(&mut buf).unwrap();
        let help = String::from_utf8(buf).unwrap();

        let expected_subcommands = &["inspect", "find", "refs", "list", "daemon"];

        for subcmd in expected_subcommands {
            assert!(
                help.contains(subcmd),
                "Expected subcommand '{subcmd}' missing from help output.\nHelp text:\n{help}"
            );
        }

        // generate-docs is intentionally hidden
        assert!(
            !help.contains("generate-docs"),
            "Hidden subcommand 'generate-docs' should not appear in help.\nHelp text:\n{help}"
        );
    }
}
