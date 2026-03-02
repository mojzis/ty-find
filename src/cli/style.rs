use crate::cli::args::ColorMode;
use owo_colors::OwoColorize;

/// Resolved coloring decision — either on or off.
///
/// Created once at CLI startup from the `--color` flag, `NO_COLOR` env var,
/// and TTY detection, then threaded through to all formatting code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseColor {
    Yes,
    No,
}

impl UseColor {
    /// Resolve the effective colour setting from flag + env + TTY.
    ///
    /// Priority order:
    /// 1. `NO_COLOR` env var — if set (any value), always no colour
    /// 2. `--color=always` / `--color=never` — explicit override
    /// 3. `--color=auto` (default) — colour if stdout is a TTY
    pub fn resolve(mode: &ColorMode) -> Self {
        // NO_COLOR overrides everything (https://no-color.org)
        if std::env::var_os("NO_COLOR").is_some() {
            return Self::No;
        }

        match mode {
            ColorMode::Always => Self::Yes,
            ColorMode::Never => Self::No,
            ColorMode::Auto => {
                if supports_color::on(supports_color::Stream::Stdout).is_some() {
                    Self::Yes
                } else {
                    Self::No
                }
            }
        }
    }

    fn enabled(self) -> bool {
        self == Self::Yes
    }
}

/// A lightweight styler that applies ANSI colours when enabled.
///
/// All methods return owned `String`s — they're only used during output
/// formatting, never in hot loops. `Styler` is `Copy` (1 byte), so all
/// methods take `self` by value.
#[derive(Clone, Copy)]
pub struct Styler {
    color: UseColor,
}

impl Styler {
    pub fn new(color: UseColor) -> Self {
        Self { color }
    }

    /// A styler that never emits ANSI codes. Useful for tests and non-human formats.
    pub fn no_color() -> Self {
        Self { color: UseColor::No }
    }

    /// Section headings: `## Def`, `## Type`, `## Refs`, etc.
    /// Bold green — distinct from cyan file paths.
    pub fn heading(self, text: &str) -> String {
        if self.color.enabled() {
            format!("{}", text.bold().green())
        } else {
            text.to_string()
        }
    }

    /// Top-level symbol names: `# MyClass`, `# calculate_sum`.
    /// Bold magenta underlined — prominent on both light and dark backgrounds.
    pub fn symbol(self, text: &str) -> String {
        if self.color.enabled() {
            format!("{}", text.bold().magenta().underline())
        } else {
            text.to_string()
        }
    }

    /// Line/column numbers: `:15:1`.
    /// Dim/grey.
    pub fn line_col(self, text: &str) -> String {
        if self.color.enabled() {
            format!("{}", text.dimmed())
        } else {
            text.to_string()
        }
    }

    /// Kind labels in fuzzy find: `[class]`, `[function]`.
    /// Dim.
    pub fn dim(self, text: &str) -> String {
        if self.color.enabled() {
            format!("{}", text.dimmed())
        } else {
            text.to_string()
        }
    }

    /// Error messages.
    /// Red.
    pub fn error(self, text: &str) -> String {
        if self.color.enabled() {
            format!("{}", text.red())
        } else {
            text.to_string()
        }
    }

    /// Format a file path with colored line:col suffix.
    ///
    /// E.g. `src/models.py` in cyan + `:15:1` in dim.
    pub fn file_location(self, path: &str, line: u32, col: u32) -> String {
        if self.color.enabled() {
            let loc = format!(":{line}:{col}");
            format!("{}{}", path.cyan(), loc.dimmed())
        } else {
            format!("{path}:{line}:{col}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_never() {
        assert_eq!(UseColor::resolve(&ColorMode::Never), UseColor::No);
    }

    #[test]
    fn test_resolve_always() {
        // Only valid when NO_COLOR is not set; tests run without it by default
        assert_eq!(UseColor::resolve(&ColorMode::Always), UseColor::Yes);
    }

    #[test]
    fn test_styler_no_color_returns_plain_text() {
        let s = Styler::no_color();
        assert_eq!(s.heading("# Def"), "# Def");
        assert_eq!(s.symbol("MyClass"), "MyClass");
        assert_eq!(s.file_location("src/foo.py", 15, 1), "src/foo.py:15:1");
        assert_eq!(s.error("boom"), "boom");
        assert_eq!(s.dim("[class]"), "[class]");
    }

    #[test]
    fn test_styler_with_color_adds_ansi() {
        let s = Styler::new(UseColor::Yes);
        let heading = s.heading("# Def");
        assert!(heading.contains('\x1b'), "heading should contain ANSI: {heading:?}");

        let sym = s.symbol("MyClass");
        assert!(sym.contains('\x1b'), "symbol should contain ANSI: {sym:?}");

        let loc = s.file_location("src/foo.py", 15, 1);
        assert!(loc.contains('\x1b'), "file_location should contain ANSI: {loc:?}");

        let err = s.error("boom");
        assert!(err.contains('\x1b'), "error should contain ANSI: {err:?}");
    }
}
