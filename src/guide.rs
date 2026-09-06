//! Agent-facing instructions, printed by `tyf guide`.
//!
//! The prose lives in `docs/guide/*.md` and is pulled in with [`include_str!`]
//! at compile time, so the text always matches the build. There is no guide
//! text in this file, and there must never be: a second copy is a copy that
//! drifts.
//!
//! The CLAUDE.md snippet in the setup page is deliberately shorter than the
//! shared one in README.md: three commands the reader can paste under the
//! line cap, with the full list one `tyf guide use` away. It is the one
//! place the two are allowed to differ.
//!
//! The guide is the one command an agent runs *before* the project is set
//! up, often as `uvx ty-find guide` in a directory with no ty at all. It is
//! dispatched in `main` ahead of workspace detection, the daemon socket and
//! the LSP; nothing in this module may reach for any of them. Page selection
//! is a handful of `stat` calls and nothing else.

use std::path::{Path, PathBuf};

/// Instructions for a project that does not have ty-find installed yet.
const SETUP: &str = include_str!("../docs/guide/setup.md");
/// The command reference for a project that has it.
const USE: &str = include_str!("../docs/guide/use.md");

/// Which page to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Page {
    /// Install ty-find, wire it into CLAUDE.md, run a first query
    Setup,
    /// Command reference: which command answers which question
    Use,
}

impl Page {
    /// The page text, byte-identical to the docs file it is included from.
    pub fn text(self) -> &'static str {
        match self {
            Self::Setup => SETUP,
            Self::Use => USE,
        }
    }

    /// The page's name as written on the command line, taken from the
    /// `ValueEnum` derive so the two can never disagree.
    #[cfg(test)]
    pub fn name(self) -> String {
        use clap::ValueEnum as _;
        self.to_possible_value().expect("no skipped variants").get_name().to_owned()
    }

    /// Every page, taken from the `ValueEnum` derive so a page added later is
    /// covered by every content check below without being listed by hand.
    #[cfg(test)]
    pub fn all() -> impl Iterator<Item = Self> {
        <Self as clap::ValueEnum>::value_variants().iter().copied()
    }
}

/// Where a project install of ty-find puts the binary, relative to the
/// project root.
#[cfg(unix)]
const VENV_BINARY: &str = ".venv/bin/tyf";
#[cfg(not(unix))]
const VENV_BINARY: &str = ".venv/Scripts/tyf.exe";

/// The page to print when the user named none.
///
/// The nearest `pyproject.toml` at or above `cwd` marks the project. If
/// `.venv/bin/tyf` sits next to it, ty-find is installed there and the
/// reader needs the command reference; otherwise, or with no project at all,
/// they need setup. Only the filesystem is consulted, and only with `stat`.
pub fn detect(cwd: &Path) -> Page {
    match project_root(cwd) {
        Some(root) if root.join(VENV_BINARY).is_file() => Page::Use,
        _ => Page::Setup,
    }
}

/// The nearest ancestor of `dir` (inclusive) holding a `pyproject.toml`.
fn project_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().find(|d| d.join("pyproject.toml").is_file()).map(Path::to_path_buf)
}

/// The text `tyf guide [PAGE]` prints: the chosen page, verbatim.
///
/// `page` is `None` for the bare `tyf guide`, in which case [`detect`] picks.
pub fn render(page: Option<Page>, cwd: &Path) -> &'static str {
    page.unwrap_or_else(|| detect(cwd)).text()
}

/// Every `tyf` invocation the pages show, as argv vectors ready for clap.
///
/// Test-only: it exists so `cli::args` can feed each one through the real
/// `Cli` parser. A guide that shows a command the CLI would reject is worse
/// than no guide. Understands inline backtick spans and lines of fenced
/// `bash` blocks. A `<placeholder>` token is a hole for the reader; it is
/// replaced with [`PLACEHOLDER`] so the argv still carries the argument the
/// hole stands for. A leading `uv run` is stripped.
#[cfg(test)]
pub fn embedded_invocations(page: Page) -> Vec<Vec<&'static str>> {
    command_lines(page.text()).into_iter().filter_map(tyf_argv).collect()
}

/// Every non-blank line of a `bash` fence, plus inline code spans everywhere
/// else, in source order. Spans are read inside other fences too: the
/// CLAUDE.md snippet in the setup page is a bare fence, and its commands are
/// exactly the ones an agent will paste.
#[cfg(test)]
fn command_lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    for line in text.lines() {
        if let Some(info) = line.strip_prefix("```") {
            fence = if fence.is_some() { None } else { Some(info.trim()) };
            continue;
        }
        if fence == Some("bash") {
            if !line.trim().is_empty() {
                out.push(line);
            }
        } else {
            out.extend(inline_code_spans(line));
        }
    }
    out
}

/// The contents of each `code` span on one line, in order.
#[cfg(test)]
fn inline_code_spans(line: &str) -> Vec<&str> {
    let mut spans = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else { break };
        spans.push(&after[..close]);
        rest = &after[close + 1..];
    }
    spans
}

/// What a `<placeholder>` token becomes in an extracted argv.
#[cfg(test)]
pub const PLACEHOLDER: &str = "PLACEHOLDER";

/// A command line as a `tyf` argv, or `None` when it is prose or another
/// tool. Trailing `# comments` are dropped. A bare `tyf` or a `tyf ...`
/// names the tool rather than invoking it, and is prose.
#[cfg(test)]
fn tyf_argv(line: &str) -> Option<Vec<&str>> {
    let line = line.split('#').next().unwrap_or("").trim();
    let line = line.strip_prefix("uv run ").unwrap_or(line);
    if !line.starts_with("tyf ") || line.contains("...") {
        return None;
    }
    Some(
        line.split_whitespace()
            .map(|token| if token.contains('<') { PLACEHOLDER } else { token })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No page may exceed this many lines: a guide that grows past a
    /// screenful stops being read. Cut the page rather than raising the cap.
    const LINE_CAP: usize = 60;

    fn write(dir: &Path, rel: &str, contents: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().expect("has parent")).expect("mkdir");
        std::fs::write(path, contents).expect("write");
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    // --- Content rules ---

    #[test]
    fn every_page_fits_the_line_cap() {
        for page in Page::all() {
            let lines = page.text().trim_end().lines().count();
            assert!(
                lines <= LINE_CAP,
                "guide `{}` is {lines} lines, cap is {LINE_CAP}",
                page.name()
            );
            assert!(lines > 10, "guide `{}` is {lines} lines, which is not a guide", page.name());
        }
    }

    #[test]
    fn every_page_is_plain_ascii() {
        for page in Page::all() {
            let offender = page.text().chars().find(|c| !c.is_ascii());
            assert!(
                offender.is_none(),
                "guide `{}` contains non-ASCII {offender:?}; guides are piped and captured",
                page.name()
            );
        }
    }

    #[test]
    fn every_page_starts_with_its_title() {
        for page in Page::all() {
            let expected = format!("# ty-find: {}\n", page.name());
            assert!(
                page.text().starts_with(&expected),
                "guide `{}` must start with {expected:?}",
                page.name()
            );
        }
    }

    #[test]
    fn every_page_ends_with_a_single_next_line() {
        for page in Page::all() {
            let trimmed = page.text().trim_end();
            let last = trimmed.lines().next_back().expect("guide should not be empty");
            assert!(last.starts_with("next: "), "guide `{}` ends with {last:?}", page.name());
            let count = trimmed.lines().filter(|l| l.starts_with("next: ")).count();
            assert_eq!(count, 1, "guide `{}` should have exactly one next line", page.name());
            assert!(page.text().ends_with('\n'), "guide `{}` must end with a newline", page.name());
        }
    }

    #[test]
    fn setup_installs_then_queries_then_points_at_use() {
        let text = Page::Setup.text();
        let add = text.find("uv add --dev").expect("setup should install with uv add");
        let run = text.find("uv run tyf find").expect("setup should run a first query");
        assert!(add < run, "setup must install before it queries");
        assert!(text.contains("CLAUDE.md"), "setup must tell the agent to edit CLAUDE.md");
        assert!(text.contains("background daemon"), "setup states the daemon fact once");
        assert_eq!(text.matches("daemon").count(), 1, "setup mentions the daemon exactly once");
        assert!(text.trim_end().ends_with("next: run `uv run tyf guide use`"));
    }

    #[test]
    fn use_covers_every_public_command_and_isolates_daemon_advice() {
        let text = Page::Use.text();
        for cmd in ["tyf find", "tyf refs", "tyf show", "tyf members", "tyf calls", "tyf list"] {
            assert!(text.contains(cmd), "use guide should cover `{cmd}`");
        }
        for flag in ["--format json", "--fuzzy"] {
            assert!(text.contains(flag), "use guide should mention `{flag}`");
        }
        let heading = text.find("## If something is off").expect("use has the recovery heading");
        for cmd in ["tyf daemon status", "tyf daemon stop"] {
            let at = text.find(cmd).unwrap_or_else(|| panic!("use guide should mention `{cmd}`"));
            assert!(at > heading, "`{cmd}` belongs under the recovery heading, not above it");
        }
    }

    // --- Extraction ---

    #[test]
    fn extraction_finds_every_command_the_pages_show() {
        let setup = embedded_invocations(Page::Setup);
        assert!(
            setup.contains(&vec!["tyf", "find", "UserService"]),
            "setup shows `uv run tyf find UserService` in a bash fence; got {setup:?}"
        );
        assert!(
            setup.contains(&vec!["tyf", "guide", "use"]),
            "the next line's `uv run` prefix should be stripped; got {setup:?}"
        );
        assert!(
            setup.contains(&vec!["tyf", "refs", PLACEHOLDER]),
            "the CLAUDE.md snippet sits in a bare fence and must still be checked; got {setup:?}"
        );
        let use_page = embedded_invocations(Page::Use);
        assert!(
            use_page.contains(&vec!["tyf", "find", PLACEHOLDER]),
            "the `<name>` placeholder should become {PLACEHOLDER}; got {use_page:?}"
        );
        assert!(
            use_page.contains(&vec!["tyf", "daemon", "status"]),
            "bash fences in the use page should be extracted; got {use_page:?}"
        );
        for page in Page::all() {
            assert!(
                !embedded_invocations(page).is_empty(),
                "guide `{}` shows no commands",
                page.name()
            );
        }
    }

    #[test]
    fn extraction_ignores_prose_other_tools_and_bare_fence_lines() {
        let text = "Run `uv add --dev tyf` then `tyf find x`; `tyf` is `tyf ...`.\n```\ntyf refs bare\n- `tyf refs spanned`\n```\n```bash\nuv run tyf show y # note\n```\n";
        let found: Vec<Vec<&str>> = command_lines(text).into_iter().filter_map(tyf_argv).collect();
        assert_eq!(
            found,
            vec![
                vec!["tyf", "find", "x"],
                vec!["tyf", "refs", "spanned"],
                vec!["tyf", "show", "y"]
            ]
        );
    }

    // --- Selection ---

    #[test]
    fn no_project_selects_setup() {
        let dir = tempdir();
        assert_eq!(detect(dir.path()), Page::Setup);
    }

    #[test]
    fn project_without_the_venv_binary_selects_setup() {
        let dir = tempdir();
        write(dir.path(), "pyproject.toml", "");
        assert_eq!(detect(dir.path()), Page::Setup);
    }

    #[test]
    fn project_with_the_venv_binary_selects_use_from_any_subdirectory() {
        let dir = tempdir();
        write(dir.path(), "pyproject.toml", "");
        write(dir.path(), VENV_BINARY, "");
        assert_eq!(detect(dir.path()), Page::Use);
        let sub = dir.path().join("src").join("pkg");
        std::fs::create_dir_all(&sub).expect("mkdir");
        assert_eq!(detect(&sub), Page::Use);
    }

    /// The nearest `pyproject.toml` decides: a sub-project without its own
    /// venv is not configured, even inside a parent that is.
    #[test]
    fn nearest_pyproject_wins() {
        let dir = tempdir();
        write(dir.path(), "pyproject.toml", "");
        write(dir.path(), VENV_BINARY, "");
        write(dir.path(), "sub/pyproject.toml", "");
        assert_eq!(detect(&dir.path().join("sub")), Page::Setup);
    }

    /// A venv directory alone is not an install: the binary must be there.
    #[test]
    fn venv_without_the_binary_selects_setup() {
        let dir = tempdir();
        write(dir.path(), "pyproject.toml", "");
        std::fs::create_dir_all(dir.path().join(".venv/bin")).expect("mkdir");
        assert_eq!(detect(dir.path()), Page::Setup);
    }

    #[test]
    fn explicit_page_beats_detection() {
        let dir = tempdir();
        assert_eq!(render(Some(Page::Use), dir.path()), USE);
        assert_eq!(render(None, dir.path()), SETUP);
    }
}
