use anyhow::{Context, Result};
use clap::Command;
use std::fmt::Write;
use std::path::Path;

/// Generate markdown documentation for all CLI commands.
///
/// Produces:
/// - `overview.md` — the top-level help formatted as markdown
/// - One file per subcommand (e.g., `find.md`, `inspect.md`)
pub fn generate_docs(cmd: &Command, output_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create output directory: {}", output_dir.display()))?;

    // Generate overview.md from the top-level command
    let overview = render_overview(cmd);
    let overview_path = output_dir.join("overview.md");
    std::fs::write(&overview_path, &overview)
        .with_context(|| format!("Failed to write {}", overview_path.display()))?;
    println!("  wrote {}", overview_path.display());

    // Generate one file per visible subcommand
    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() {
            continue;
        }

        let name = sub.get_name().to_string();
        let filename = format!("{name}.md");
        let content = render_subcommand(sub, &name);
        let path = output_dir.join(&filename);
        std::fs::write(&path, &content)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        println!("  wrote {}", path.display());
    }

    Ok(())
}

/// Extract the plain-text string from a clap help value.
fn help_text(styled: &clap::builder::StyledStr) -> String {
    styled.to_string()
}

/// Render the top-level overview page.
fn render_overview(cmd: &Command) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "# Commands Overview\n");

    if let Some(about) = cmd.get_about() {
        let _ = writeln!(out, "{}\n", help_text(about));
    }

    // Usage
    let _ = writeln!(out, "## Usage\n");
    let _ = writeln!(out, "```");
    let _ = writeln!(out, "{} [OPTIONS] <COMMAND>", cmd.get_name());
    let _ = writeln!(out, "```\n");

    // Global options
    let global_opts: Vec<_> = cmd.get_opts().collect();
    if !global_opts.is_empty() {
        let _ = writeln!(out, "## Global Options\n");
        for opt in &global_opts {
            let long = opt.get_long().map_or_else(String::new, |l| format!("--{l}"));
            let short = opt.get_short().map_or_else(String::new, |s| format!("-{s}, "));
            let desc = opt.get_help().map_or_else(String::new, help_text);
            let _ = writeln!(out, "**`{short}{long}`**");
            if desc.is_empty() {
                let _ = writeln!(out);
            } else {
                let _ = writeln!(out, ": {desc}\n");
            }
        }
    }

    // Commands list
    let _ = writeln!(out, "## Commands\n");
    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() {
            continue;
        }
        let name = sub.get_name();
        let about = sub.get_about().map_or_else(String::new, help_text);
        let _ = writeln!(out, "**[{name}]({name}.md)**");
        let _ = writeln!(out, ": {about}\n");
    }

    out
}

/// Render a subcommand page.
fn render_subcommand(cmd: &Command, name: &str) -> String {
    let mut out = String::new();

    // Title
    let _ = writeln!(out, "# {name}\n");

    // Description
    if let Some(about) = cmd.get_long_about().or_else(|| cmd.get_about()) {
        let _ = writeln!(out, "{}\n", help_text(about));
    }

    // Usage
    let _ = writeln!(out, "## Usage\n");
    let _ = writeln!(out, "```");
    let _ = write!(out, "tyf {name}");
    // Positional arguments
    for arg in cmd.get_positionals() {
        let val = arg
            .get_value_names()
            .map_or_else(|| arg.get_id().to_string().to_uppercase(), |v| v.join(" "));
        if arg.is_required_set() {
            let _ = write!(out, " <{val}>");
        } else {
            let _ = write!(out, " [{val}]");
        }
    }
    // Indicate options exist
    if cmd.get_opts().next().is_some() {
        let _ = write!(out, " [OPTIONS]");
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "```\n");

    // Positional arguments section
    let positionals: Vec<_> = cmd.get_positionals().collect();
    if !positionals.is_empty() {
        let _ = writeln!(out, "## Arguments\n");
        for arg in &positionals {
            let arg_name = arg.get_id();
            let desc = arg.get_help().map_or_else(String::new, help_text);
            let required = if arg.is_required_set() { " *(required)*" } else { "" };
            let _ = writeln!(out, "**`<{arg_name}>`**{required}");
            let _ = writeln!(out, ": {desc}\n");
        }
    }

    // Named options
    let opts: Vec<_> = cmd.get_opts().collect();
    if !opts.is_empty() {
        let _ = writeln!(out, "## Options\n");
        for opt in &opts {
            let long = opt.get_long().map_or_else(String::new, |l| format!("--{l}"));
            let short = opt.get_short().map_or_else(String::new, |s| format!("-{s}, "));
            let desc = opt.get_help().map_or_else(String::new, help_text);
            let _ = writeln!(out, "**`{short}{long}`**");
            let _ = writeln!(out, ": {desc}\n");
        }
    }

    // Nested subcommands (e.g., daemon start/stop/status)
    let subs: Vec<_> = cmd.get_subcommands().filter(|s| !s.is_hide_set()).collect();
    if !subs.is_empty() {
        let _ = writeln!(out, "## Subcommands\n");
        for sub in &subs {
            let sub_name = sub.get_name();
            let about = sub.get_about().map_or_else(String::new, help_text);
            let _ = writeln!(out, "**`{sub_name}`**");
            let _ = writeln!(out, ": {about}\n");
        }
    }

    // Examples section
    let _ = writeln!(out, "## Examples\n");
    let _ = writeln!(out, "```bash");
    write_examples(&mut out, name, cmd);
    let _ = writeln!(out, "```\n");

    // See also
    let _ = writeln!(out, "## See also\n");
    let _ = writeln!(out, "- [Commands Overview](overview.md)");

    out
}

/// Write example usages for a given command.
fn write_examples(out: &mut String, name: &str, cmd: &Command) {
    match name {
        "find" => {
            let _ = writeln!(out, "# Find a single symbol");
            let _ = writeln!(out, "tyf find calculate_sum");
            let _ = writeln!(out);
            let _ = writeln!(out, "# Find multiple symbols at once");
            let _ = writeln!(out, "tyf find calculate_sum multiply divide");
            let _ = writeln!(out);
            let _ = writeln!(out, "# Find a symbol in a specific file");
            let _ = writeln!(out, "tyf find my_function --file src/module.py");
            let _ = writeln!(out);
            let _ = writeln!(out, "# Fuzzy/prefix match");
            let _ = writeln!(out, "tyf find handle_ --fuzzy");
        }
        "inspect" => {
            let _ = writeln!(out, "# Inspect a single symbol");
            let _ = writeln!(out, "tyf inspect MyClass");
            let _ = writeln!(out);
            let _ = writeln!(out, "# Inspect multiple symbols at once");
            let _ = writeln!(out, "tyf inspect MyClass my_function");
            let _ = writeln!(out);
            let _ = writeln!(out, "# Inspect a symbol in a specific file");
            let _ = writeln!(out, "tyf inspect MyClass --file src/module.py");
        }
        "interactive" => {
            let _ = writeln!(out, "# Start interactive REPL");
            let _ = writeln!(out, "tyf interactive");
            let _ = writeln!(out);
            let _ = writeln!(out, "# Start with a file context");
            let _ = writeln!(out, "tyf interactive main.py");
        }
        "refs" => {
            let _ = writeln!(out, "# Find all references to a symbol");
            let _ = writeln!(out, "tyf refs main.py --line 10 --column 5");
        }
        "list" => {
            let _ = writeln!(out, "# List all symbols in a file");
            let _ = writeln!(out, "tyf list main.py");
        }
        "daemon" => {
            let _ = writeln!(out, "# Start the background daemon");
            let _ = writeln!(out, "tyf daemon start");
            let _ = writeln!(out);
            let _ = writeln!(out, "# Check daemon status");
            let _ = writeln!(out, "tyf daemon status");
            let _ = writeln!(out);
            let _ = writeln!(out, "# Stop the daemon");
            let _ = writeln!(out, "tyf daemon stop");
        }
        _ => {
            // Generic example for unknown commands
            let has_positional = cmd.get_positionals().next().is_some();
            if has_positional {
                let _ = writeln!(out, "tyf {name} <args>");
            } else {
                let _ = writeln!(out, "tyf {name}");
            }
        }
    }
}
