//! Source-annotation recovery for symbols whose type `ty` reports as `Unknown`.
//!
//! `ty` widens a type to `Unknown` when it cannot resolve the annotation —
//! most often because third-party stubs are not installed in the analyzed
//! environment (e.g. a method annotated `-> pa.Table` where `pyarrow`'s types
//! are unresolvable). `Unknown` is technically honest but useless to the
//! caller, who only needs to know what the developer actually wrote.
//!
//! The developer's source annotation is the ground truth. When a rendered type
//! contains the `Unknown` token, we recover the literal annotation text from
//! source and show that instead. If the construct genuinely has no annotation,
//! we show [`UNANNOTATED`] rather than `Unknown` or an empty type.
//!
//! This works without the analyzed project's dependencies or stubs installed —
//! it reads source only. It does not parse imports, inspect lockfiles, or
//! require a full environment.
//!
//! No Python AST crate is available in this project (and adding one would be a
//! heavy, semver-unstable dependency for a one-symbol lookup), so extraction
//! uses a small bracket- and quote-aware scanner over the symbol's definition
//! line. That is enough to handle multi-line signatures, nested brackets,
//! string annotations, and trailing comments correctly — the cases where naive
//! line slicing breaks.

/// Marker shown for a symbol that genuinely has no source annotation.
pub const UNANNOTATED: &str = "(unannotated)";

/// The token `ty` emits for a type it could not resolve.
const UNKNOWN: &str = "Unknown";

/// Outcome of looking up an annotation in source.
#[derive(Debug, PartialEq, Eq)]
enum SourceAnnotation {
    /// Annotation text recovered from source (already normalized).
    Found(String),
    /// The construct exists but has no annotation — caller shows [`UNANNOTATED`].
    Missing,
    /// Could not locate or parse the construct — caller keeps the original text.
    Unparseable,
}

/// Returns true if `s` contains the bare `Unknown` token, ignoring occurrences
/// that are part of a longer identifier (e.g. `UnknownError`, `_Unknown`).
#[must_use]
pub fn contains_unknown(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut from = 0;
    while let Some(rel) = s[from..].find(UNKNOWN) {
        let start = from + rel;
        let end = start + UNKNOWN.len();
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Substitute `Unknown` in a rendered type/signature with the literal source
/// annotation.
///
/// `rendered` is the type text as `ty` produced it (e.g.
/// `get_table(self) -> Unknown`, `def f() -> Unknown`, or a bare `Unknown` /
/// `attr: Unknown`). `source` is the full text of the file and `def_line_0` is
/// the 0-indexed line where the symbol's definition begins.
///
/// When the return type (or, for a bare type, the whole type) contains the
/// `Unknown` token, it is replaced with the source annotation, or with
/// [`UNANNOTATED`] when source has no annotation. Anything we cannot
/// confidently recover is left untouched. Inputs without `Unknown` are returned
/// unchanged.
#[must_use]
pub fn substitute_unknown(rendered: &str, source: &str, def_line_0: u32) -> String {
    if !contains_unknown(rendered) {
        return rendered.to_string();
    }

    // Function form: replace the return type when it is the part that is Unknown.
    if let Some(arrow_end) = top_level_arrow(rendered) {
        let ret = rendered[arrow_end..].trim();
        if !contains_unknown(ret) {
            // The Unknown is in the parameter list, not the return type — the
            // return-type substitution this performs does not apply. Leave as-is.
            return rendered.to_string();
        }
        let head = rendered[..arrow_end].trim_end();
        return match return_annotation(source, def_line_0) {
            SourceAnnotation::Found(ann) => format!("{head} {ann}"),
            SourceAnnotation::Missing => format!("{head} {UNANNOTATED}"),
            SourceAnnotation::Unparseable => rendered.to_string(),
        };
    }

    // Bare-type form: `attr: Unknown` (members prefixes the name) or just
    // `Unknown` (hover). Recover the variable/attribute annotation from source.
    let (prefix, ty) = split_name_prefix(rendered);
    if !contains_unknown(ty) {
        return rendered.to_string();
    }
    match variable_annotation(source, def_line_0) {
        SourceAnnotation::Found(ann) => format!("{prefix}{ann}"),
        SourceAnnotation::Missing => format!("{prefix}{UNANNOTATED}"),
        SourceAnnotation::Unparseable => rendered.to_string(),
    }
}

/// Split a rendered bare type into an optional `name: ` prefix and the type.
///
/// `attr: Unknown` → (`"attr: "`, `"Unknown"`). A plain type with no leading
/// identifier-and-colon (e.g. `dict[str, Unknown]`, `str | None`) yields an
/// empty prefix and the whole string as the type.
fn split_name_prefix(rendered: &str) -> (&str, &str) {
    if let Some(pos) = rendered.find(": ") {
        let name = &rendered[..pos];
        if !name.is_empty() && name.bytes().all(is_ident_byte) {
            return (&rendered[..pos + 2], &rendered[pos + 2..]);
        }
    }
    ("", rendered)
}

/// Find the return arrow `->` at bracket depth 0 (outside strings) and return
/// the byte index of the character immediately after it.
fn top_level_arrow(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut in_str: Option<char> = None;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if let Some(q) = in_str {
            if c == '\\' {
                chars.next();
            } else if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => in_str = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '-' if depth == 0 => {
                if matches!(chars.peek(), Some(&(_, '>'))) {
                    chars.next();
                    return Some(i + 2);
                }
            }
            _ => {}
        }
    }
    None
}

/// Recover the literal return annotation from a function definition.
///
/// Scans from `def_line_0` (skipping decorators, which have no top-level `->`
/// or `:`), tracking bracket depth and string state, until the `:` that ends
/// the `def` header. The annotation is the text between `->` and that colon.
fn return_annotation(source: &str, def_line_0: u32) -> SourceAnnotation {
    let Some(start) = line_start_offset(source, def_line_0) else {
        return SourceAnnotation::Unparseable;
    };
    let slice = &source[start..];

    let mut depth: i32 = 0;
    let mut in_str: Option<char> = None;
    let mut seen_params = false;
    let mut arrow_end: Option<usize> = None;
    let mut chars = slice.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if let Some(q) = in_str {
            if c == '\\' {
                chars.next();
            } else if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '#' => skip_to_eol(&mut chars),
            '\'' | '"' => in_str = Some(c),
            '(' => {
                depth += 1;
                seen_params = true;
            }
            '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '-' if depth == 0 => {
                if matches!(chars.peek(), Some(&(_, '>'))) {
                    chars.next();
                    arrow_end = Some(i + 2);
                }
            }
            ':' if depth == 0 && seen_params => {
                return match arrow_end {
                    Some(a) => finish(&slice[a..i]),
                    None => SourceAnnotation::Missing,
                };
            }
            _ => {}
        }
    }
    SourceAnnotation::Unparseable
}

/// Recover the literal annotation from an annotated assignment / variable.
///
/// Reads from `def_line_0` to the first top-level `:` (the annotation colon),
/// then captures up to a top-level `=` or the end of the logical line.
fn variable_annotation(source: &str, def_line_0: u32) -> SourceAnnotation {
    let Some(start) = line_start_offset(source, def_line_0) else {
        return SourceAnnotation::Unparseable;
    };
    let slice = &source[start..];

    let mut depth: i32 = 0;
    let mut in_str: Option<char> = None;
    let mut ann_start: Option<usize> = None;
    let mut chars = slice.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if let Some(q) = in_str {
            if c == '\\' {
                chars.next();
            } else if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '#' => skip_to_eol(&mut chars),
            '\'' | '"' => in_str = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 && ann_start.is_none() => {
                // Ignore the walrus operator `:=`.
                if matches!(chars.peek(), Some(&(_, '='))) {
                    chars.next();
                    continue;
                }
                ann_start = Some(i + 1);
            }
            '=' if depth == 0 => {
                if let Some(a) = ann_start {
                    return finish(&slice[a..i]);
                }
            }
            '\n' if depth == 0 => {
                return match ann_start {
                    Some(a) => finish(&slice[a..i]),
                    None => SourceAnnotation::Missing,
                };
            }
            _ => {}
        }
    }
    match ann_start {
        Some(a) => finish(&slice[a..]),
        None => SourceAnnotation::Missing,
    }
}

/// Advance the iterator to (but not past) the next newline.
fn skip_to_eol(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) {
    while let Some(&(_, c)) = chars.peek() {
        if c == '\n' {
            break;
        }
        chars.next();
    }
}

/// Byte offset of the start of 0-indexed `line`, or `None` if out of range.
fn line_start_offset(source: &str, line: u32) -> Option<usize> {
    if line == 0 {
        return Some(0);
    }
    let mut seen = 0u32;
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            seen += 1;
            if seen == line {
                return Some(i + 1);
            }
        }
    }
    None
}

/// Normalize a raw annotation span into clean type text, classifying empty
/// spans as [`SourceAnnotation::Missing`].
fn finish(raw: &str) -> SourceAnnotation {
    let norm = normalize(raw);
    if norm.is_empty() {
        SourceAnnotation::Missing
    } else {
        SourceAnnotation::Found(norm)
    }
}

/// Collapse whitespace/newlines/comments out of an annotation span and strip a
/// single enclosing string-literal quote pair (stringized annotation).
fn normalize(raw: &str) -> String {
    // Strip any line comments first (the span can cross lines that carry `#`).
    let mut without_comments = String::with_capacity(raw.len());
    let mut in_str: Option<char> = None;
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(q) = in_str {
            without_comments.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    without_comments.push(n);
                }
            } else if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '#' => {
                while let Some(&n) = chars.peek() {
                    if n == '\n' {
                        break;
                    }
                    chars.next();
                }
            }
            '\'' | '"' => {
                in_str = Some(c);
                without_comments.push(c);
            }
            _ => without_comments.push(c),
        }
    }

    // Collapse all whitespace runs to single spaces.
    let collapsed = without_comments.split_whitespace().collect::<Vec<_>>().join(" ");
    // Tidy spacing introduced inside brackets by the collapse.
    let collapsed =
        collapsed.replace("( ", "(").replace(" )", ")").replace("[ ", "[").replace(" ]", "]");
    let trimmed = collapsed.trim();
    strip_enclosing_quotes(trimmed)
}

/// Strip one enclosing quote pair if the whole string is a single string
/// literal (e.g. `"pa.Table"` → `pa.Table`).
fn strip_enclosing_quotes(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' || first == b'\'') && first == last {
            let inner = &s[1..s.len() - 1];
            if !inner.contains(first as char) {
                return inner.trim().to_string();
            }
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_unknown_matches_bare_token() {
        assert!(contains_unknown("Unknown"));
        assert!(contains_unknown("get(self) -> Unknown"));
        assert!(contains_unknown("dict[str, Unknown]"));
        assert!(contains_unknown("list[Unknown]"));
    }

    #[test]
    fn contains_unknown_ignores_longer_identifiers() {
        assert!(!contains_unknown("UnknownError"));
        assert!(!contains_unknown("_Unknown"));
        assert!(!contains_unknown("MyUnknownType"));
        assert!(!contains_unknown("str | None"));
    }

    #[test]
    fn return_annotation_simple() {
        let src = "def f(self) -> pa.Table:\n    ...\n";
        assert_eq!(return_annotation(src, 0), SourceAnnotation::Found("pa.Table".to_string()));
    }

    #[test]
    fn return_annotation_generic_with_brackets() {
        let src = "def f(self, x: int) -> dict[str, pa.Table]:\n    ...\n";
        assert_eq!(
            return_annotation(src, 0),
            SourceAnnotation::Found("dict[str, pa.Table]".to_string())
        );
    }

    #[test]
    fn return_annotation_missing() {
        let src = "def f(self):\n    ...\n";
        assert_eq!(return_annotation(src, 0), SourceAnnotation::Missing);
    }

    #[test]
    fn return_annotation_multiline_signature() {
        let src = "def assist(\n    self,\n    task: str,\n) -> pa.Table:\n    ...\n";
        assert_eq!(return_annotation(src, 0), SourceAnnotation::Found("pa.Table".to_string()));
    }

    #[test]
    fn return_annotation_stringized() {
        let src = "def f(self) -> \"pa.Table\":\n    ...\n";
        assert_eq!(return_annotation(src, 0), SourceAnnotation::Found("pa.Table".to_string()));
    }

    #[test]
    fn return_annotation_trailing_comment() {
        let src = "def f(self) -> pa.Table:  # returns a table\n    ...\n";
        assert_eq!(return_annotation(src, 0), SourceAnnotation::Found("pa.Table".to_string()));
    }

    #[test]
    fn return_annotation_skips_decorators() {
        let src = "@property\ndef f(self) -> pa.Table:\n    ...\n";
        assert_eq!(return_annotation(src, 0), SourceAnnotation::Found("pa.Table".to_string()));
    }

    #[test]
    fn return_annotation_default_with_colon_in_params() {
        let src = "def f(self, m: dict[str, int] = {}) -> pa.Table:\n    ...\n";
        assert_eq!(return_annotation(src, 0), SourceAnnotation::Found("pa.Table".to_string()));
    }

    #[test]
    fn variable_annotation_simple() {
        let src = "table: pa.Table = load()\n";
        assert_eq!(variable_annotation(src, 0), SourceAnnotation::Found("pa.Table".to_string()));
    }

    #[test]
    fn variable_annotation_no_default() {
        let src = "table: pa.Table\n";
        assert_eq!(variable_annotation(src, 0), SourceAnnotation::Found("pa.Table".to_string()));
    }

    #[test]
    fn variable_annotation_missing() {
        let src = "table = load()\n";
        assert_eq!(variable_annotation(src, 0), SourceAnnotation::Missing);
    }

    #[test]
    fn substitute_return_type_with_source_annotation() {
        let src = "def get_table(self) -> pa.Table:\n    ...\n";
        let out = substitute_unknown("get_table(self) -> Unknown", src, 0);
        assert_eq!(out, "get_table(self) -> pa.Table");
    }

    #[test]
    fn substitute_return_type_with_def_prefix() {
        let src = "def get_table(self) -> pa.Table:\n    ...\n";
        let out = substitute_unknown("def get_table(self) -> Unknown", src, 0);
        assert_eq!(out, "def get_table(self) -> pa.Table");
    }

    #[test]
    fn substitute_generic_return_type() {
        let src = "def cols(self) -> list[pa.Field]:\n    ...\n";
        let out = substitute_unknown("cols(self) -> list[Unknown]", src, 0);
        assert_eq!(out, "cols(self) -> list[pa.Field]");
    }

    #[test]
    fn substitute_unannotated_return() {
        let src = "def mystery(self):\n    ...\n";
        let out = substitute_unknown("mystery(self) -> Unknown", src, 0);
        assert_eq!(out, "mystery(self) -> (unannotated)");
    }

    #[test]
    fn substitute_bare_variable_type() {
        let src = "table: pa.Table = load()\n";
        let out = substitute_unknown("table: Unknown", src, 0);
        assert_eq!(out, "table: pa.Table");
    }

    #[test]
    fn substitute_leaves_resolvable_types_untouched() {
        let src = "def speak(self) -> str:\n    ...\n";
        let out = substitute_unknown("speak(self) -> str", src, 0);
        assert_eq!(out, "speak(self) -> str");
    }

    #[test]
    fn substitute_leaves_param_unknown_untouched() {
        // Unknown is in a parameter, not the return type — out of scope.
        let src = "def f(self, x: pa.Table) -> str:\n    ...\n";
        let out = substitute_unknown("f(self, x: Unknown) -> str", src, 0);
        assert_eq!(out, "f(self, x: Unknown) -> str");
    }

    #[test]
    fn substitute_multiline_signature() {
        let src = "def assist(\n    self,\n    task: str,\n) -> pa.Table:\n    ...\n";
        let out = substitute_unknown("assist(self, task: str) -> Unknown", src, 0);
        assert_eq!(out, "assist(self, task: str) -> pa.Table");
    }

    #[test]
    fn substitute_stringized_annotation() {
        let src = "def f(self) -> \"pa.Table\":\n    ...\n";
        let out = substitute_unknown("f(self) -> Unknown", src, 0);
        assert_eq!(out, "f(self) -> pa.Table");
    }

    #[test]
    fn substitute_trailing_comment_no_leak() {
        let src = "def f(self) -> pa.Table:  # not part of the type\n    ...\n";
        let out = substitute_unknown("f(self) -> Unknown", src, 0);
        assert_eq!(out, "f(self) -> pa.Table");
    }
}
