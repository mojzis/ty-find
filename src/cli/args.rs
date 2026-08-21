use clap::builder::styling::{AnsiColor, Styles};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// When to use colored output.
#[derive(Clone, Default, ValueEnum)]
pub enum ColorMode {
    /// Colour if stdout is a TTY (default)
    #[default]
    Auto,
    /// Always emit ANSI colours
    Always,
    /// Never emit ANSI colours
    Never,
}

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().bold());

const HELP_TEMPLATE: &str = "\
{name} \u{2014} {about}

{usage-heading} {usage}

Symbol Lookup:
  show         Definition, signature, and usages of a symbol by name
  find         Find where a symbol is defined by name (--fuzzy for partial matching)
  refs         All usages of a symbol across the codebase (by name or file:line:col)
  members      Public interface of a class: methods, properties, and class variables
  calls        Call tree of a symbol: what it calls, or what calls it

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

    /// Write a detailed debug trace to a temp file for diagnosing issues
    #[arg(short, long)]
    pub debug: bool,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    /// Output detail level: condensed (token-efficient, default) or full (verbose)
    #[arg(long, value_enum, default_value_t = OutputDetail::Condensed)]
    pub detail: OutputDetail,

    /// Timeout in seconds for daemon operations (default: 30)
    #[arg(long, value_name = "SECS")]
    pub timeout: Option<u64>,

    /// When to use colored output [default: auto]
    #[arg(long, value_enum, default_value_t = ColorMode::Auto)]
    pub color: ColorMode,
}

#[derive(Subcommand)]
pub enum Commands {
    // -- Symbol Lookup --
    /// Definition, signature, and usages of a symbol by name
    #[command(
        name = "show",
        alias = "inspect",
        long_about = "Definition, signature, and usages of a symbol \u{2014} where it's defined, \
        its type signature, and optionally all usages. Searches the whole project by name, \
        no file path needed.\n\n\
        Use Class.member dotted notation (one level only) to narrow to a specific class \
        member. Module-qualified names (module.func) and nested paths (Outer.Inner.method) \
        are not supported; using 2+ dots is a usage error.\n\n\
        When ty cannot resolve a type (e.g. missing third-party stubs), the signature \
        shows the literal source annotation instead of 'Unknown'; a symbol with no \
        annotation is marked '(unannotated)'.\n\n\
        Examples:\n  \
        tyf show MyClass\n  \
        tyf show MyClass.get_data             # narrow to a specific class method\n  \
        tyf show calculate_sum UserService    # multiple symbols at once\n  \
        tyf show MyClass --doc                # include docstring\n  \
        tyf show MyClass --references         # also show all usages\n  \
        tyf show MyClass --all                # show everything\n  \
        tyf show MyClass --file src/models.py # narrow to one file"
    )]
    Show {
        /// Symbol name(s) to show. Use Class.member (one level) to narrow to a class member.
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,

        /// Narrow the search to a specific file (searches whole project if omitted)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Include docstring in output (omitted by default)
        #[arg(short = 'd', long, default_value_t = false)]
        doc: bool,

        /// Show individual reference locations (capped by --references-limit)
        #[arg(short, long, default_value_t = false)]
        references: bool,

        /// Maximum number of individual references to display (0 = unlimited)
        #[arg(long, default_value_t = 20)]
        references_limit: usize,

        /// Show test references in a separate section (excluded by default)
        #[arg(short = 't', long, default_value_t = false)]
        tests: bool,

        /// Show everything: doc + references + test references
        #[arg(short = 'a', long, default_value_t = false)]
        all: bool,
    },

    /// Find where a symbol is defined by name (--fuzzy for partial matching)
    #[command(long_about = "Find where a function, class, or variable is defined. Searches the \
        whole project by name \u{2014} no need to know which file it's in.\n\n\
        Use Class.member dotted notation (one level only) to narrow to a specific class \
        member. Module-qualified names (module.func) and nested paths (Outer.Inner.method) \
        are not supported; using 2+ dots is a usage error.\n\
        Use --fuzzy for partial/prefix matching (returns richer symbol information \
        including kind and container name).\n\n\
        Examples:\n  \
        tyf find calculate_sum\n  \
        tyf find Calculator.add                  # find a specific class method\n  \
        tyf find calculate_sum multiply divide   # multiple symbols at once\n  \
        tyf find handler --file src/routes.py    # narrow to one file\n  \
        tyf find handle_ --fuzzy                 # fuzzy/prefix match")]
    Find {
        /// Symbol name(s) to find. Use Class.member (one level) to narrow to a class member.
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
        Use Class.member dotted notation (one level only) to narrow to a specific class \
        member. Module-qualified names (module.func) and nested paths (Outer.Inner.method) \
        are not supported; using 2+ dots is a usage error.\n\n\
        Examples:\n  \
        tyf refs myfile.py -l 10 -c 5\n  \
        tyf refs my_func my_class\n  \
        tyf refs Calculator.add                 # refs for a specific method\n  \
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

        /// Maximum number of individual references to display (0 = unlimited)
        #[arg(long, default_value_t = 20)]
        references_limit: usize,

        /// Show test references in a separate section (excluded by default)
        #[arg(short = 't', long, default_value_t = false)]
        tests: bool,
    },

    /// Public interface of a class: methods, properties, and class variables
    #[command(
        long_about = "Public interface of a class \u{2014} methods with signatures, properties, \
        and class variables with types. Like 'list' scoped to a class, with type info included.\n\n\
        Excludes private (_prefixed) and dunder (__dunder__) members by default; \
        use --all to include everything.\n\n\
        Note: only shows members defined directly on the class, not inherited members.\n\n\
        When ty cannot resolve a member's type (e.g. missing third-party stubs), the \
        literal source annotation is shown instead of 'Unknown'; a member with no \
        annotation is marked '(unannotated)'.\n\n\
        Examples:\n  \
        tyf members MyClass\n  \
        tyf members MyClass UserService        # multiple classes\n  \
        tyf members MyClass --all              # include __init__, __repr__, etc\n  \
        tyf members MyClass -f src/models.py   # narrow to one file"
    )]
    Members {
        /// Class name(s) to query (supports multiple classes)
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,

        /// Narrow the search to a specific file (searches whole project if omitted)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Include dunder methods and private members (excluded by default)
        #[arg(long, default_value_t = false)]
        all: bool,
    },

    /// Call tree of a symbol: what it calls, or what calls it
    #[command(
        name = "calls",
        long_about = "Call tree of a symbol \u{2014} recursively, as a tree.\n\n\
        Outgoing (the default) answers \"what does this do, transitively\" in one \
        call instead of a chain of file reads. Incoming (--in) answers \"who calls \
        this\" \u{2014} impact analysis before an edit, with caller identity rather than \
        the raw text ranges 'refs' returns.\n\n\
        Use Class.member dotted notation (one level only) to narrow to a specific class \
        member. Module-qualified names (module.func) and nested paths (Outer.Inner.method) \
        are not supported; using 2+ dots is a usage error.\n\n\
        A callee already expanded elsewhere in the tree is marked with an up-arrow \
        instead of being repeated; a call that re-enters the current path is marked \
        (cycle). Code outside the workspace (stdlib, site-packages) is shown as a \
        single (external) line and is never expanded.\n\n\
        Requires a ty with call-hierarchy support (ty 0.0.41 or newer).\n\n\
        Examples:\n  \
        tyf calls process_order                 # what it calls, 2 levels deep\n  \
        tyf calls process_order --depth 3       # deeper\n  \
        tyf calls check_inventory --in          # who calls it\n  \
        tyf calls OrderPipeline.run             # a specific class method\n  \
        tyf calls a b c                         # several symbols at once\n  \
        tyf calls process_order --external      # locate stdlib callees too"
    )]
    Calls {
        /// Symbol name(s). Use Class.member (one level) to narrow to a class member.
        #[arg(required = true, num_args = 1..)]
        symbols: Vec<String>,

        /// Incoming calls: who calls this symbol
        #[arg(long = "in", default_value_t = false, conflicts_with = "outgoing")]
        incoming: bool,

        /// Outgoing calls: what this symbol calls (the default)
        #[arg(long = "out", default_value_t = false)]
        outgoing: bool,

        /// Recursion depth (max 5; larger values are clamped)
        #[arg(long, default_value_t = 2)]
        depth: u32,

        /// Show locations for out-of-workspace callees (still never expanded)
        #[arg(long, default_value_t = false)]
        external: bool,

        /// Narrow the initial symbol lookup to a specific file
        #[arg(short, long)]
        file: Option<PathBuf>,
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
    /// Stop and restart the background LSP server
    Restart,
    /// Show the daemon's running status
    Status,
}

#[derive(Clone, PartialEq, Eq, ValueEnum)]
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
            "--debug",
            "--format",
            "--detail",
            "--timeout",
            "--color",
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

    #[test]
    fn refs_accepts_tests_flag() {
        let cli = Cli::try_parse_from(["tyf", "refs", "my_func", "--tests"]).unwrap();
        match cli.command {
            Commands::References { tests, .. } => assert!(tests),
            _ => panic!("expected References"),
        }
    }

    #[test]
    fn refs_accepts_tests_short_flag() {
        let cli = Cli::try_parse_from(["tyf", "refs", "my_func", "-t"]).unwrap();
        match cli.command {
            Commands::References { tests, .. } => assert!(tests),
            _ => panic!("expected References"),
        }
    }

    #[test]
    fn refs_tests_flag_defaults_to_false() {
        let cli = Cli::try_parse_from(["tyf", "refs", "my_func"]).unwrap();
        match cli.command {
            Commands::References { tests, .. } => assert!(!tests),
            _ => panic!("expected References"),
        }
    }

    #[test]
    fn show_accepts_tests_flag() {
        let cli =
            Cli::try_parse_from(["tyf", "show", "MyClass", "--references", "--tests"]).unwrap();
        match cli.command {
            Commands::Show { tests, .. } => assert!(tests),
            _ => panic!("expected Show"),
        }
    }

    #[test]
    fn show_alias_inspect_works() {
        let cli = Cli::try_parse_from(["tyf", "inspect", "MyClass"]).unwrap();
        match cli.command {
            Commands::Show { symbols, .. } => assert_eq!(symbols, vec!["MyClass"]),
            _ => panic!("expected Show via inspect alias"),
        }
    }

    #[test]
    fn show_doc_flag_works() {
        let cli = Cli::try_parse_from(["tyf", "show", "MyClass", "--doc"]).unwrap();
        match cli.command {
            Commands::Show { doc, .. } => assert!(doc),
            _ => panic!("expected Show"),
        }
    }

    #[test]
    fn show_doc_short_flag_works() {
        let cli = Cli::try_parse_from(["tyf", "show", "MyClass", "-d"]).unwrap();
        match cli.command {
            Commands::Show { doc, .. } => assert!(doc),
            _ => panic!("expected Show"),
        }
    }

    #[test]
    fn show_all_flag_works() {
        let cli = Cli::try_parse_from(["tyf", "show", "MyClass", "--all"]).unwrap();
        match cli.command {
            Commands::Show { all, .. } => assert!(all),
            _ => panic!("expected Show"),
        }
    }

    #[test]
    fn show_all_short_flag_works() {
        let cli = Cli::try_parse_from(["tyf", "show", "MyClass", "-a"]).unwrap();
        match cli.command {
            Commands::Show { all, .. } => assert!(all),
            _ => panic!("expected Show"),
        }
    }

    #[test]
    fn calls_defaults_to_outgoing_depth_two() {
        let cli = Cli::try_parse_from(["tyf", "calls", "my_func"]).unwrap();
        match cli.command {
            Commands::Calls { symbols, incoming, outgoing, depth, external, file } => {
                assert_eq!(symbols, vec!["my_func"]);
                assert!(!incoming, "outgoing is the default direction");
                assert!(!outgoing, "the explicit --out flag defaults off");
                assert_eq!(depth, 2);
                assert!(!external);
                assert!(file.is_none());
            }
            _ => panic!("expected Calls"),
        }
    }

    #[test]
    fn calls_accepts_in_flag() {
        let cli = Cli::try_parse_from(["tyf", "calls", "my_func", "--in"]).unwrap();
        match cli.command {
            Commands::Calls { incoming, .. } => assert!(incoming),
            _ => panic!("expected Calls"),
        }
    }

    #[test]
    fn calls_accepts_out_flag() {
        let cli = Cli::try_parse_from(["tyf", "calls", "my_func", "--out"]).unwrap();
        match cli.command {
            Commands::Calls { outgoing, incoming, .. } => {
                assert!(outgoing);
                assert!(!incoming);
            }
            _ => panic!("expected Calls"),
        }
    }

    /// `--in` and `--out` are mutually exclusive: asking for both is a usage
    /// error, not a silent win for one of them.
    #[test]
    fn calls_rejects_both_directions() {
        let Err(err) = Cli::try_parse_from(["tyf", "calls", "my_func", "--in", "--out"]) else {
            panic!("--in and --out must conflict");
        };
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn calls_accepts_depth_and_external_and_file() {
        let cli = Cli::try_parse_from([
            "tyf",
            "calls",
            "my_func",
            "--depth",
            "4",
            "--external",
            "--file",
            "src/a.py",
        ])
        .unwrap();
        match cli.command {
            Commands::Calls { depth, external, file, .. } => {
                assert_eq!(depth, 4);
                assert!(external);
                assert_eq!(file, Some(PathBuf::from("src/a.py")));
            }
            _ => panic!("expected Calls"),
        }
    }

    /// An out-of-range depth is clamped by the daemon, not rejected here, so
    /// parsing must accept it.
    #[test]
    fn calls_accepts_out_of_range_depth_without_erroring() {
        let cli = Cli::try_parse_from(["tyf", "calls", "my_func", "--depth", "99"]).unwrap();
        match cli.command {
            Commands::Calls { depth, .. } => assert_eq!(depth, 99),
            _ => panic!("expected Calls"),
        }
    }

    #[test]
    fn calls_requires_at_least_one_symbol() {
        let Err(err) = Cli::try_parse_from(["tyf", "calls"]) else {
            panic!("calls needs at least one symbol");
        };
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    /// Verify that all subcommands appear in help (except hidden ones like generate-docs).
    #[test]
    fn help_shows_all_subcommands() {
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        cmd.write_help(&mut buf).unwrap();
        let help = String::from_utf8(buf).unwrap();

        let expected_subcommands = &["show", "find", "refs", "members", "calls", "list", "daemon"];

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

        // inspect alias should NOT appear in help
        assert!(
            !help.contains("inspect"),
            "Hidden alias 'inspect' should not appear in help.\nHelp text:\n{help}"
        );
    }
}
