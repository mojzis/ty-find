//! Source-annotation recovery for unresolved types.
//!
//! When `ty` cannot resolve an annotation — most often because third-party
//! stubs aren't installed (e.g. a method annotated `-> pa.Table` where
//! `pyarrow` types aren't resolvable) — it widens the displayed type to
//! `Unknown`. That's honest but useless to the caller, who only needs to know
//! what the developer actually wrote.
//!
//! The developer's source annotation is the ground truth. When a displayed type
//! is `Unknown`, we recover the literal annotation text from source and show
//! that instead. If there is genuinely no annotation in source, we mark it
//! `(unannotated)` rather than leaking `Unknown`.
//!
//! This reads source only — no import parsing, no dependency/lockfile
//! inspection, no stubs required. It therefore works on partial codebases.
//!
//! Extraction is bracket-, string-, and comment-aware (not naive line slicing)
//! so it handles multi-line signatures, stringized annotations, and trailing
//! comments. The same internals cleanly distinguish two future reports —
//! "Unknown + has annotation" (missing-stubs surface) vs "Unknown + no
//! annotation" (untyped surface) — via [`SourceAnnotation`].

/// Marker shown when source has no annotation at all.
const UNANNOTATED: &str = "(unannotated)";

/// The literal type text `ty` emits when it cannot resolve an annotation.
const UNKNOWN: &str = "Unknown";

/// The literal source annotation for a symbol, or the absence of one.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceAnnotation {
    /// The annotation text exactly as written in source (normalized).
    Annotated(String),
    /// No annotation is present in source.
    Unannotated,
}

/// Substitute `Unknown` in a rendered type/signature with the source annotation.
///
/// `rendered` is the type text as ty rendered it (e.g. `def f(self) -> Unknown`,
/// `name: Unknown`, or a bare `Unknown`). `source` is the full text of the file
/// containing the definition and `def_line_0` is the 0-indexed line where the
/// `def`/assignment begins.
///
/// Only a wholesale `Unknown` type is substituted — partially-resolved types
/// like `dict[str, Unknown]` are left untouched, since ty resolved the outer
/// shape and the source annotation wouldn't improve on it. When the type is
/// `Unknown` but source has no annotation, [`UNANNOTATED`] is shown.
pub fn substitute_unknown(rendered: &str, source: &str, def_line_0: u32) -> String {
    // Cheap string check during existing result iteration — no extra passes.
    if !rendered.contains(UNKNOWN) {
        return rendered.to_string();
    }

    // Function/method return type: `... -> Unknown`.
    if let Some(arrow) = rendered.rfind("->") {
        if rendered[arrow + 2..].trim() == UNKNOWN {
            let head = rendered[..arrow].trim_end();
            let repl = annotation_or_marker(&extract_return_annotation(source, def_line_0));
            return format!("{head} -> {repl}");
        }
        // Return type resolved fine; any remaining `Unknown` is in a parameter,
        // which we intentionally leave as ty rendered it.
        return rendered.to_string();
    }

    // Bare variable/attribute type: `Unknown`.
    if rendered.trim() == UNKNOWN {
        return annotation_or_marker(&extract_variable_annotation(source, def_line_0));
    }

    // Named variable/attribute: `name: Unknown`.
    if let Some(colon) = rendered.rfind(':') {
        if rendered[colon + 1..].trim() == UNKNOWN {
            let head = rendered[..colon].trim_end();
            let repl = annotation_or_marker(&extract_variable_annotation(source, def_line_0));
            return format!("{head}: {repl}");
        }
    }

    rendered.to_string()
}

fn annotation_or_marker(annotation: &SourceAnnotation) -> String {
    match annotation {
        SourceAnnotation::Annotated(text) => text.clone(),
        SourceAnnotation::Unannotated => UNANNOTATED.to_string(),
    }
}

/// Extract a function/method return annotation from source.
///
/// `def_line_0` may sit on a decorator; we scan forward to the `def`/`async def`
/// header line, then recover the text between `->` and the header-terminating
/// colon.
fn extract_return_annotation(source: &str, def_line_0: u32) -> SourceAnnotation {
    let lines: Vec<&str> = source.lines().collect();
    let start = def_line_0 as usize;
    if start >= lines.len() {
        return SourceAnnotation::Unannotated;
    }

    // Locate the `def`/`async def` line (def_line_0 may point at a decorator).
    let mut idx = start;
    loop {
        if idx >= lines.len() {
            return SourceAnnotation::Unannotated;
        }
        let trimmed = lines[idx].trim_start();
        if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
            break;
        }
        if trimmed.starts_with('@') || trimmed.is_empty() {
            idx += 1;
            continue;
        }
        // Not a function header — nothing to recover.
        break;
    }

    // Cap the window: signatures are never this long, and the scanner stops at
    // the header colon anyway. The cap just bounds work on huge files.
    let end = lines.len().min(idx + 80);
    scan_return_annotation(&lines[idx..end].join("\n"))
}

/// Extract a variable/attribute annotation (`name: <annotation>`) from source.
fn extract_variable_annotation(source: &str, def_line_0: u32) -> SourceAnnotation {
    let lines: Vec<&str> = source.lines().collect();
    let start = def_line_0 as usize;
    if start >= lines.len() {
        return SourceAnnotation::Unannotated;
    }
    let end = lines.len().min(start + 40);
    scan_variable_annotation(&lines[start..end].join("\n"))
}

/// Scan a `def` header for its return annotation.
///
/// Tracks bracket depth so parameter colons and the param list don't confuse
/// the `->`/terminating-colon detection, and skips string and comment contents.
fn scan_return_annotation(text: &str) -> SourceAnnotation {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut capturing = false;
    let mut buf = String::new();

    while i < n {
        let c = chars[i];
        if c == '"' || c == '\'' {
            let (literal, next) = read_string(&chars, i);
            if capturing {
                buf.push_str(&literal);
            }
            i = next;
            continue;
        }
        if c == '#' {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                if capturing {
                    buf.push(c);
                }
            }
            ')' | ']' | '}' => {
                depth -= 1;
                if capturing {
                    buf.push(c);
                }
            }
            ':' if depth == 0 => {
                // Header-terminating colon: `def f(...) -> R:`.
                return finalize(&buf, capturing);
            }
            '-' if depth == 0 && chars.get(i + 1) == Some(&'>') => {
                capturing = true;
                i += 2;
                continue;
            }
            _ => {
                if capturing {
                    buf.push(c);
                }
            }
        }
        i += 1;
    }
    finalize(&buf, capturing)
}

/// Scan an annotated assignment for its annotation (`name: <annotation> = ...`).
///
/// The annotation runs from the first depth-0 `:` to the value's `=`, the end of
/// the logical line, or EOF — whichever comes first.
fn scan_variable_annotation(text: &str) -> SourceAnnotation {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut capturing = false;
    let mut buf = String::new();

    while i < n {
        let c = chars[i];
        if c == '"' || c == '\'' {
            let (literal, next) = read_string(&chars, i);
            if capturing {
                buf.push_str(&literal);
            }
            i = next;
            continue;
        }
        if c == '#' {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                if capturing {
                    buf.push(c);
                }
            }
            ')' | ']' | '}' => {
                depth -= 1;
                if capturing {
                    buf.push(c);
                }
            }
            ':' if depth == 0 && !capturing => {
                capturing = true;
            }
            // Value assignment (`=`) or end of the logical line (`\n`) — the
            // annotation (if any) ends here.
            '=' | '\n' if depth == 0 => {
                return finalize(&buf, capturing);
            }
            _ => {
                if capturing {
                    buf.push(c);
                }
            }
        }
        i += 1;
    }
    finalize(&buf, capturing)
}

/// Turn a captured annotation buffer into a [`SourceAnnotation`].
fn finalize(buf: &str, capturing: bool) -> SourceAnnotation {
    if !capturing {
        return SourceAnnotation::Unannotated;
    }
    let normalized = normalize_annotation(buf);
    let stripped = strip_string_quotes(&normalized);
    if stripped.is_empty() {
        SourceAnnotation::Unannotated
    } else {
        SourceAnnotation::Annotated(stripped.to_string())
    }
}

/// Read a string literal starting at `start` (a quote char), returning the
/// literal text (including quotes) and the index just past it.
///
/// Handles single- and triple-quoted strings and backslash escapes. An
/// unterminated single-quoted string stops at the newline.
fn read_string(chars: &[char], start: usize) -> (String, usize) {
    let quote = chars[start];
    let n = chars.len();
    let triple = start + 2 < n && chars[start + 1] == quote && chars[start + 2] == quote;
    let mut out = String::new();

    if triple {
        out.push(quote);
        out.push(quote);
        out.push(quote);
        let mut i = start + 3;
        while i < n {
            if chars[i] == '\\' && i + 1 < n {
                out.push(chars[i]);
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if chars[i] == quote && i + 2 < n && chars[i + 1] == quote && chars[i + 2] == quote {
                out.push(quote);
                out.push(quote);
                out.push(quote);
                return (out, i + 3);
            }
            out.push(chars[i]);
            i += 1;
        }
        return (out, i);
    }

    out.push(quote);
    let mut i = start + 1;
    while i < n {
        if chars[i] == '\\' && i + 1 < n {
            out.push(chars[i]);
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if chars[i] == quote {
            out.push(quote);
            return (out, i + 1);
        }
        if chars[i] == '\n' {
            // Unterminated — bail at the line boundary.
            return (out, i);
        }
        out.push(chars[i]);
        i += 1;
    }
    (out, i)
}

/// Collapse whitespace and tidy spacing around brackets/commas so an annotation
/// that spanned lines or carried awkward whitespace reads as one clean
/// expression (e.g. `Optional[\n  X\n]` → `Optional[X]`).
fn normalize_annotation(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = collapsed.chars().collect();
    let mut out = String::with_capacity(collapsed.len());
    for i in 0..chars.len() {
        if chars[i] == ' ' {
            let prev = i.checked_sub(1).map(|p| chars[p]);
            let next = chars.get(i + 1).copied();
            // Drop spaces just inside an opening bracket or just before a
            // closing bracket / comma; keep the space after a comma.
            if matches!(prev, Some('(' | '[' | '{')) || matches!(next, Some(')' | ']' | '}' | ','))
            {
                continue;
            }
        }
        out.push(chars[i]);
    }
    out
}

/// Strip the surrounding quotes of a stringized annotation so the contents show
/// as written (e.g. `"pa.Table"` → `pa.Table`). Non-string annotations and
/// strings whose quote recurs inside are returned unchanged.
fn strip_string_quotes(s: &str) -> &str {
    for triple in ["\"\"\"", "'''"] {
        if s.len() >= 6 && s.starts_with(triple) && s.ends_with(triple) {
            return &s[3..s.len() - 3];
        }
    }
    for quote in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(quote) && s.ends_with(quote) {
            let inner = &s[1..s.len() - 1];
            if !inner.contains(quote) {
                return inner;
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotated(s: &str) -> SourceAnnotation {
        SourceAnnotation::Annotated(s.to_string())
    }

    #[test]
    fn return_annotation_simple() {
        let src = "def f(self) -> pa.Table:\n    return x\n";
        assert_eq!(extract_return_annotation(src, 0), annotated("pa.Table"));
    }

    #[test]
    fn return_annotation_with_param_colons() {
        // Parameter annotations contain colons that must not be mistaken for the
        // header-terminating colon.
        let src = "def f(self, x: int, y: dict[str, int]) -> Result:\n    pass\n";
        assert_eq!(extract_return_annotation(src, 0), annotated("Result"));
    }

    #[test]
    fn return_annotation_none() {
        let src = "def untyped(self):\n    return something()\n";
        assert_eq!(extract_return_annotation(src, 0), SourceAnnotation::Unannotated);
    }

    #[test]
    fn return_annotation_multiline_signature() {
        let src = "def assist(\n    self,\n    task: str,\n) -> pa.Table:\n    return x\n";
        assert_eq!(extract_return_annotation(src, 0), annotated("pa.Table"));
    }

    #[test]
    fn return_annotation_multiline_annotation() {
        let src = "def f(self) -> Optional[\n    pa.Table\n]:\n    return None\n";
        assert_eq!(extract_return_annotation(src, 0), annotated("Optional[pa.Table]"));
    }

    #[test]
    fn return_annotation_stringized() {
        let src = "def f(self) -> \"pa.Table\":\n    return x\n";
        assert_eq!(extract_return_annotation(src, 0), annotated("pa.Table"));
    }

    #[test]
    fn return_annotation_trailing_comment() {
        let src = "def f(self) -> pa.Table:  # external type\n    return x\n";
        assert_eq!(extract_return_annotation(src, 0), annotated("pa.Table"));
    }

    #[test]
    fn return_annotation_comment_inside_signature() {
        let src = "def f(\n    self,  # receiver\n) -> pa.Table:\n    return x\n";
        assert_eq!(extract_return_annotation(src, 0), annotated("pa.Table"));
    }

    #[test]
    fn return_annotation_skips_decorators() {
        let src = "@property\n@cached\ndef f(self) -> pa.Table:\n    return x\n";
        assert_eq!(extract_return_annotation(src, 0), annotated("pa.Table"));
    }

    #[test]
    fn return_annotation_async() {
        let src = "async def f(self) -> pa.Table:\n    return x\n";
        assert_eq!(extract_return_annotation(src, 0), annotated("pa.Table"));
    }

    #[test]
    fn variable_annotation_simple() {
        let src = "registry: dict[str, Plugin] = {}\n";
        assert_eq!(extract_variable_annotation(src, 0), annotated("dict[str, Plugin]"));
    }

    #[test]
    fn variable_annotation_no_value() {
        let src = "registry: pa.Table\n";
        assert_eq!(extract_variable_annotation(src, 0), annotated("pa.Table"));
    }

    #[test]
    fn variable_annotation_none() {
        let src = "registry = pa.array([])\n";
        assert_eq!(extract_variable_annotation(src, 0), SourceAnnotation::Unannotated);
    }

    #[test]
    fn variable_annotation_dict_value_with_colons() {
        // No annotation; the colons live inside the value dict and must be
        // ignored (depth > 0).
        let src = "config = {\"a\": 1, \"b\": 2}\n";
        assert_eq!(extract_variable_annotation(src, 0), SourceAnnotation::Unannotated);
    }

    #[test]
    fn variable_annotation_stringized() {
        let src = "registry: \"pa.Table\" = make()\n";
        assert_eq!(extract_variable_annotation(src, 0), annotated("pa.Table"));
    }

    #[test]
    fn substitute_return_unknown_with_annotation() {
        let src = "def get_table(self) -> pa.Table:\n    return x\n";
        assert_eq!(
            substitute_unknown("get_table(self) -> Unknown", src, 0),
            "get_table(self) -> pa.Table"
        );
    }

    #[test]
    fn substitute_return_unknown_no_annotation() {
        let src = "def untyped(self):\n    return x\n";
        assert_eq!(
            substitute_unknown("untyped(self) -> Unknown", src, 0),
            "untyped(self) -> (unannotated)"
        );
    }

    #[test]
    fn substitute_named_variable_unknown() {
        let src = "registry: pa.Table = make()\n";
        assert_eq!(substitute_unknown("registry: Unknown", src, 0), "registry: pa.Table");
    }

    #[test]
    fn substitute_named_variable_unannotated() {
        let src = "registry = make()\n";
        assert_eq!(substitute_unknown("registry: Unknown", src, 0), "registry: (unannotated)");
    }

    #[test]
    fn substitute_bare_variable_unknown() {
        let src = "value: pa.Table = make()\n";
        assert_eq!(substitute_unknown("Unknown", src, 0), "pa.Table");
    }

    #[test]
    fn substitute_show_def_form() {
        let src = "def get_table(self) -> pa.Table:\n    return x\n";
        assert_eq!(
            substitute_unknown("def get_table(self) -> Unknown", src, 0),
            "def get_table(self) -> pa.Table"
        );
    }

    #[test]
    fn substitute_resolved_type_unchanged() {
        // No `Unknown` present — must be a no-op (no regression / no scan).
        let src = "def f(self) -> str:\n    return x\n";
        assert_eq!(substitute_unknown("f(self) -> str", src, 0), "f(self) -> str");
    }

    #[test]
    fn extract_out_of_range_line_is_unannotated() {
        let src = "def f(self) -> pa.Table:\n    return x\n";
        assert_eq!(extract_return_annotation(src, 999), SourceAnnotation::Unannotated);
        assert_eq!(extract_variable_annotation(src, 999), SourceAnnotation::Unannotated);
    }

    #[test]
    fn substitute_out_of_range_line_marks_unannotated() {
        // Guards against panics when the def line is past EOF.
        assert_eq!(
            substitute_unknown("f(self) -> Unknown", "x = 1\n", 999),
            "f(self) -> (unannotated)"
        );
    }

    #[test]
    fn substitute_partial_unknown_untouched() {
        // Outer type resolved; inner Unknown is left as ty rendered it.
        let src = "registry: dict[str, Plugin] = {}\n";
        assert_eq!(
            substitute_unknown("registry: dict[str, Unknown]", src, 0),
            "registry: dict[str, Unknown]"
        );
    }

    #[test]
    fn substitute_param_unknown_untouched() {
        // Return type resolved; an Unknown parameter is not substituted.
        let src = "def f(self, x) -> str:\n    pass\n";
        assert_eq!(
            substitute_unknown("f(self, x: Unknown) -> str", src, 0),
            "f(self, x: Unknown) -> str"
        );
    }
}
