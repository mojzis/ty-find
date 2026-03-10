use crate::cli::args::{OutputDetail, OutputFormat};
use crate::cli::style::Styler;
#[cfg(unix)]
use crate::daemon::protocol::{MemberInfo, MembersResult};
use crate::lsp::protocol::{
    DocumentSymbol, Hover, HoverContents, Location, MarkedStringOrString, SymbolInformation,
    SymbolKind,
};
use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// Pre-read file contents for non-blocking source line lookups during formatting.
///
/// Built asynchronously (via `tokio::fs`) in command handlers, then passed into
/// synchronous formatters so they never block the async runtime on file I/O.
pub struct SourceCache {
    files: HashMap<String, String>,
}

impl SourceCache {
    /// Create an empty cache (for tests that don't need source).
    #[cfg(test)]
    pub fn new() -> Self {
        Self { files: HashMap::new() }
    }

    /// Asynchronously read all files referenced by the given `file://` URIs.
    ///
    /// Deduplicates paths and silently skips files that cannot be read.
    pub async fn from_uris<'a>(uris: impl IntoIterator<Item = &'a str>) -> Self {
        let mut paths: Vec<String> = uris
            .into_iter()
            .filter_map(|uri| uri.strip_prefix("file://").map(String::from))
            .collect();
        paths.sort();
        paths.dedup();

        let mut files = HashMap::with_capacity(paths.len());
        for path in paths {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                files.insert(path, content);
            }
        }
        Self { files }
    }

    /// Get the full content of a cached file by absolute path.
    fn get_content(&self, file_path: &str) -> Option<&str> {
        self.files.get(file_path).map(String::as_str)
    }

    #[cfg(test)]
    fn from_entries(entries: impl IntoIterator<Item = (String, String)>) -> Self {
        Self { files: entries.into_iter().collect() }
    }
}

/// A reference location enriched with enclosing symbol context.
#[derive(Clone, Debug)]
pub struct EnrichedReference {
    pub location: Location,
    /// Dot-separated path of the tightest enclosing symbol (e.g. "RequestHandler.process"),
    /// or "module scope" if at top level.
    pub context: String,
}

/// A single show result with optional symbol kind.
pub struct ShowEntry<'a> {
    pub symbol: &'a str,
    pub kind: Option<&'a SymbolKind>,
    pub definitions: &'a [Location],
    pub hover: Option<&'a Hover>,
    /// Total number of references found (always populated, used for count summary).
    pub total_reference_count: usize,
    /// Number of unique files across all references.
    pub total_reference_files: usize,
    /// Displayed references (capped by --references-limit), enriched with context.
    pub displayed_references: Vec<EnrichedReference>,
    /// How many references were not displayed due to the limit.
    pub remaining_reference_count: usize,
    /// Whether individual references should be shown (true = -r was passed).
    pub show_individual_refs: bool,
    /// Whether the docstring section should be shown (true = --doc or --all was passed).
    pub show_doc: bool,
    /// Test references separated from the main refs (None = no test refs exist).
    pub test_references: Option<TestReferencesSection>,
}

impl ShowEntry<'_> {
    /// Returns true when all sections (definitions, type/hover, references) are empty.
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty() && self.hover.is_none() && self.total_reference_count == 0
    }
}

pub struct OutputFormatter {
    format: OutputFormat,
    detail: OutputDetail,
    cwd: PathBuf,
    s: Styler,
}

/// Read a single line of source code from the cache (1-based line number).
fn read_source_line(cache: &SourceCache, file_path: &str, line: u32) -> Option<String> {
    let content = cache.get_content(file_path)?;
    content.lines().nth((line - 1) as usize).map(|s| s.trim().to_string())
}

/// Context around a definition: decorator lines and the keyword line.
struct DefinitionContext {
    /// Decorator lines (e.g. `@dataclass`, `@property`), if any.
    decorators: Option<String>,
    /// The `class`/`def` keyword line (e.g. `class Dog(Animal):`).
    definition_line: String,
}

/// Read definition context from a file at a 0-indexed starting line.
///
/// Handles both cases:
/// - Start line is at a decorator (workspace symbols): scans forward past `@`
///   lines, then also scans backwards from the keyword line to capture any
///   decorators above it.
/// - Start line is at the `class`/`def` keyword (`execute_definition`): scans
///   backwards to find decorators above.
fn read_definition_context(
    cache: &SourceCache,
    file_path: &str,
    start_line_0: u32,
) -> Option<DefinitionContext> {
    let content = cache.get_content(file_path)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = start_line_0 as usize;

    // Find the keyword line by scanning forward past decorators
    let mut def_idx = None;
    for (i, line) in lines.iter().enumerate().skip(start) {
        if !line.trim().starts_with('@') {
            def_idx = Some(i);
            break;
        }
    }
    let def_idx = def_idx?;
    let definition_line = lines[def_idx].trim().to_string();

    // Scan backwards from the keyword line to collect all decorators
    let mut decorator_lines = Vec::new();
    let mut idx = def_idx;
    while idx > 0 {
        idx -= 1;
        let trimmed = lines[idx].trim();
        if trimmed.starts_with('@') {
            decorator_lines.push(trimmed);
        } else {
            break;
        }
    }

    let decorators = if decorator_lines.is_empty() {
        None
    } else {
        decorator_lines.reverse(); // restore top-to-bottom order
        Some(decorator_lines.into_iter().collect::<Vec<_>>().join("\n"))
    };

    Some(DefinitionContext { decorators, definition_line })
}

#[cfg(test)]
fn read_decorators(cache: &SourceCache, file_path: &str, start_line_0: u32) -> Option<String> {
    read_definition_context(cache, file_path, start_line_0).and_then(|ctx| ctx.decorators)
}

#[cfg(test)]
fn read_definition_line(cache: &SourceCache, file_path: &str, start_line_0: u32) -> Option<String> {
    read_definition_context(cache, file_path, start_line_0).map(|ctx| ctx.definition_line)
}

/// Test references that were separated from the main results.
pub struct TestReferencesSection {
    /// Total test references found.
    pub total_count: usize,
    /// Displayed test references (enriched with context).
    pub displayed: Vec<EnrichedReference>,
    /// How many test references were not displayed due to the limit.
    pub remaining_count: usize,
}

/// Enriched references result for the references command.
pub struct EnrichedReferencesResult {
    /// Symbol name or query label.
    pub label: String,
    /// Total number of non-test references found.
    pub total_count: usize,
    /// Displayed non-test references (capped by limit), enriched with context.
    pub displayed: Vec<EnrichedReference>,
    /// How many non-test references were not displayed due to the limit.
    pub remaining_count: usize,
    /// Test references shown separately (None = no test refs exist).
    pub test_references: Option<TestReferencesSection>,
}

/// Check whether a position (line, character) is inside a range (inclusive).
fn position_in_range(range: &crate::lsp::protocol::Range, line: u32, character: u32) -> bool {
    if line < range.start.line || line > range.end.line {
        return false;
    }
    if line == range.start.line && character < range.start.character {
        return false;
    }
    if line == range.end.line && character > range.end.character {
        return false;
    }
    true
}

/// Walk a `DocumentSymbol` tree to find the tightest enclosing symbol for a position.
///
/// Returns the dot-separated path (e.g. `"MyClass.my_method"`) of the deepest
/// symbol whose `range` contains the given position, or `None` if the position
/// is outside all symbols (module scope).
pub fn find_enclosing_symbol(
    symbols: &[DocumentSymbol],
    line: u32,
    character: u32,
) -> Option<String> {
    let mut path = Vec::new();
    find_enclosing_recursive(symbols, line, character, &mut path);
    if path.is_empty() {
        None
    } else {
        Some(path.join("."))
    }
}

fn find_enclosing_recursive(
    symbols: &[DocumentSymbol],
    line: u32,
    character: u32,
    path: &mut Vec<String>,
) {
    for sym in symbols {
        if position_in_range(&sym.range, line, character) {
            path.push(sym.name.clone());
            if let Some(children) = &sym.children {
                find_enclosing_recursive(children, line, character, path);
            }
            return;
        }
    }
}

/// Strip markdown code fences (`` ```lang `` / `` ``` ``) leaving only content.
fn strip_code_fences(text: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n")
}

impl OutputFormatter {
    #[cfg(test)]
    pub fn new(format: OutputFormat) -> Self {
        Self::with_detail_and_styler(format, OutputDetail::default(), Styler::no_color())
    }

    pub fn with_detail(format: OutputFormat, detail: OutputDetail, styler: Styler) -> Self {
        Self::with_detail_and_styler(format, detail, styler)
    }

    fn with_detail_and_styler(format: OutputFormat, detail: OutputDetail, styler: Styler) -> Self {
        // Non-human formats never get color, regardless of the flag.
        let s = match format {
            OutputFormat::Human => styler,
            _ => Styler::no_color(),
        };
        Self {
            format,
            detail,
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            s,
        }
    }

    /// Access the styler (used for error formatting from main).
    pub fn styler(&self) -> Styler {
        self.s
    }

    pub fn format_definitions(
        &self,
        locations: &[Location],
        query_info: &str,
        cache: &SourceCache,
    ) -> String {
        match self.format {
            OutputFormat::Human => self.format_human(locations, query_info, cache),
            OutputFormat::Json => Self::format_json(locations),
            OutputFormat::Csv => self.format_csv(locations),
            OutputFormat::Paths => self.format_paths(locations),
        }
    }

    fn format_human(
        &self,
        locations: &[Location],
        query_info: &str,
        cache: &SourceCache,
    ) -> String {
        if locations.is_empty() {
            return self.s.error(&format!("No results found for: {query_info}"));
        }

        let mut output = format!("Found {} definition(s) for: {query_info}\n\n", locations.len());

        for (i, location) in locations.iter().enumerate() {
            let file_path = self.uri_to_path(&location.uri);
            let line = location.range.start.line + 1;
            let column = location.range.start.character + 1;

            let _ =
                writeln!(output, "{}. {}", i + 1, self.s.file_location(&file_path, line, column));

            if let Some(src) = read_source_line(cache, &file_path, line) {
                let _ = writeln!(output, "   {src}");
            }
            output.push('\n');
        }

        output
    }

    fn format_json(locations: &[Location]) -> String {
        serde_json::to_string_pretty(locations).unwrap_or_else(|_| "[]".to_string())
    }

    fn format_csv(&self, locations: &[Location]) -> String {
        let mut output = String::from("file,line,column\n");
        for location in locations {
            let file_path = self.uri_to_path(&location.uri);
            let line = location.range.start.line + 1;
            let column = location.range.start.character + 1;
            let _ = writeln!(output, "{file_path},{line},{column}");
        }
        output
    }

    fn format_paths(&self, locations: &[Location]) -> String {
        locations.iter().map(|loc| self.uri_to_path(&loc.uri)).collect::<Vec<_>>().join("\n")
    }

    fn uri_to_path(&self, uri: &str) -> String {
        let abs_path = if let Some(stripped) = uri.strip_prefix("file://") {
            stripped.to_string()
        } else {
            return uri.to_string();
        };

        // Try to make path relative to cwd
        let path = Path::new(&abs_path);
        match path.strip_prefix(&self.cwd) {
            Ok(rel) => rel.display().to_string(),
            Err(_) => abs_path,
        }
    }

    /// Format results for one or more symbol find queries, grouped by symbol.
    pub fn format_find_results(
        &self,
        results: &[(String, Vec<Location>)],
        cache: &SourceCache,
    ) -> String {
        if results.len() == 1 {
            let (symbol, locations) = &results[0];
            if locations.is_empty() {
                return self.s.error(&format!("No results found for: '{symbol}'"));
            }
            let query_info = format!("'{symbol}'");
            return self.format_definitions(locations, &query_info, cache);
        }

        match self.format {
            OutputFormat::Human => {
                let mut output = String::new();
                for (symbol, locations) in results {
                    if locations.is_empty() {
                        let _ = writeln!(
                            output,
                            "{}",
                            self.s.error(&format!("No results found for: '{symbol}'"))
                        );
                        continue;
                    }
                    let _ = writeln!(output, "=== {} ===", self.s.symbol(symbol));
                    {
                        output.push_str(&self.format_human(
                            locations,
                            &format!("'{symbol}'"),
                            cache,
                        ));
                    }
                    output.push('\n');
                }
                output.trim_end().to_string()
            }
            OutputFormat::Json => {
                let grouped: Vec<serde_json::Value> = results
                    .iter()
                    .map(|(symbol, locations)| {
                        serde_json::json!({
                            "symbol": symbol,
                            "definitions": locations,
                        })
                    })
                    .collect();
                serde_json::to_string_pretty(&grouped).unwrap_or_else(|_| "[]".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("symbol,file,line,column\n");
                for (symbol, locations) in results {
                    for location in locations {
                        let file_path = self.uri_to_path(&location.uri);
                        let line = location.range.start.line + 1;
                        let column = location.range.start.character + 1;
                        let _ = writeln!(output, "{symbol},{file_path},{line},{column}");
                    }
                }
                output
            }
            OutputFormat::Paths => {
                let mut paths: Vec<String> = results
                    .iter()
                    .flat_map(|(_, locations)| {
                        locations.iter().map(|loc| self.uri_to_path(&loc.uri))
                    })
                    .collect();
                paths.sort();
                paths.dedup();
                paths.join("\n")
            }
        }
    }

    /// Format enriched references results (with context and limit support).
    pub fn format_enriched_references_results(
        &self,
        results: &[EnrichedReferencesResult],
        cache: &SourceCache,
    ) -> String {
        if results.len() == 1 {
            return self.format_enriched_references_single(&results[0], cache);
        }

        match self.format {
            OutputFormat::Human => {
                let mut output = String::new();
                for result in results {
                    let _ = writeln!(output, "=== {} ===", self.s.symbol(&result.label));
                    output.push_str(&self.format_enriched_references_single(result, cache));
                    output.push('\n');
                }
                output.trim_end().to_string()
            }
            OutputFormat::Json => {
                let grouped: Vec<serde_json::Value> =
                    results.iter().map(Self::enriched_refs_to_json).collect();
                serde_json::to_string_pretty(&grouped).unwrap_or_else(|_| "[]".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("symbol,file,line,column,context,test\n");
                for result in results {
                    for enriched in &result.displayed {
                        let file_path = self.uri_to_path(&enriched.location.uri);
                        let line = enriched.location.range.start.line + 1;
                        let column = enriched.location.range.start.character + 1;
                        let _ = writeln!(
                            output,
                            "{},{file_path},{line},{column},{},false",
                            result.label, enriched.context
                        );
                    }
                    if let Some(test_refs) = &result.test_references {
                        for enriched in &test_refs.displayed {
                            let file_path = self.uri_to_path(&enriched.location.uri);
                            let line = enriched.location.range.start.line + 1;
                            let column = enriched.location.range.start.character + 1;
                            let _ = writeln!(
                                output,
                                "{},{file_path},{line},{column},{},true",
                                result.label, enriched.context
                            );
                        }
                    }
                }
                output
            }
            OutputFormat::Paths => {
                let mut paths: Vec<String> = results
                    .iter()
                    .flat_map(|r| {
                        let main = r.displayed.iter().map(|e| self.uri_to_path(&e.location.uri));
                        let test = r.test_references.iter().flat_map(|t| {
                            t.displayed.iter().map(|e| self.uri_to_path(&e.location.uri))
                        });
                        main.chain(test)
                    })
                    .collect();
                paths.sort();
                paths.dedup();
                paths.join("\n")
            }
        }
    }

    fn format_enriched_references_human(
        &self,
        result: &EnrichedReferencesResult,
        cache: &SourceCache,
    ) -> String {
        if result.total_count == 0
            && result.test_references.as_ref().is_none_or(|t| t.total_count == 0)
        {
            return self.s.error(&format!("No results found for: '{}'", result.label));
        }

        let mut output =
            format!("Found {} reference(s) for: '{}'\n\n", result.total_count, result.label);

        self.write_enriched_ref_list(&mut output, &result.displayed, cache);

        if result.remaining_count > 0 {
            let _ = writeln!(
                output,
                "... and {} more — use --references-limit 0 to show all",
                result.remaining_count
            );
        }

        self.write_test_references_section(&mut output, result.test_references.as_ref(), cache);

        output
    }

    /// Append numbered enriched reference lines (with source) to `output`.
    fn write_enriched_ref_list(
        &self,
        output: &mut String,
        refs: &[EnrichedReference],
        cache: &SourceCache,
    ) {
        for (i, enriched) in refs.iter().enumerate() {
            let file_path = self.uri_to_path(&enriched.location.uri);
            let line = enriched.location.range.start.line + 1;
            let column = enriched.location.range.start.character + 1;

            let _ = writeln!(
                output,
                "{}. {} ({})",
                i + 1,
                self.s.file_location(&file_path, line, column),
                self.s.dim(&enriched.context),
            );

            if let Some(src) = read_source_line(cache, &file_path, line) {
                let _ = writeln!(output, "   {src}");
            }
            output.push('\n');
        }
    }

    /// Append the test references section (or hidden hint) to `output`.
    fn write_test_references_section(
        &self,
        output: &mut String,
        test_references: Option<&TestReferencesSection>,
        cache: &SourceCache,
    ) {
        if let Some(test_refs) = test_references {
            if !test_refs.displayed.is_empty() {
                let heading = format!("Test references ({}):", test_refs.total_count);
                let _ = writeln!(output, "\n{}\n", self.s.heading(&heading));
                self.write_enriched_ref_list(output, &test_refs.displayed, cache);
                if test_refs.remaining_count > 0 {
                    let _ =
                        writeln!(output, "... and {} more test ref(s)", test_refs.remaining_count);
                }
            } else if test_refs.total_count > 0 {
                let heading =
                    format!("Test references: {} (use --tests/-t to show)", test_refs.total_count);
                let _ = writeln!(output, "\n{}", self.s.heading(&heading));
            }
        }
    }

    fn format_enriched_references_single(
        &self,
        result: &EnrichedReferencesResult,
        cache: &SourceCache,
    ) -> String {
        match self.format {
            OutputFormat::Human => self.format_enriched_references_human(result, cache),
            OutputFormat::Json => {
                let val = Self::enriched_refs_to_json(result);
                serde_json::to_string_pretty(&val).unwrap_or_else(|_| "{}".to_string())
            }
            OutputFormat::Csv => {
                let has_test_refs =
                    result.test_references.as_ref().is_some_and(|t| !t.displayed.is_empty());
                let mut output = String::from("file,line,column,context,test\n");
                for enriched in &result.displayed {
                    let file_path = self.uri_to_path(&enriched.location.uri);
                    let line = enriched.location.range.start.line + 1;
                    let column = enriched.location.range.start.character + 1;
                    let _ =
                        writeln!(output, "{file_path},{line},{column},{},false", enriched.context);
                }
                if has_test_refs {
                    if let Some(test_refs) = &result.test_references {
                        for enriched in &test_refs.displayed {
                            let file_path = self.uri_to_path(&enriched.location.uri);
                            let line = enriched.location.range.start.line + 1;
                            let column = enriched.location.range.start.character + 1;
                            let _ = writeln!(
                                output,
                                "{file_path},{line},{column},{},true",
                                enriched.context
                            );
                        }
                    }
                }
                output
            }
            OutputFormat::Paths => {
                let mut paths: Vec<String> =
                    result.displayed.iter().map(|r| self.uri_to_path(&r.location.uri)).collect();
                if let Some(test_refs) = &result.test_references {
                    paths.extend(
                        test_refs.displayed.iter().map(|r| self.uri_to_path(&r.location.uri)),
                    );
                }
                paths.sort();
                paths.dedup();
                paths.join("\n")
            }
        }
    }

    fn enriched_refs_to_json(result: &EnrichedReferencesResult) -> serde_json::Value {
        let refs_json: Vec<serde_json::Value> =
            result.displayed.iter().map(Self::enriched_ref_to_json).collect();

        let test_refs_json: Vec<serde_json::Value> =
            result.test_references.as_ref().map_or_else(Vec::new, |t| {
                t.displayed.iter().map(Self::enriched_ref_to_json).collect()
            });

        let test_count = result.test_references.as_ref().map_or(0, |t| t.total_count);

        serde_json::json!({
            "symbol": result.label,
            "reference_count": result.total_count,
            "references": refs_json,
            "test_reference_count": test_count,
            "test_references": test_refs_json,
        })
    }

    fn enriched_ref_to_json(r: &EnrichedReference) -> serde_json::Value {
        let file_path = r.location.uri.strip_prefix("file://").unwrap_or(&r.location.uri);
        serde_json::json!({
            "file": file_path,
            "line": r.location.range.start.line + 1,
            "column": r.location.range.start.character + 1,
            "context": r.context,
        })
    }

    pub fn format_workspace_symbols(&self, symbols: &[SymbolInformation]) -> String {
        match self.format {
            OutputFormat::Human => {
                let mut output = String::new();

                for (i, symbol) in symbols.iter().enumerate() {
                    let file_path = self.uri_to_path(&symbol.location.uri);
                    let line = symbol.location.range.start.line + 1;
                    let column = symbol.location.range.start.character + 1;

                    let kind_str = format!("({:?})", symbol.kind);
                    let _ = write!(
                        output,
                        "{}. {} {}\n   {}\n\n",
                        i + 1,
                        self.s.symbol(&symbol.name),
                        self.s.dim(&kind_str),
                        self.s.file_location(&file_path, line, column),
                    );
                }

                output
            }
            OutputFormat::Json => {
                serde_json::to_string_pretty(symbols).unwrap_or_else(|_| "[]".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("name,kind,file,line,column\n");
                for symbol in symbols {
                    let file_path = self.uri_to_path(&symbol.location.uri);
                    let line = symbol.location.range.start.line + 1;
                    let column = symbol.location.range.start.character + 1;
                    let _ = writeln!(
                        output,
                        "{},{:?},{file_path},{line},{column}",
                        symbol.name, symbol.kind,
                    );
                }
                output
            }
            OutputFormat::Paths => symbols
                .iter()
                .map(|symbol| self.uri_to_path(&symbol.location.uri))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    pub fn format_document_symbols(&self, symbols: &[DocumentSymbol]) -> String {
        match self.format {
            OutputFormat::Human => {
                let mut output = String::new();
                format_document_symbols_recursive(symbols, 0, &mut output);
                output
            }
            OutputFormat::Json => {
                serde_json::to_string_pretty(symbols).unwrap_or_else(|_| "[]".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("name,kind,line,column\n");
                format_document_symbols_csv(symbols, &mut output);
                output
            }
            OutputFormat::Paths => {
                // Paths format doesn't make sense for document symbols, fall back to human
                let mut output = String::new();
                format_document_symbols_recursive(symbols, 0, &mut output);
                output
            }
        }
    }

    fn extract_hover_text(contents: &HoverContents) -> String {
        match contents {
            HoverContents::Scalar(s) => s.clone(),
            HoverContents::Markup(markup) => markup.value.clone(),
            HoverContents::MarkedString(ms) => ms.value.clone(),
            HoverContents::Array(arr) => arr
                .iter()
                .map(|item| match item {
                    MarkedStringOrString::String(s) => s.clone(),
                    MarkedStringOrString::MarkedString(ms) => ms.value.clone(),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Extract just the type signature from hover, stripping docstrings and code fences.
    ///
    /// ty's hover markdown is structured as:
    ///   ```lang\n<type info>\n```\n---\nDocstring...
    ///
    /// Returns the bare type text without markdown fences or docstring.
    fn extract_hover_type(contents: &HoverContents) -> String {
        let full = Self::extract_hover_text(contents);

        // Strip docstring: everything after the first "\n---" separator
        let type_part = match full.find("\n---") {
            Some(pos) => &full[..pos],
            None => &full,
        };

        // Strip code fences (```python, ```xml, etc.)
        strip_code_fences(type_part)
    }

    /// Extract just the docstring portion from hover, if present.
    ///
    /// Returns `None` if there is no `---` separator (i.e. no docstring).
    fn extract_hover_doc(contents: &HoverContents) -> Option<String> {
        let full = Self::extract_hover_text(contents);
        let pos = full.find("\n---")?;
        let doc = full[pos + 4..].trim(); // skip "\n---"
        if doc.is_empty() {
            None
        } else {
            Some(doc.to_string())
        }
    }

    /// Short label for a `SymbolKind`, used in condensed output.
    fn kind_label(kind: &SymbolKind) -> &'static str {
        match kind {
            SymbolKind::Function => "func",
            SymbolKind::Method => "method",
            SymbolKind::Class => "class",
            SymbolKind::Variable => "var",
            SymbolKind::Constant => "const",
            SymbolKind::Module => "module",
            SymbolKind::Property => "prop",
            SymbolKind::Field => "field",
            SymbolKind::Constructor => "ctor",
            SymbolKind::Enum => "enum",
            SymbolKind::Interface => "iface",
            SymbolKind::Struct => "struct",
            SymbolKind::EnumMember => "member",
            SymbolKind::TypeParameter => "type",
            _ => "symbol",
        }
    }

    /// Write the type section content to `output`.
    ///
    /// For class definitions: shows decorators + source `class` line (preserves
    /// inheritance info that ty's hover strips away).
    /// For everything else: shows decorators + hover type (has inferred return types).
    /// Falls back to `empty_label` when no information is available.
    fn write_type_section(
        &self,
        output: &mut String,
        location: Option<&Location>,
        hover: Option<&Hover>,
        empty_label: &str,
        cache: &SourceCache,
    ) {
        if let Some(location) = location {
            let file_path = self.uri_to_path(&location.uri);
            if let Some(ctx) = read_definition_context(cache, &file_path, location.range.start.line)
            {
                // Show decorators
                if let Some(decs) = &ctx.decorators {
                    output.push_str(decs);
                    output.push('\n');
                }
                // For classes, prefer the source definition line (shows inheritance)
                // over ty's bare `<class 'X'>` which doesn't.
                if ctx.definition_line.starts_with("class ") {
                    output.push_str(&ctx.definition_line);
                    output.push('\n');
                    return;
                }
            }
        }

        // Non-class or source not readable: use hover type
        if let Some(hover) = hover {
            output.push_str(&Self::extract_hover_type(&hover.contents));
            output.push('\n');
        } else {
            output.push_str(empty_label);
        }
    }

    /// Format a single symbol show, using the header level appropriate for context.
    /// `h_level` controls markdown heading depth (1 = `#`, 2 = `##`).
    fn format_show_human(&self, entry: &ShowEntry<'_>, h_level: u8, cache: &SourceCache) -> String {
        match self.detail {
            OutputDetail::Condensed => self.format_show_condensed(entry, h_level, cache),
            OutputDetail::Full => self.format_show_full(entry, h_level, cache),
        }
    }

    fn format_show_condensed(
        &self,
        entry: &ShowEntry<'_>,
        h_level: u8,
        cache: &SourceCache,
    ) -> String {
        let h = "#".repeat(h_level as usize);
        let mut output = String::new();

        // Definition section — paths only, no source snippets (symbol name is already known)
        if let Some(kind) = entry.kind {
            let heading = format!("{h} Definition ({})", Self::kind_label(kind));
            let _ = writeln!(output, "{}", self.s.heading(&heading));
        } else {
            let heading = format!("{h} Definition");
            let _ = writeln!(output, "{}", self.s.heading(&heading));
        }
        if entry.definitions.is_empty() {
            output.push_str("(none)\n");
        } else {
            for location in entry.definitions {
                let file_path = self.uri_to_path(&location.uri);
                let line = location.range.start.line + 1;
                let column = location.range.start.character + 1;
                let _ = writeln!(output, "{}", self.s.file_location(&file_path, line, column));
            }
        }

        // Signature section — always shown, compact placeholder when empty
        let sig_heading = format!("\n{h} Signature");
        let _ = writeln!(output, "{}", self.s.heading(&sig_heading));
        self.write_type_section(
            &mut output,
            entry.definitions.first(),
            entry.hover,
            "(none)\n",
            cache,
        );

        // Doc section — only shown when --doc or --all is passed and a docstring is present
        if entry.show_doc {
            if let Some(hover) = entry.hover {
                if let Some(doc) = Self::extract_hover_doc(&hover.contents) {
                    let doc_heading = format!("\n{h} Doc");
                    let _ = writeln!(output, "{}", self.s.heading(&doc_heading));
                    output.push_str(&doc);
                    output.push('\n');
                }
            }
        }

        // Refs section — always show count summary
        if entry.total_reference_count == 0 {
            let refs_heading = format!("\n{h} Refs: none");
            let _ = writeln!(output, "{}", self.s.heading(&refs_heading));
        } else {
            let refs_heading = format!(
                "\n{h} Refs: {} across {} file(s)",
                entry.total_reference_count, entry.total_reference_files
            );
            let _ = writeln!(output, "{}", self.s.heading(&refs_heading));
            if entry.show_individual_refs {
                for enriched in &entry.displayed_references {
                    let file_path = self.uri_to_path(&enriched.location.uri);
                    let line = enriched.location.range.start.line + 1;
                    let column = enriched.location.range.start.character + 1;
                    let _ = writeln!(
                        output,
                        "{} ({})",
                        self.s.file_location(&file_path, line, column),
                        self.s.dim(&enriched.context),
                    );
                }
                if entry.remaining_reference_count > 0 {
                    let _ = writeln!(
                        output,
                        "... and {} more — use --references-limit 0 to show all",
                        entry.remaining_reference_count
                    );
                }
            }
        }

        // Test refs section
        if let Some(test_refs) = &entry.test_references {
            if !test_refs.displayed.is_empty() {
                let test_files: std::collections::HashSet<&str> =
                    test_refs.displayed.iter().map(|r| r.location.uri.as_str()).collect();
                let test_heading = format!(
                    "\n{h} Test Refs: {} across {} file(s)",
                    test_refs.total_count,
                    test_files.len()
                );
                let _ = writeln!(output, "{}", self.s.heading(&test_heading));
                for enriched in &test_refs.displayed {
                    let file_path = self.uri_to_path(&enriched.location.uri);
                    let line = enriched.location.range.start.line + 1;
                    let column = enriched.location.range.start.character + 1;
                    let _ = writeln!(
                        output,
                        "{} ({})",
                        self.s.file_location(&file_path, line, column),
                        self.s.dim(&enriched.context),
                    );
                }
                if test_refs.remaining_count > 0 {
                    let _ =
                        writeln!(output, "... and {} more test ref(s)", test_refs.remaining_count);
                }
            } else if test_refs.total_count > 0 {
                let test_heading =
                    format!("\n{h} Test Refs: {} (use --tests/-t to show)", test_refs.total_count);
                let _ = writeln!(output, "{}", self.s.heading(&test_heading));
            }
        }

        output
    }

    #[allow(clippy::too_many_lines)]
    fn format_show_full(&self, entry: &ShowEntry<'_>, h_level: u8, cache: &SourceCache) -> String {
        let h = "#".repeat(h_level as usize);
        let h2 = "#".repeat(h_level as usize + 1);

        let show_heading = format!("{h} Show: {}", entry.symbol);
        let mut output = format!("{}\n\n", self.s.heading(&show_heading));

        // Definition section
        let def_heading = format!("{h2} Definition");
        let _ = writeln!(output, "{}", self.s.heading(&def_heading));
        if entry.definitions.is_empty() {
            output.push_str("No definitions found.\n");
        } else {
            for (i, location) in entry.definitions.iter().enumerate() {
                let file_path = self.uri_to_path(&location.uri);
                let line = location.range.start.line + 1;
                let column = location.range.start.character + 1;
                let _ = writeln!(
                    output,
                    "{}. {}",
                    i + 1,
                    self.s.file_location(&file_path, line, column)
                );

                if let Some(src) = read_source_line(cache, &file_path, line) {
                    let _ = writeln!(output, "   {src}");
                }
            }
        }
        output.push('\n');

        // Signature section — same class-vs-other logic as condensed mode
        let sig_heading = format!("{h2} Signature");
        let _ = writeln!(output, "{}", self.s.heading(&sig_heading));
        self.write_type_section(
            &mut output,
            entry.definitions.first(),
            entry.hover,
            "No hover information available.\n",
            cache,
        );
        output.push('\n');

        // Doc section — only shown when --doc or --all is passed and a docstring is present
        if entry.show_doc {
            if let Some(hover) = entry.hover {
                if let Some(doc) = Self::extract_hover_doc(&hover.contents) {
                    let doc_heading = format!("{h2} Doc");
                    let _ = writeln!(output, "{}", self.s.heading(&doc_heading));
                    output.push_str(&doc);
                    output.push_str("\n\n");
                }
            }
        }

        // References section — always show count summary
        let refs_heading = format!("{h2} References");
        let _ = writeln!(output, "{}", self.s.heading(&refs_heading));
        if entry.total_reference_count == 0 {
            output.push_str("No references found.\n");
        } else {
            let _ = writeln!(
                output,
                "{} reference(s) across {} file(s):",
                entry.total_reference_count, entry.total_reference_files
            );
            if entry.show_individual_refs {
                for (i, enriched) in entry.displayed_references.iter().enumerate() {
                    let file_path = self.uri_to_path(&enriched.location.uri);
                    let line = enriched.location.range.start.line + 1;
                    let column = enriched.location.range.start.character + 1;
                    let _ = writeln!(
                        output,
                        "{}. {} ({})",
                        i + 1,
                        self.s.file_location(&file_path, line, column),
                        self.s.dim(&enriched.context),
                    );

                    if let Some(src) = read_source_line(cache, &file_path, line) {
                        let _ = writeln!(output, "   {src}");
                    }
                }
                if entry.remaining_reference_count > 0 {
                    let _ = writeln!(
                        output,
                        "... and {} more — use --references-limit 0 to show all",
                        entry.remaining_reference_count
                    );
                }
            }
        }

        // Test references section
        if let Some(test_refs) = &entry.test_references {
            if !test_refs.displayed.is_empty() {
                output.push('\n');
                let _ = writeln!(output, "{h2} Test References");
                let _ = writeln!(output, "{} test reference(s):", test_refs.total_count);
                for (i, enriched) in test_refs.displayed.iter().enumerate() {
                    let file_path = self.uri_to_path(&enriched.location.uri);
                    let line = enriched.location.range.start.line + 1;
                    let column = enriched.location.range.start.character + 1;
                    let _ = writeln!(
                        output,
                        "{}. {file_path}:{line}:{column} ({})",
                        i + 1,
                        enriched.context
                    );
                    if let Some(src) = read_source_line(cache, &file_path, line) {
                        let _ = writeln!(output, "   {src}");
                    }
                }
                if test_refs.remaining_count > 0 {
                    let _ =
                        writeln!(output, "... and {} more test ref(s)", test_refs.remaining_count);
                }
            } else if test_refs.total_count > 0 {
                let test_heading =
                    format!("\n{h2} Test Refs: {} (use --tests/-t to show)", test_refs.total_count);
                let _ = writeln!(output, "{}", self.s.heading(&test_heading));
            }
        }

        output
    }

    pub fn format_show(&self, entry: &ShowEntry<'_>, cache: &SourceCache) -> String {
        if entry.is_empty() && self.format == OutputFormat::Human {
            return self.s.error(&format!("No results found for: '{}'", entry.symbol));
        }
        match self.format {
            OutputFormat::Human => self.format_show_human(entry, 1, cache),
            OutputFormat::Json => Self::format_show_json_single(entry),
            OutputFormat::Csv => self.format_show_csv_single(entry, false),
            OutputFormat::Paths => self.format_show_paths_single(entry),
        }
    }

    fn format_show_json_single(entry: &ShowEntry<'_>) -> String {
        let refs_json: Vec<serde_json::Value> =
            entry.displayed_references.iter().map(Self::enriched_ref_to_json).collect();

        let test_refs_json: Vec<serde_json::Value> =
            entry.test_references.as_ref().map_or_else(Vec::new, |t| {
                t.displayed.iter().map(Self::enriched_ref_to_json).collect()
            });

        let test_count = entry.test_references.as_ref().map_or(0, |t| t.total_count);

        let signature = entry.hover.as_ref().and_then(|h| {
            let t = Self::extract_hover_type(&h.contents);
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        });

        let doc = if entry.show_doc {
            entry.hover.as_ref().and_then(|h| Self::extract_hover_doc(&h.contents))
        } else {
            None
        };

        let json_val = serde_json::json!({
            "symbol": entry.symbol,
            "kind": entry.kind.map(Self::kind_label),
            "definitions": entry.definitions,
            "signature": signature,
            "doc": doc,
            "reference_count": entry.total_reference_count,
            "reference_files": entry.total_reference_files,
            "references": refs_json,
            "test_reference_count": test_count,
            "test_references": test_refs_json,
        });
        serde_json::to_string_pretty(&json_val).unwrap_or_else(|_| "{}".to_string())
    }

    fn format_show_csv_single(&self, entry: &ShowEntry<'_>, include_symbol: bool) -> String {
        let header = if include_symbol {
            "symbol,section,file,line,column,context\n"
        } else {
            "section,file,line,column,context\n"
        };
        let mut output = String::from(header);
        let prefix = if include_symbol { format!("{},", entry.symbol) } else { String::new() };
        for location in entry.definitions {
            let file_path = self.uri_to_path(&location.uri);
            let line = location.range.start.line + 1;
            let column = location.range.start.character + 1;
            let _ = writeln!(output, "{prefix}definition,{file_path},{line},{column},");
        }
        for enriched in &entry.displayed_references {
            let file_path = self.uri_to_path(&enriched.location.uri);
            let line = enriched.location.range.start.line + 1;
            let column = enriched.location.range.start.character + 1;
            let _ = writeln!(
                output,
                "{prefix}reference,{file_path},{line},{column},{}",
                enriched.context
            );
        }
        if let Some(test_refs) = &entry.test_references {
            for enriched in &test_refs.displayed {
                let file_path = self.uri_to_path(&enriched.location.uri);
                let line = enriched.location.range.start.line + 1;
                let column = enriched.location.range.start.character + 1;
                let _ = writeln!(
                    output,
                    "{prefix}test_reference,{file_path},{line},{column},{}",
                    enriched.context
                );
            }
        }
        output
    }

    fn format_show_paths_single(&self, entry: &ShowEntry<'_>) -> String {
        let mut paths: Vec<String> = entry
            .definitions
            .iter()
            .map(|loc| self.uri_to_path(&loc.uri))
            .chain(entry.displayed_references.iter().map(|r| self.uri_to_path(&r.location.uri)))
            .chain(
                entry
                    .test_references
                    .iter()
                    .flat_map(|t| t.displayed.iter().map(|r| self.uri_to_path(&r.location.uri))),
            )
            .collect();
        paths.sort();
        paths.dedup();
        paths.join("\n")
    }

    /// Format results for one or more symbol show queries, grouped by symbol.
    pub fn format_show_results(&self, results: &[ShowEntry<'_>], cache: &SourceCache) -> String {
        if results.len() == 1 {
            return self.format_show(&results[0], cache);
        }

        match self.format {
            OutputFormat::Human => {
                let mut output = String::new();
                for entry in results {
                    if entry.is_empty() {
                        let _ = writeln!(
                            output,
                            "{}",
                            self.s.error(&format!("No results found for: '{}'", entry.symbol))
                        );
                    } else {
                        // Multi-symbol: symbol name gets top-level heading, sections get sub-headings
                        let symbol_heading = format!("# {}", entry.symbol);
                        let _ = writeln!(output, "{}", self.s.symbol(&symbol_heading));
                        output.push_str(&self.format_show_human(entry, 2, cache));
                        output.push('\n');
                    }
                }
                output.trim_end().to_string()
            }
            OutputFormat::Json => {
                let grouped: Vec<serde_json::Value> = results
                    .iter()
                    .map(|entry| {
                        serde_json::from_str(&Self::format_show_json_single(entry))
                            .unwrap_or_default()
                    })
                    .collect();
                serde_json::to_string_pretty(&grouped).unwrap_or_else(|_| "[]".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("symbol,section,file,line,column,context\n");
                for entry in results {
                    // Skip the header from each entry — we already wrote it
                    let entry_csv = self.format_show_csv_single(entry, true);
                    for line in entry_csv.lines().skip(1) {
                        output.push_str(line);
                        output.push('\n');
                    }
                }
                output
            }
            OutputFormat::Paths => {
                let mut paths: Vec<String> = results
                    .iter()
                    .flat_map(|entry| {
                        entry
                            .definitions
                            .iter()
                            .map(|loc| self.uri_to_path(&loc.uri))
                            .chain(
                                entry
                                    .displayed_references
                                    .iter()
                                    .map(|r| self.uri_to_path(&r.location.uri)),
                            )
                            .chain(entry.test_references.iter().flat_map(|t| {
                                t.displayed.iter().map(|r| self.uri_to_path(&r.location.uri))
                            }))
                    })
                    .collect();
                paths.sort();
                paths.dedup();
                paths.join("\n")
            }
        }
    }
}

/// Categorize members into Methods, Properties, and Class variables.
#[cfg(unix)]
fn categorize_members(
    members: &[MemberInfo],
) -> (Vec<&MemberInfo>, Vec<&MemberInfo>, Vec<&MemberInfo>) {
    let mut methods = Vec::new();
    let mut properties = Vec::new();
    let mut class_vars = Vec::new();

    for m in members {
        match m.kind {
            SymbolKind::Method | SymbolKind::Function | SymbolKind::Constructor => {
                methods.push(m);
            }
            SymbolKind::Property => {
                properties.push(m);
            }
            _ => {
                class_vars.push(m);
            }
        }
    }

    (methods, properties, class_vars)
}

/// Format members as human-readable text for a single class.
#[cfg(unix)]
fn format_members_human(result: &MembersResult, file_path: &str, s: Styler) -> String {
    let mut output = String::new();

    let class_line = result.class_line + 1;
    let class_col = result.class_column + 1;
    let _ = writeln!(
        output,
        "{} ({})",
        s.symbol(&result.class_name),
        s.file_location(file_path, class_line, class_col),
    );

    if result.members.is_empty() {
        let _ = writeln!(output, "  (no public members)");
        return output;
    }

    let (methods, properties, class_vars) = categorize_members(&result.members);

    if !methods.is_empty() {
        let _ = writeln!(output, "  {}:", s.heading("Methods"));
        for m in &methods {
            let sig = m.signature.as_deref().unwrap_or(&m.name);
            let line = m.line + 1;
            let col = m.column + 1;
            let loc = format!(":{line}:{col}");
            let _ = writeln!(output, "    {sig:<60} {}", s.line_col(&loc));
        }
    }

    if !properties.is_empty() {
        let _ = writeln!(output, "  {}:", s.heading("Properties"));
        for m in &properties {
            let sig = m.signature.as_deref().unwrap_or(&m.name);
            let line = m.line + 1;
            let col = m.column + 1;
            let loc = format!(":{line}:{col}");
            let _ = writeln!(output, "    {sig:<60} {}", s.line_col(&loc));
        }
    }

    if !class_vars.is_empty() {
        let _ = writeln!(output, "  {}:", s.heading("Class variables"));
        for m in &class_vars {
            let sig = m.signature.as_deref().unwrap_or(&m.name);
            let line = m.line + 1;
            let col = m.column + 1;
            let loc = format!(":{line}:{col}");
            let _ = writeln!(output, "    {sig:<60} {}", s.line_col(&loc));
        }
    }

    output
}

#[cfg(unix)]
impl OutputFormatter {
    /// Format a single class members result.
    pub fn format_members_result(&self, result: &MembersResult) -> String {
        let file_path = self.uri_to_path(&result.file_uri);

        match self.format {
            OutputFormat::Human => format_members_human(result, &file_path, self.s),
            OutputFormat::Json => {
                serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("class,member,kind,signature,line,column\n");
                for m in &result.members {
                    let sig = m.signature.as_deref().unwrap_or("");
                    let line = m.line + 1;
                    let col = m.column + 1;
                    let _ = writeln!(
                        output,
                        "{},{},{},\"{}\",{line},{col}",
                        result.class_name,
                        m.name,
                        Self::kind_label(&m.kind),
                        sig.replace('"', "\"\""),
                    );
                }
                output
            }
            OutputFormat::Paths => file_path,
        }
    }

    /// Format results for one or more class members queries.
    pub fn format_members_results(&self, results: &[MembersResult]) -> String {
        if results.len() == 1 {
            return self.format_members_result(&results[0]);
        }

        match self.format {
            OutputFormat::Human => {
                let mut output = String::new();
                for result in results {
                    output.push_str(&self.format_members_result(result));
                    output.push('\n');
                }
                output.trim_end().to_string()
            }
            OutputFormat::Json => {
                serde_json::to_string_pretty(results).unwrap_or_else(|_| "[]".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("class,member,kind,signature,line,column\n");
                for result in results {
                    let file_path = self.uri_to_path(&result.file_uri);
                    let _ = file_path; // included in class context
                    for m in &result.members {
                        let sig = m.signature.as_deref().unwrap_or("");
                        let line = m.line + 1;
                        let col = m.column + 1;
                        let _ = writeln!(
                            output,
                            "{},{},{},\"{}\",{line},{col}",
                            result.class_name,
                            m.name,
                            Self::kind_label(&m.kind),
                            sig.replace('"', "\"\""),
                        );
                    }
                }
                output
            }
            OutputFormat::Paths => {
                let mut paths: Vec<String> =
                    results.iter().map(|r| self.uri_to_path(&r.file_uri)).collect();
                paths.sort();
                paths.dedup();
                paths.join("\n")
            }
        }
    }
}

fn format_document_symbols_recursive(
    symbols: &[DocumentSymbol],
    indent: usize,
    output: &mut String,
) {
    for symbol in symbols {
        let line = symbol.range.start.line + 1;
        let column = symbol.range.start.character + 1;
        let indent_str = "  ".repeat(indent);

        let _ = writeln!(
            output,
            "{indent_str}{} ({:?}) - line {line}, col {column}",
            symbol.name, symbol.kind,
        );

        if let Some(children) = &symbol.children {
            format_document_symbols_recursive(children, indent + 1, output);
        }
    }
}

fn format_document_symbols_csv(symbols: &[DocumentSymbol], output: &mut String) {
    for symbol in symbols {
        let line = symbol.range.start.line + 1;
        let column = symbol.range.start.character + 1;

        let _ = writeln!(output, "{},{:?},{line},{column}", symbol.name, symbol.kind);

        if let Some(children) = &symbol.children {
            format_document_symbols_csv(children, output);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::protocol::{Position, Range, SymbolKind};

    fn make_location(uri: &str, line: u32, character: u32) -> Location {
        Location {
            uri: uri.to_string(),
            range: Range {
                start: Position { line, character },
                end: Position { line, character: character + 5 },
            },
        }
    }

    #[test]
    fn test_format_definitions_empty() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let result = formatter.format_definitions(&[], "test:1:1", &SourceCache::new());
        assert_eq!(result, "No results found for: test:1:1");
    }

    #[test]
    fn test_format_definitions_single() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let locations = [make_location("file:///nonexistent.py", 5, 10)];
        let result = formatter.format_definitions(&locations, "test:6:11", &SourceCache::new());

        assert!(result.contains("Found 1 definition(s)"));
        assert!(result.contains("nonexistent.py:6:11"));
    }

    #[test]
    fn test_format_definitions_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let locations = [make_location("file:///test.py", 0, 0)];
        let result = formatter.format_definitions(&locations, "test", &SourceCache::new());

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["uri"], "file:///test.py");
    }

    #[test]
    fn test_format_definitions_csv() {
        let formatter = OutputFormatter::new(OutputFormat::Csv);
        let locations = [make_location("file:///test.py", 4, 2)];
        let result = formatter.format_definitions(&locations, "test", &SourceCache::new());

        assert!(result.starts_with("file,line,column\n"));
        assert!(result.contains("5,3")); // 0-based -> 1-based
    }

    #[test]
    fn test_format_find_results_single_symbol() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let locations = vec![make_location("file:///test.py", 0, 0)];
        let results = vec![("foo".to_string(), locations)];
        let result = formatter.format_find_results(&results, &SourceCache::new());

        assert!(result.contains("Found 1 definition(s) for: 'foo'"));
    }

    #[test]
    fn test_format_find_results_symbol_not_found() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let results = vec![("missing".to_string(), vec![])];
        let result = formatter.format_find_results(&results, &SourceCache::new());

        assert_eq!(result, "No results found for: 'missing'");
    }

    #[test]
    fn test_format_find_results_multiple_symbols() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let results = vec![
            ("foo".to_string(), vec![make_location("file:///test.py", 0, 0)]),
            ("bar".to_string(), vec![]),
        ];
        let result = formatter.format_find_results(&results, &SourceCache::new());

        assert!(result.contains("=== foo ==="));
        assert!(!result.contains("=== bar ==="), "empty symbol should not get a heading");
        assert!(result.contains("No results found for: 'bar'"));
    }

    #[test]
    fn test_format_enriched_references_empty() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let result = EnrichedReferencesResult {
            label: "test:1:1".to_string(),
            total_count: 0,
            displayed: Vec::new(),
            remaining_count: 0,
            test_references: None,
        };
        let output = formatter.format_enriched_references_results(&[result], &SourceCache::new());
        assert_eq!(output, "No results found for: 'test:1:1'");
    }

    #[test]
    fn test_format_workspace_symbols() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let symbols = vec![SymbolInformation {
            name: "MyClass".to_string(),
            kind: SymbolKind::Class,
            tags: None,
            deprecated: None,
            location: make_location("file:///test.py", 0, 0),
            container_name: None,
        }];
        let result = formatter.format_workspace_symbols(&symbols);

        assert!(result.contains("MyClass"));
        assert!(result.contains("Class"));
    }

    #[test]
    fn test_uri_to_path_with_file_prefix() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let result = formatter.uri_to_path("file:///some/path/test.py");
        // Should strip the file:// prefix
        assert!(!result.starts_with("file://"));
        assert!(result.contains("test.py"));
    }

    #[test]
    fn test_uri_to_path_without_file_prefix() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let result = formatter.uri_to_path("https://example.com");
        assert_eq!(result, "https://example.com");
    }

    fn make_entry<'a>(
        symbol: &'a str,
        kind: Option<&'a SymbolKind>,
        definitions: &'a [Location],
        hover: Option<&'a Hover>,
    ) -> ShowEntry<'a> {
        ShowEntry {
            symbol,
            kind,
            definitions,
            hover,
            total_reference_count: 0,
            total_reference_files: 0,
            displayed_references: Vec::new(),
            remaining_reference_count: 0,
            show_individual_refs: false,
            show_doc: false,
            test_references: None,
        }
    }

    #[test]
    fn test_format_show_condensed_empty() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let entry = make_entry("missing", None, &[], None);
        let result = formatter.format_show(&entry, &SourceCache::new());

        // When all sections are empty, should show a single "no results" line
        assert_eq!(result, "No results found for: 'missing'");
    }

    #[test]
    fn test_format_show_refs_zero_count_condensed() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = ShowEntry {
            symbol: "Animal",
            kind: Some(&SymbolKind::Class),
            definitions: &defs,
            hover: None,
            total_reference_count: 0,
            total_reference_files: 0,
            displayed_references: Vec::new(),
            remaining_reference_count: 0,
            show_individual_refs: false,
            show_doc: false,
            test_references: None,
        };
        let result = formatter.format_show(&entry, &SourceCache::new());

        // Zero refs should show "Refs: none"
        assert!(
            result.contains("# Refs: none"),
            "should show 'Refs: none' for zero references, got:\n{result}"
        );
    }

    #[test]
    fn test_format_show_refs_count_without_individual_full() {
        let formatter = OutputFormatter::with_detail(
            OutputFormat::Human,
            OutputDetail::Full,
            Styler::no_color(),
        );
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = ShowEntry {
            symbol: "Animal",
            kind: Some(&SymbolKind::Class),
            definitions: &defs,
            hover: None,
            total_reference_count: 5,
            total_reference_files: 2,
            displayed_references: Vec::new(),
            remaining_reference_count: 0,
            show_individual_refs: false,
            show_doc: false,
            test_references: None,
        };
        let result = formatter.format_show(&entry, &SourceCache::new());

        // Should show count summary but no individual refs
        assert!(
            result.contains("5 reference(s) across 2 file(s)"),
            "should show reference count summary, got:\n{result}"
        );
    }

    #[test]
    fn test_format_show_condensed_with_kind() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = make_entry("foo", Some(&SymbolKind::Function), &defs, None);
        let result = formatter.format_show(&entry, &SourceCache::new());

        assert!(result.contains("# Definition (func)"));
        assert!(result.contains("test.py:1:1"));
    }

    #[test]
    fn test_format_show_full_empty() {
        let formatter = OutputFormatter::with_detail(
            OutputFormat::Human,
            OutputDetail::Full,
            Styler::no_color(),
        );
        let entry = make_entry("missing", None, &[], None);
        let result = formatter.format_show(&entry, &SourceCache::new());

        // When all sections are empty, should show a single "no results" line
        assert_eq!(result, "No results found for: 'missing'");
    }

    #[test]
    fn test_format_show_condensed_with_defs() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = make_entry("foo", None, &defs, None);
        let result = formatter.format_show(&entry, &SourceCache::new());

        // Should show path:line:col on one line (no numbering)
        assert!(result.contains("test.py:1:1"));
        assert!(result.contains("# Definition"));
        // No symbol name header for single symbol in condensed
        assert!(!result.contains("foo"));
    }

    #[test]
    fn test_format_show_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = make_entry("foo", Some(&SymbolKind::Function), &defs, None);
        let result = formatter.format_show(&entry, &SourceCache::new());

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["symbol"], "foo");
        assert_eq!(parsed["kind"], "func");
        assert!(parsed["definitions"].is_array());
    }

    #[test]
    fn test_read_source_line_valid() {
        let path = "/tmp/test_source_line.py";
        let content = "line 1\n  line 2\nline 3\n";
        let cache = SourceCache::from_entries([(path.to_string(), content.to_string())]);

        assert_eq!(read_source_line(&cache, path, 1), Some("line 1".to_string()));
        assert_eq!(read_source_line(&cache, path, 2), Some("line 2".to_string()));
        assert_eq!(read_source_line(&cache, path, 3), Some("line 3".to_string()));
        assert_eq!(read_source_line(&cache, path, 4), None);
    }

    #[test]
    fn test_read_source_line_nonexistent_file() {
        let cache = SourceCache::new();
        assert_eq!(read_source_line(&cache, "/nonexistent/file.py", 1), None);
    }

    #[test]
    fn test_extract_hover_type_strips_docstring() {
        use crate::lsp::protocol::{HoverContents, MarkupContent};

        let contents = HoverContents::Markup(MarkupContent {
            kind: crate::lsp::protocol::MarkupKind::Markdown,
            value: "```xml\n<class 'Animal'>\n```\n---\nBase class for animals.".to_string(),
        });

        let result = OutputFormatter::extract_hover_type(&contents);
        // Should strip docstring after ---
        assert!(!result.contains("Base class"));
        assert!(!result.contains("---"));
        // Should strip code fences
        assert!(!result.contains("```"), "should not contain code fences, got: {result}");
        assert_eq!(result, "<class 'Animal'>");
    }

    #[test]
    fn test_extract_hover_type_no_docstring() {
        use crate::lsp::protocol::{HoverContents, MarkupContent};

        let contents = HoverContents::Markup(MarkupContent {
            kind: crate::lsp::protocol::MarkupKind::Markdown,
            value: "```python\ndef hello_world() -> Unknown\n```".to_string(),
        });

        let result = OutputFormatter::extract_hover_type(&contents);
        assert!(result.contains("def hello_world() -> Unknown"));
        // No xml tag to replace
        assert!(!result.contains("xml"));
    }

    #[test]
    fn test_extract_hover_doc() {
        use crate::lsp::protocol::{HoverContents, MarkupContent};

        let with_doc = HoverContents::Markup(MarkupContent {
            kind: crate::lsp::protocol::MarkupKind::Markdown,
            value: "```xml\n<class 'Animal'>\n```\n---\nBase class for animals.".to_string(),
        });
        assert_eq!(
            OutputFormatter::extract_hover_doc(&with_doc),
            Some("Base class for animals.".to_string())
        );

        let without_doc = HoverContents::Markup(MarkupContent {
            kind: crate::lsp::protocol::MarkupKind::Markdown,
            value: "```python\ndef foo() -> int\n```".to_string(),
        });
        assert_eq!(OutputFormatter::extract_hover_doc(&without_doc), None);
    }

    #[test]
    fn test_condensed_show_separates_doc() {
        use crate::lsp::protocol::{Hover, HoverContents, MarkupContent, Range};

        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location("file:///test.py", 3, 6)];
        let hover = Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: crate::lsp::protocol::MarkupKind::Markdown,
                value: "```xml\n<class 'Animal'>\n```\n---\nBase class for animals.".to_string(),
            }),
            range: Some(Range {
                start: crate::lsp::protocol::Position { line: 3, character: 6 },
                end: crate::lsp::protocol::Position { line: 3, character: 12 },
            }),
        };
        let entry = make_entry("Animal", Some(&SymbolKind::Class), &defs, Some(&hover));
        let result = formatter.format_show(&entry, &SourceCache::new());

        // Signature section should have the class type without code fences or docstring
        assert!(
            result.contains("# Signature\n<class 'Animal'>"),
            "signature section should contain bare type, got:\n{result}"
        );
        assert!(!result.contains("```"), "should not contain code fences, got:\n{result}");
        // Doc section should NOT appear by default (show_doc is false)
        assert!(
            !result.contains("# Doc"),
            "doc section should not appear by default, got:\n{result}"
        );
        // No raw --- separator in output
        assert!(!result.contains("\n---\n"));
    }

    #[test]
    fn test_extract_hover_type_strips_code_fences() {
        use crate::lsp::protocol::{HoverContents, MarkupContent};

        let contents = HoverContents::Markup(MarkupContent {
            kind: crate::lsp::protocol::MarkupKind::Markdown,
            value: "```python\ndef hello_world() -> str\n```".to_string(),
        });

        let result = OutputFormatter::extract_hover_type(&contents);
        // Should NOT contain backtick fences
        assert!(!result.contains("```"), "type should not contain code fences, got: {result}");
        assert_eq!(result, "def hello_world() -> str");
    }

    #[test]
    fn test_extract_hover_type_strips_xml_fences() {
        use crate::lsp::protocol::{HoverContents, MarkupContent};

        let contents = HoverContents::Markup(MarkupContent {
            kind: crate::lsp::protocol::MarkupKind::Markdown,
            value: "```xml\n<class 'Animal'>\n```\n---\nBase class for animals.".to_string(),
        });

        let result = OutputFormatter::extract_hover_type(&contents);
        assert!(!result.contains("```"), "type should not contain code fences, got: {result}");
        assert_eq!(result, "<class 'Animal'>");
    }

    #[test]
    fn test_read_decorators_single() {
        let content = "@dataclass\nclass Config:\n    host: str\n";
        let path = "/tmp/test_decorators_single.py";
        let cache = SourceCache::from_entries([(path.to_string(), content.to_string())]);

        let result = read_decorators(&cache, path, 0);
        assert_eq!(result, Some("@dataclass".to_string()));
    }

    #[test]
    fn test_read_decorators_multiple() {
        let content = "@some_decorator\n@another_decorator\ndef my_func():\n    pass\n";
        let path = "/tmp/test_decorators_multiple.py";
        let cache = SourceCache::from_entries([(path.to_string(), content.to_string())]);

        let result = read_decorators(&cache, path, 0);
        assert_eq!(result, Some("@some_decorator\n@another_decorator".to_string()));
    }

    #[test]
    fn test_read_decorators_none() {
        let content = "class Config:\n    host: str\n";
        let path = "/tmp/test_decorators_none.py";
        let cache = SourceCache::from_entries([(path.to_string(), content.to_string())]);

        let result = read_decorators(&cache, path, 0);
        assert_eq!(result, None);
    }

    #[test]
    fn test_read_decorators_nonexistent_file() {
        let cache = SourceCache::new();
        assert_eq!(read_decorators(&cache, "/nonexistent/file.py", 0), None);
    }

    #[test]
    fn test_condensed_show_shows_decorators() {
        use crate::lsp::protocol::{Hover, HoverContents, MarkupContent, Range};

        let content = "@dataclass\nclass Config:\n    host: str\n";
        let file_path = "/tmp/test_condensed_decorators.py";
        let file_uri = format!("file://{file_path}");
        let cache = SourceCache::from_entries([(file_path.to_string(), content.to_string())]);

        let formatter = OutputFormatter::new(OutputFormat::Human);
        // Definition starts at the decorator line (line 0)
        let defs = [make_location(&file_uri, 0, 0)];
        let hover = Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: crate::lsp::protocol::MarkupKind::Markdown,
                value: "```xml\n<class 'Config'>\n```".to_string(),
            }),
            range: Some(Range {
                start: crate::lsp::protocol::Position { line: 1, character: 6 },
                end: crate::lsp::protocol::Position { line: 1, character: 12 },
            }),
        };
        let entry = make_entry("Config", Some(&SymbolKind::Class), &defs, Some(&hover));
        let result = formatter.format_show(&entry, &cache);

        // Type section should show decorator + source class definition
        assert!(
            result.contains("@dataclass\nclass Config:"),
            "should show decorator and class definition in type section, got:\n{result}"
        );
        // No code fences
        assert!(!result.contains("```"), "should not contain code fences, got:\n{result}");
    }

    #[test]
    fn test_condensed_show_no_decorator_no_crash() {
        use crate::lsp::protocol::{Hover, HoverContents, MarkupContent, Range};

        let content = "class Animal:\n    pass\n";
        let file_path = "/tmp/test_condensed_no_decorator.py";
        let file_uri = format!("file://{file_path}");
        let cache = SourceCache::from_entries([(file_path.to_string(), content.to_string())]);

        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location(&file_uri, 0, 0)];
        let hover = Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: crate::lsp::protocol::MarkupKind::Markdown,
                value: "```xml\n<class 'Animal'>\n```".to_string(),
            }),
            range: Some(Range {
                start: crate::lsp::protocol::Position { line: 0, character: 6 },
                end: crate::lsp::protocol::Position { line: 0, character: 12 },
            }),
        };
        let entry = make_entry("Animal", Some(&SymbolKind::Class), &defs, Some(&hover));
        let result = formatter.format_show(&entry, &cache);

        // Should show source class line (no decorator, no inheritance)
        assert!(
            result.contains("# Signature\nclass Animal:"),
            "should show source class definition, got:\n{result}"
        );
    }

    #[test]
    fn test_read_definition_line_class() {
        let path = "/tmp/test_def_line_class.py";
        let content = "class Dog(Animal):\n    pass\n";
        let cache = SourceCache::from_entries([(path.to_string(), content.to_string())]);

        let result = read_definition_line(&cache, path, 0);
        assert_eq!(result, Some("class Dog(Animal):".to_string()));
    }

    #[test]
    fn test_read_definition_line_with_decorators() {
        let path = "/tmp/test_def_line_decorators.py";
        let content = "@dataclass\nclass Config(Base):\n    host: str\n";
        let cache = SourceCache::from_entries([(path.to_string(), content.to_string())]);

        // Starting at decorator line, should skip decorators and return class line
        let result = read_definition_line(&cache, path, 0);
        assert_eq!(result, Some("class Config(Base):".to_string()));
    }

    #[test]
    fn test_read_definition_line_no_inheritance() {
        let path = "/tmp/test_def_line_no_inherit.py";
        let content = "class Animal:\n    pass\n";
        let cache = SourceCache::from_entries([(path.to_string(), content.to_string())]);

        let result = read_definition_line(&cache, path, 0);
        assert_eq!(result, Some("class Animal:".to_string()));
    }

    #[test]
    fn test_condensed_show_class_shows_source_definition() {
        use crate::lsp::protocol::{Hover, HoverContents, MarkupContent, Range};

        let content = "class Dog(Animal):\n    pass\n";
        let file_path = "/tmp/test_class_source_def.py";
        let file_uri = format!("file://{file_path}");
        let cache = SourceCache::from_entries([(file_path.to_string(), content.to_string())]);

        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location(&file_uri, 0, 6)];
        let hover = Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: crate::lsp::protocol::MarkupKind::Markdown,
                value: "```xml\n<class 'Dog'>\n```\n---\nA dog.".to_string(),
            }),
            range: Some(Range {
                start: crate::lsp::protocol::Position { line: 0, character: 6 },
                end: crate::lsp::protocol::Position { line: 0, character: 9 },
            }),
        };
        let entry = make_entry("Dog", Some(&SymbolKind::Class), &defs, Some(&hover));
        let result = formatter.format_show(&entry, &cache);

        // Type section should show source class line with inheritance, not <class 'Dog'>
        assert!(
            result.contains("class Dog(Animal):"),
            "should show class definition with base class, got:\n{result}"
        );
        assert!(
            !result.contains("<class 'Dog'>"),
            "should NOT show bare <class> tag, got:\n{result}"
        );
    }

    #[test]
    fn test_condensed_show_decorated_class_with_inheritance() {
        use crate::lsp::protocol::{Hover, HoverContents, MarkupContent, Range};

        let content = "@dataclass\nclass AppConfig(Config):\n    debug: bool\n";
        let file_path = "/tmp/test_decorated_class_inherit.py";
        let file_uri = format!("file://{file_path}");
        let cache = SourceCache::from_entries([(file_path.to_string(), content.to_string())]);

        let formatter = OutputFormatter::new(OutputFormat::Human);
        // Definition starts at the decorator line (line 0)
        let defs = [make_location(&file_uri, 0, 0)];
        let hover = Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: crate::lsp::protocol::MarkupKind::Markdown,
                value: "```xml\n<class 'AppConfig'>\n```".to_string(),
            }),
            range: Some(Range {
                start: crate::lsp::protocol::Position { line: 1, character: 6 },
                end: crate::lsp::protocol::Position { line: 1, character: 15 },
            }),
        };
        let entry = make_entry("AppConfig", Some(&SymbolKind::Class), &defs, Some(&hover));
        let result = formatter.format_show(&entry, &cache);

        // Should show decorator + source class line with inheritance
        assert!(
            result.contains("@dataclass\nclass AppConfig(Config):"),
            "should show decorator and class definition, got:\n{result}"
        );
    }

    #[test]
    fn test_condensed_show_function_keeps_hover_type() {
        use crate::lsp::protocol::{Hover, HoverContents, MarkupContent, Range};

        let content = "def hello_world():\n    return 'hi'\n";
        let file_path = "/tmp/test_func_hover_type.py";
        let file_uri = format!("file://{file_path}");
        let cache = SourceCache::from_entries([(file_path.to_string(), content.to_string())]);

        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location(&file_uri, 0, 4)];
        let hover = Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: crate::lsp::protocol::MarkupKind::Markdown,
                value: "```python\ndef hello_world() -> str\n```".to_string(),
            }),
            range: Some(Range {
                start: crate::lsp::protocol::Position { line: 0, character: 4 },
                end: crate::lsp::protocol::Position { line: 0, character: 15 },
            }),
        };
        let entry = make_entry("hello_world", Some(&SymbolKind::Function), &defs, Some(&hover));
        let result = formatter.format_show(&entry, &cache);

        // Functions should still show the hover type (has inferred return type)
        assert!(
            result.contains("def hello_world() -> str"),
            "function should show hover type with inferred return type, got:\n{result}"
        );
    }

    #[cfg(unix)]
    pub(super) mod members_tests {
        use super::*;
        use crate::daemon::protocol::{MemberInfo, MembersResult};

        pub(super) fn make_members_result() -> MembersResult {
            MembersResult {
                class_name: "Animal".to_string(),
                file_uri: "file:///src/models.py".to_string(),
                class_line: 4,
                class_column: 0,
                symbol_kind: Some(SymbolKind::Class),
                members: vec![
                    MemberInfo {
                        name: "speak".to_string(),
                        kind: SymbolKind::Method,
                        signature: Some("speak(self) -> str".to_string()),
                        line: 10,
                        column: 4,
                    },
                    MemberInfo {
                        name: "name".to_string(),
                        kind: SymbolKind::Property,
                        signature: Some("name: str".to_string()),
                        line: 7,
                        column: 4,
                    },
                    MemberInfo {
                        name: "MAX_LEGS".to_string(),
                        kind: SymbolKind::Variable,
                        signature: Some("MAX_LEGS: int".to_string()),
                        line: 5,
                        column: 4,
                    },
                ],
            }
        }

        #[test]
        fn test_format_members_human() {
            let formatter = OutputFormatter::new(OutputFormat::Human);
            let result = make_members_result();
            let output = formatter.format_members_result(&result);

            assert!(output.contains("Animal"), "should show class name");
            assert!(output.contains(":5:1"), "should show class location (1-based)");
            assert!(output.contains("Methods:"), "should have Methods section");
            assert!(output.contains("speak(self) -> str"), "should show method sig");
            assert!(output.contains("Properties:"), "should have Properties section");
            assert!(output.contains("name: str"), "should show property sig");
            assert!(output.contains("Class variables:"), "should have Class variables section");
            assert!(output.contains("MAX_LEGS: int"), "should show class var sig");
        }

        #[test]
        fn test_format_members_json() {
            let formatter = OutputFormatter::new(OutputFormat::Json);
            let result = make_members_result();
            let output = formatter.format_members_result(&result);

            let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
            assert_eq!(parsed["class_name"], "Animal");
            assert!(parsed["members"].is_array());
            assert_eq!(parsed["members"].as_array().unwrap().len(), 3);
        }

        #[test]
        fn test_format_members_csv() {
            let formatter = OutputFormatter::new(OutputFormat::Csv);
            let result = make_members_result();
            let output = formatter.format_members_result(&result);

            assert!(output.starts_with("class,member,kind,signature,line,column\n"));
            assert!(output.contains("Animal,speak,method"));
            assert!(output.contains("Animal,name,prop"));
            assert!(output.contains("Animal,MAX_LEGS,var"));
        }

        #[test]
        fn test_format_members_paths() {
            let formatter = OutputFormatter::new(OutputFormat::Paths);
            let result = make_members_result();
            let output = formatter.format_members_result(&result);

            assert!(output.contains("models.py"));
        }

        #[test]
        fn test_format_members_empty_class() {
            let formatter = OutputFormatter::new(OutputFormat::Human);
            let result = MembersResult {
                class_name: "Empty".to_string(),
                file_uri: "file:///empty.py".to_string(),
                class_line: 0,
                class_column: 0,
                symbol_kind: Some(SymbolKind::Class),
                members: Vec::new(),
            };
            let output = formatter.format_members_result(&result);

            assert!(output.contains("Empty"));
            assert!(output.contains("(no public members)"));
        }

        #[test]
        fn test_format_members_multiple_classes() {
            let formatter = OutputFormatter::new(OutputFormat::Human);
            let results = vec![
                make_members_result(),
                MembersResult {
                    class_name: "Dog".to_string(),
                    file_uri: "file:///src/models.py".to_string(),
                    class_line: 20,
                    class_column: 0,
                    symbol_kind: Some(SymbolKind::Class),
                    members: vec![MemberInfo {
                        name: "fetch".to_string(),
                        kind: SymbolKind::Method,
                        signature: Some("fetch(self, item: str) -> str".to_string()),
                        line: 25,
                        column: 4,
                    }],
                },
            ];
            let output = formatter.format_members_results(&results);

            assert!(output.contains("Animal"), "should show first class");
            assert!(output.contains("Dog"), "should show second class");
            assert!(output.contains("fetch(self, item: str) -> str"));
        }
    }

    // ── Enclosing symbol tree walk tests ───────────────────────────────

    fn make_doc_symbol(
        name: &str,
        kind: SymbolKind,
        start_line: u32,
        end_line: u32,
        children: Option<Vec<DocumentSymbol>>,
    ) -> DocumentSymbol {
        DocumentSymbol {
            name: name.to_string(),
            detail: None,
            kind,
            tags: None,
            deprecated: None,
            range: Range {
                start: Position { line: start_line, character: 0 },
                end: Position { line: end_line, character: 0 },
            },
            selection_range: Range {
                start: Position { line: start_line, character: 0 },
                #[allow(clippy::cast_possible_truncation)]
                end: Position { line: start_line, character: name.len() as u32 },
            },
            children,
        }
    }

    #[test]
    fn test_find_enclosing_symbol_top_level_function() {
        let symbols = vec![make_doc_symbol("my_func", SymbolKind::Function, 5, 15, None)];

        assert_eq!(find_enclosing_symbol(&symbols, 10, 4), Some("my_func".to_string()));
    }

    #[test]
    fn test_find_enclosing_symbol_nested_method() {
        let method = make_doc_symbol("process", SymbolKind::Method, 10, 20, None);
        let class = make_doc_symbol("RequestHandler", SymbolKind::Class, 5, 30, Some(vec![method]));
        let symbols = vec![class];

        assert_eq!(
            find_enclosing_symbol(&symbols, 15, 8),
            Some("RequestHandler.process".to_string())
        );
    }

    #[test]
    fn test_find_enclosing_symbol_module_scope() {
        let symbols = vec![make_doc_symbol("my_func", SymbolKind::Function, 5, 15, None)];

        // Position outside any symbol → module scope (None)
        assert_eq!(find_enclosing_symbol(&symbols, 2, 0), None);
    }

    #[test]
    fn test_find_enclosing_symbol_empty_tree() {
        assert_eq!(find_enclosing_symbol(&[], 10, 5), None);
    }

    #[test]
    fn test_find_enclosing_symbol_class_level_not_in_method() {
        let method = make_doc_symbol("process", SymbolKind::Method, 10, 20, None);
        let class = make_doc_symbol("RequestHandler", SymbolKind::Class, 5, 30, Some(vec![method]));
        let symbols = vec![class];

        // Position in class but outside the method
        assert_eq!(find_enclosing_symbol(&symbols, 7, 0), Some("RequestHandler".to_string()));
    }

    #[test]
    fn test_find_enclosing_symbol_deeply_nested() {
        let inner_method = make_doc_symbol("inner_method", SymbolKind::Method, 15, 18, None);
        let inner_class =
            make_doc_symbol("InnerClass", SymbolKind::Class, 12, 20, Some(vec![inner_method]));
        let outer_class =
            make_doc_symbol("OuterClass", SymbolKind::Class, 5, 30, Some(vec![inner_class]));
        let symbols = vec![outer_class];

        assert_eq!(
            find_enclosing_symbol(&symbols, 16, 4),
            Some("OuterClass.InnerClass.inner_method".to_string())
        );
    }

    // ── Enriched show output tests ──────────────────────────────────

    #[test]
    fn test_format_show_with_ref_count_condensed() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = ShowEntry {
            symbol: "my_func",
            kind: Some(&SymbolKind::Function),
            definitions: &defs,
            hover: None,
            total_reference_count: 47,
            total_reference_files: 12,
            displayed_references: Vec::new(),
            remaining_reference_count: 0,
            show_individual_refs: false,
            show_doc: false,
            test_references: None,
        };
        let result = formatter.format_show(&entry, &SourceCache::new());

        assert!(
            result.contains("# Refs: 47 across 12 file(s)"),
            "should show reference count summary, got:\n{result}"
        );
    }

    #[test]
    fn test_format_show_with_enriched_refs_condensed() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location("file:///test.py", 0, 0)];
        let enriched = vec![
            EnrichedReference {
                location: make_location("file:///src/main.py", 44, 11),
                context: "RequestHandler.process".to_string(),
            },
            EnrichedReference {
                location: make_location("file:///src/main.py", 2, 0),
                context: "module scope".to_string(),
            },
        ];
        let entry = ShowEntry {
            symbol: "my_func",
            kind: Some(&SymbolKind::Function),
            definitions: &defs,
            hover: None,
            total_reference_count: 5,
            total_reference_files: 2,
            displayed_references: enriched,
            remaining_reference_count: 3,
            show_individual_refs: true,
            show_doc: false,
            test_references: None,
        };
        let result = formatter.format_show(&entry, &SourceCache::new());

        assert!(result.contains("# Refs: 5 across 2 file(s)"), "should show count, got:\n{result}");
        assert!(result.contains("(RequestHandler.process)"), "should show context, got:\n{result}");
        assert!(
            result.contains("(module scope)"),
            "should show module scope context, got:\n{result}"
        );
        assert!(result.contains("... and 3 more"), "should show remaining count, got:\n{result}");
    }

    #[test]
    fn test_format_show_json_includes_context() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let defs = [make_location("file:///test.py", 0, 0)];
        let enriched = vec![EnrichedReference {
            location: make_location("file:///src/main.py", 44, 11),
            context: "RequestHandler.process".to_string(),
        }];
        let entry = ShowEntry {
            symbol: "my_func",
            kind: Some(&SymbolKind::Function),
            definitions: &defs,
            hover: None,
            total_reference_count: 1,
            total_reference_files: 1,
            displayed_references: enriched,
            remaining_reference_count: 0,
            show_individual_refs: true,
            show_doc: false,
            test_references: None,
        };
        let result = formatter.format_show(&entry, &SourceCache::new());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["reference_count"], 1);
        assert_eq!(parsed["reference_files"], 1);
        assert_eq!(parsed["references"][0]["context"], "RequestHandler.process");
    }

    #[test]
    fn test_format_enriched_references_with_limit() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let result = EnrichedReferencesResult {
            label: "my_func".to_string(),
            total_count: 50,
            displayed: vec![EnrichedReference {
                location: make_location("file:///src/main.py", 10, 5),
                context: "Handler.process".to_string(),
            }],
            remaining_count: 49,
            test_references: None,
        };
        let output = formatter.format_enriched_references_results(&[result], &SourceCache::new());

        assert!(
            output.contains("Found 50 reference(s)"),
            "should show total count, got:\n{output}"
        );
        assert!(output.contains("(Handler.process)"), "should show context, got:\n{output}");
        assert!(output.contains("... and 49 more"), "should show remaining, got:\n{output}");
    }

    #[test]
    fn test_format_enriched_references_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let result = EnrichedReferencesResult {
            label: "my_func".to_string(),
            total_count: 2,
            displayed: vec![EnrichedReference {
                location: make_location("file:///src/main.py", 10, 5),
                context: "Handler.process".to_string(),
            }],
            remaining_count: 1,
            test_references: None,
        };
        let output = formatter.format_enriched_references_results(&[result], &SourceCache::new());
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["reference_count"], 2);
        assert_eq!(parsed["references"][0]["context"], "Handler.process");
    }

    #[test]
    fn test_format_enriched_references_limit_zero_shows_all() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        // When limit is 0, remaining_count should be 0 (all displayed)
        let result = EnrichedReferencesResult {
            label: "my_func".to_string(),
            total_count: 3,
            displayed: vec![
                EnrichedReference {
                    location: make_location("file:///a.py", 1, 0),
                    context: "module scope".to_string(),
                },
                EnrichedReference {
                    location: make_location("file:///b.py", 2, 0),
                    context: "foo".to_string(),
                },
                EnrichedReference {
                    location: make_location("file:///c.py", 3, 0),
                    context: "bar".to_string(),
                },
            ],
            remaining_count: 0,
            test_references: None,
        };
        let output = formatter.format_enriched_references_results(&[result], &SourceCache::new());

        assert!(
            !output.contains("... and"),
            "should not show truncation message when all displayed, got:\n{output}"
        );
        assert!(output.contains("Found 3 reference(s)"), "should show total count, got:\n{output}");
    }

    // ── Test references formatting tests ─────────────────────────────

    #[test]
    fn test_format_enriched_refs_hides_tests_by_default() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let result = EnrichedReferencesResult {
            label: "my_func".to_string(),
            total_count: 2,
            displayed: vec![EnrichedReference {
                location: make_location("file:///project/src/main.py", 5, 0),
                context: "module scope".to_string(),
            }],
            remaining_count: 1,
            test_references: Some(TestReferencesSection {
                total_count: 3,
                displayed: Vec::new(),
                remaining_count: 0,
            }),
        };
        let output = formatter.format_enriched_references_results(&[result], &SourceCache::new());
        assert!(
            output.contains("Test references: 3 (use --tests/-t to show)"),
            "should show test refs heading with count, got:\n{output}"
        );
    }

    #[test]
    fn test_format_enriched_refs_shows_tests_when_enabled() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let result = EnrichedReferencesResult {
            label: "my_func".to_string(),
            total_count: 1,
            displayed: vec![EnrichedReference {
                location: make_location("file:///project/src/main.py", 5, 0),
                context: "module scope".to_string(),
            }],
            remaining_count: 0,
            test_references: Some(TestReferencesSection {
                total_count: 1,
                displayed: vec![EnrichedReference {
                    location: make_location("file:///project/tests/test_main.py", 3, 0),
                    context: "test_my_func".to_string(),
                }],
                remaining_count: 0,
            }),
        };
        let output = formatter.format_enriched_references_results(&[result], &SourceCache::new());
        assert!(
            output.contains("Test references (1):"),
            "should show test references section, got:\n{output}"
        );
        assert!(output.contains("test_main.py"), "should show test file, got:\n{output}");
    }

    #[test]
    fn test_format_enriched_refs_json_has_test_fields() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let result = EnrichedReferencesResult {
            label: "my_func".to_string(),
            total_count: 1,
            displayed: vec![EnrichedReference {
                location: make_location("file:///project/src/main.py", 5, 0),
                context: "module scope".to_string(),
            }],
            remaining_count: 0,
            test_references: None,
        };
        let output = formatter.format_enriched_references_results(&[result], &SourceCache::new());
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["test_reference_count"], 0);
        assert!(parsed["test_references"].is_array());
    }

    #[test]
    fn test_format_enriched_refs_json_with_test_refs() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let result = EnrichedReferencesResult {
            label: "my_func".to_string(),
            total_count: 1,
            displayed: vec![EnrichedReference {
                location: make_location("file:///project/src/main.py", 5, 0),
                context: "module scope".to_string(),
            }],
            remaining_count: 0,
            test_references: Some(TestReferencesSection {
                total_count: 2,
                displayed: vec![EnrichedReference {
                    location: make_location("file:///project/tests/test_main.py", 3, 0),
                    context: "test_my_func".to_string(),
                }],
                remaining_count: 1,
            }),
        };
        let output = formatter.format_enriched_references_results(&[result], &SourceCache::new());
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["test_reference_count"], 2);
        assert_eq!(parsed["test_references"][0]["context"], "test_my_func");
    }

    #[test]
    fn test_format_enriched_refs_csv_has_test_column() {
        let formatter = OutputFormatter::new(OutputFormat::Csv);
        let result = EnrichedReferencesResult {
            label: "my_func".to_string(),
            total_count: 1,
            displayed: vec![EnrichedReference {
                location: make_location("file:///project/src/main.py", 5, 0),
                context: "module scope".to_string(),
            }],
            remaining_count: 0,
            test_references: Some(TestReferencesSection {
                total_count: 1,
                displayed: vec![EnrichedReference {
                    location: make_location("file:///project/tests/test_main.py", 3, 0),
                    context: "test_func".to_string(),
                }],
                remaining_count: 0,
            }),
        };
        let output = formatter.format_enriched_references_results(&[result], &SourceCache::new());
        assert!(output.contains(",test\n"), "should have test column header, got:\n{output}");
        assert!(output.contains(",false\n"), "should have false for non-test, got:\n{output}");
        assert!(output.contains(",true\n"), "should have true for test, got:\n{output}");
    }

    #[test]
    fn test_format_enriched_refs_no_test_refs_no_hint() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let result = EnrichedReferencesResult {
            label: "my_func".to_string(),
            total_count: 1,
            displayed: vec![EnrichedReference {
                location: make_location("file:///project/src/main.py", 5, 0),
                context: "module scope".to_string(),
            }],
            remaining_count: 0,
            test_references: None,
        };
        let output = formatter.format_enriched_references_results(&[result], &SourceCache::new());
        assert!(
            !output.contains("test reference"),
            "should not mention test refs when none exist, got:\n{output}"
        );
    }

    #[test]
    fn test_format_show_condensed_with_test_refs_hint() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = ShowEntry {
            symbol: "my_func",
            kind: Some(&SymbolKind::Function),
            definitions: &defs,
            hover: None,
            total_reference_count: 2,
            total_reference_files: 1,
            displayed_references: Vec::new(),
            remaining_reference_count: 0,
            show_individual_refs: false,
            show_doc: false,
            test_references: Some(TestReferencesSection {
                total_count: 5,
                displayed: Vec::new(),
                remaining_count: 0,
            }),
        };
        let result = formatter.format_show(&entry, &SourceCache::new());
        assert!(
            result.contains("Test Refs: 5 (use --tests/-t to show)"),
            "should show test refs heading with count, got:\n{result}"
        );
    }

    #[test]
    fn test_format_show_full_with_test_refs_hint() {
        let formatter = OutputFormatter::with_detail(
            OutputFormat::Human,
            OutputDetail::Full,
            Styler::no_color(),
        );
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = ShowEntry {
            symbol: "my_func",
            kind: Some(&SymbolKind::Function),
            definitions: &defs,
            hover: None,
            total_reference_count: 2,
            total_reference_files: 1,
            displayed_references: Vec::new(),
            remaining_reference_count: 0,
            show_individual_refs: false,
            show_doc: false,
            test_references: Some(TestReferencesSection {
                total_count: 4,
                displayed: Vec::new(),
                remaining_count: 0,
            }),
        };
        let result = formatter.format_show(&entry, &SourceCache::new());
        assert!(
            result.contains("Test Refs: 4 (use --tests/-t to show)"),
            "full show should show test refs heading with count, got:\n{result}"
        );
    }

    #[test]
    fn test_format_show_condensed_with_test_refs_shown() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = ShowEntry {
            symbol: "my_func",
            kind: Some(&SymbolKind::Function),
            definitions: &defs,
            hover: None,
            total_reference_count: 2,
            total_reference_files: 1,
            displayed_references: Vec::new(),
            remaining_reference_count: 0,
            show_individual_refs: true,
            show_doc: false,
            test_references: Some(TestReferencesSection {
                total_count: 1,
                displayed: vec![EnrichedReference {
                    location: make_location("file:///project/tests/test_main.py", 3, 0),
                    context: "test_my_func".to_string(),
                }],
                remaining_count: 0,
            }),
        };
        let result = formatter.format_show(&entry, &SourceCache::new());
        assert!(result.contains("# Test Refs:"), "should show test refs section, got:\n{result}");
        assert!(result.contains("test_main.py"), "should show test file, got:\n{result}");
    }

    // ── Color output tests ──────────────────────────────────────────────

    /// Returns true if the string contains any ANSI escape sequences.
    fn has_ansi(s: &str) -> bool {
        s.contains('\x1b')
    }

    /// Create a formatter with color always enabled (for testing).
    fn formatter_with_color() -> OutputFormatter {
        use crate::cli::style::UseColor;
        OutputFormatter::with_detail_and_styler(
            OutputFormat::Human,
            OutputDetail::Condensed,
            Styler::new(UseColor::Yes),
        )
    }

    #[test]
    fn test_color_never_produces_no_ansi_in_show() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = ShowEntry {
            symbol: "my_func",
            kind: Some(&SymbolKind::Function),
            definitions: &defs,
            hover: None,
            total_reference_count: 3,
            total_reference_files: 1,
            displayed_references: Vec::new(),
            remaining_reference_count: 0,
            show_individual_refs: false,
            show_doc: false,
            test_references: None,
        };
        let result = formatter.format_show(&entry, &SourceCache::new());

        assert!(
            !has_ansi(&result),
            "color=never should produce zero ANSI escape sequences, got:\n{result:?}"
        );
    }

    #[test]
    fn test_color_always_produces_ansi_in_show_headings() {
        let formatter = formatter_with_color();
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = ShowEntry {
            symbol: "my_func",
            kind: Some(&SymbolKind::Function),
            definitions: &defs,
            hover: None,
            total_reference_count: 3,
            total_reference_files: 1,
            displayed_references: Vec::new(),
            remaining_reference_count: 0,
            show_individual_refs: false,
            show_doc: false,
            test_references: None,
        };
        let result = formatter.format_show(&entry, &SourceCache::new());

        assert!(
            has_ansi(&result),
            "color=always should produce ANSI escape sequences in headings, got:\n{result:?}"
        );
        assert!(result.contains("\x1b["), "should contain ANSI escape codes");
    }

    #[test]
    fn test_color_never_produces_no_ansi_in_find() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let results = vec![
            ("foo".to_string(), vec![make_location("file:///test.py", 0, 0)]),
            ("bar".to_string(), vec![]),
        ];
        let result = formatter.format_find_results(&results, &SourceCache::new());

        assert!(
            !has_ansi(&result),
            "color=never should produce zero ANSI in find output, got:\n{result:?}"
        );
    }

    #[test]
    fn test_color_always_produces_ansi_in_find() {
        let formatter = formatter_with_color();
        let results = vec![
            ("foo".to_string(), vec![make_location("file:///test.py", 0, 0)]),
            ("bar".to_string(), vec![]),
        ];
        let result = formatter.format_find_results(&results, &SourceCache::new());

        assert!(
            has_ansi(&result),
            "color=always should produce ANSI in find output, got:\n{result:?}"
        );
    }

    #[test]
    fn test_json_format_never_gets_color() {
        use crate::cli::style::UseColor;
        let formatter = OutputFormatter::with_detail_and_styler(
            OutputFormat::Json,
            OutputDetail::Condensed,
            Styler::new(UseColor::Yes),
        );
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = make_entry("foo", Some(&SymbolKind::Function), &defs, None);
        let result = formatter.format_show(&entry, &SourceCache::new());

        assert!(
            !has_ansi(&result),
            "JSON output must never contain ANSI codes even with color=always, got:\n{result:?}"
        );
    }

    #[test]
    fn test_csv_format_never_gets_color() {
        use crate::cli::style::UseColor;
        let formatter = OutputFormatter::with_detail_and_styler(
            OutputFormat::Csv,
            OutputDetail::Condensed,
            Styler::new(UseColor::Yes),
        );
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = make_entry("foo", Some(&SymbolKind::Function), &defs, None);
        let result = formatter.format_show(&entry, &SourceCache::new());

        assert!(
            !has_ansi(&result),
            "CSV output must never contain ANSI codes even with color=always, got:\n{result:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_color_never_produces_no_ansi_in_members() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let result = members_tests::make_members_result();
        let output = formatter.format_members_result(&result);

        assert!(
            !has_ansi(&output),
            "color=never should produce zero ANSI in members output, got:\n{output:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_color_always_produces_ansi_in_members() {
        let formatter = formatter_with_color();
        let result = members_tests::make_members_result();
        let output = formatter.format_members_result(&result);

        assert!(
            has_ansi(&output),
            "color=always should produce ANSI in members output, got:\n{output:?}"
        );
    }

    // ========================================================================
    // strip_code_fences tests
    // ========================================================================

    #[test]
    fn test_strip_code_fences_no_fences() {
        let input = "plain text\nno fences here";
        assert_eq!(strip_code_fences(input), input);
    }

    #[test]
    fn test_strip_code_fences_only_fences() {
        let input = "```python\n```";
        assert_eq!(strip_code_fences(input), "");
    }

    #[test]
    fn test_strip_code_fences_mixed_content() {
        let input = "```python\ndef foo():\n    pass\n```\nsome text";
        let result = strip_code_fences(input);
        assert!(result.contains("def foo():"));
        assert!(result.contains("some text"));
        assert!(!result.contains("```"));
    }

    // ========================================================================
    // position_in_range tests
    // ========================================================================

    #[test]
    fn test_position_in_range_inside() {
        let range = Range {
            start: Position { line: 5, character: 0 },
            end: Position { line: 10, character: 20 },
        };
        assert!(position_in_range(&range, 7, 10));
    }

    #[test]
    fn test_position_in_range_at_start_boundary() {
        let range = Range {
            start: Position { line: 5, character: 3 },
            end: Position { line: 10, character: 20 },
        };
        assert!(position_in_range(&range, 5, 3));
    }

    #[test]
    fn test_position_in_range_at_end_boundary() {
        let range = Range {
            start: Position { line: 5, character: 0 },
            end: Position { line: 10, character: 20 },
        };
        assert!(position_in_range(&range, 10, 20));
    }

    #[test]
    fn test_position_in_range_before_start() {
        let range = Range {
            start: Position { line: 5, character: 5 },
            end: Position { line: 10, character: 20 },
        };
        assert!(!position_in_range(&range, 5, 2));
    }

    #[test]
    fn test_position_in_range_after_end() {
        let range = Range {
            start: Position { line: 5, character: 0 },
            end: Position { line: 10, character: 20 },
        };
        assert!(!position_in_range(&range, 10, 25));
    }

    #[test]
    fn test_position_in_range_line_before() {
        let range = Range {
            start: Position { line: 5, character: 0 },
            end: Position { line: 10, character: 20 },
        };
        assert!(!position_in_range(&range, 3, 10));
    }

    #[test]
    fn test_position_in_range_line_after() {
        let range = Range {
            start: Position { line: 5, character: 0 },
            end: Position { line: 10, character: 20 },
        };
        assert!(!position_in_range(&range, 12, 0));
    }

    #[test]
    fn test_position_in_range_same_line_range() {
        let range = Range {
            start: Position { line: 5, character: 3 },
            end: Position { line: 5, character: 10 },
        };
        assert!(position_in_range(&range, 5, 5));
        assert!(!position_in_range(&range, 5, 2));
        assert!(!position_in_range(&range, 5, 11));
    }

    // ========================================================================
    // format_definitions Paths mode
    // ========================================================================

    #[test]
    fn test_format_definitions_paths() {
        let formatter = OutputFormatter::new(OutputFormat::Paths);
        let locations = [make_location("file:///a.py", 1, 0), make_location("file:///b.py", 2, 0)];
        let result = formatter.format_definitions(&locations, "test", &SourceCache::new());
        assert!(result.contains("a.py"));
        assert!(result.contains("b.py"));
    }

    // ========================================================================
    // format_find_results multi-symbol JSON/CSV/Paths
    // ========================================================================

    #[test]
    fn test_format_find_results_multiple_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let results = vec![
            ("foo".to_string(), vec![make_location("file:///a.py", 0, 0)]),
            ("bar".to_string(), vec![make_location("file:///b.py", 1, 0)]),
        ];
        let output = formatter.format_find_results(&results, &SourceCache::new());
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
        assert_eq!(parsed[0]["symbol"], "foo");
        assert_eq!(parsed[1]["symbol"], "bar");
    }

    #[test]
    fn test_format_find_results_multiple_csv() {
        let formatter = OutputFormatter::new(OutputFormat::Csv);
        let results = vec![
            ("foo".to_string(), vec![make_location("file:///a.py", 0, 0)]),
            ("bar".to_string(), vec![make_location("file:///b.py", 1, 0)]),
        ];
        let output = formatter.format_find_results(&results, &SourceCache::new());
        assert!(output.starts_with("symbol,file,line,column\n"));
        assert!(output.contains("foo,"));
        assert!(output.contains("bar,"));
    }

    #[test]
    fn test_format_find_results_multiple_paths() {
        let formatter = OutputFormatter::new(OutputFormat::Paths);
        let results = vec![
            ("foo".to_string(), vec![make_location("file:///a.py", 0, 0)]),
            (
                "bar".to_string(),
                vec![make_location("file:///a.py", 1, 0), make_location("file:///b.py", 2, 0)],
            ),
        ];
        let output = formatter.format_find_results(&results, &SourceCache::new());
        // Should be sorted and deduped
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines.len() >= 2);
        // a.py should appear only once (deduped)
        assert_eq!(lines.iter().filter(|l| l.contains("a.py")).count(), 1);
    }

    // ========================================================================
    // format_enriched_references_results multi-result
    // ========================================================================

    fn make_enriched_result(label: &str, count: usize) -> EnrichedReferencesResult {
        let displayed: Vec<EnrichedReference> = (0..count)
            .map(|i| EnrichedReference {
                location: make_location("file:///ref.py", u32::try_from(i).unwrap(), 0),
                context: "module scope".to_string(),
            })
            .collect();
        EnrichedReferencesResult {
            label: label.to_string(),
            total_count: count,
            displayed,
            remaining_count: 0,
            test_references: None,
        }
    }

    #[test]
    fn test_format_enriched_references_multiple_human() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let results = vec![make_enriched_result("foo", 1), make_enriched_result("bar", 2)];
        let output = formatter.format_enriched_references_results(&results, &SourceCache::new());
        assert!(output.contains("=== foo ==="));
        assert!(output.contains("=== bar ==="));
    }

    #[test]
    fn test_format_enriched_references_multiple_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let results = vec![make_enriched_result("foo", 1), make_enriched_result("bar", 1)];
        let output = formatter.format_enriched_references_results(&results, &SourceCache::new());
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_format_enriched_references_multiple_csv() {
        let formatter = OutputFormatter::new(OutputFormat::Csv);
        let results = vec![make_enriched_result("foo", 1), make_enriched_result("bar", 1)];
        let output = formatter.format_enriched_references_results(&results, &SourceCache::new());
        assert!(output.starts_with("symbol,file,line,column,context,test\n"));
        assert!(output.contains("foo,"));
        assert!(output.contains("bar,"));
    }

    #[test]
    fn test_format_enriched_references_multiple_paths() {
        let formatter = OutputFormatter::new(OutputFormat::Paths);
        let results = vec![make_enriched_result("foo", 1), make_enriched_result("bar", 1)];
        let output = formatter.format_enriched_references_results(&results, &SourceCache::new());
        assert!(output.contains("ref.py"));
    }

    // ========================================================================
    // format_show CSV/Paths single + multi-result
    // ========================================================================

    #[test]
    fn test_format_show_csv_single() {
        let formatter = OutputFormatter::new(OutputFormat::Csv);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = make_entry("Animal", Some(&SymbolKind::Class), &defs, None);
        let result = formatter.format_show(&entry, &SourceCache::new());
        assert!(result.starts_with("section,file,line,column,context\n"));
        assert!(result.contains("definition,"));
    }

    #[test]
    fn test_format_show_paths_single() {
        let formatter = OutputFormatter::new(OutputFormat::Paths);
        let defs = [make_location("file:///a.py", 0, 0), make_location("file:///b.py", 1, 0)];
        let entry = make_entry("Animal", None, &defs, None);
        let result = formatter.format_show(&entry, &SourceCache::new());
        assert!(result.contains("a.py"));
        assert!(result.contains("b.py"));
    }

    #[test]
    fn test_format_show_results_multiple_human() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs1 = [make_location("file:///a.py", 0, 0)];
        let defs2 = [make_location("file:///b.py", 1, 0)];
        let entry1 = make_entry("Foo", Some(&SymbolKind::Class), &defs1, None);
        let entry2 = make_entry("Bar", Some(&SymbolKind::Function), &defs2, None);
        let result = formatter.format_show_results(&[entry1, entry2], &SourceCache::new());
        assert!(result.contains("# Foo"));
        assert!(result.contains("# Bar"));
    }

    #[test]
    fn test_format_show_results_multiple_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let defs1 = [make_location("file:///a.py", 0, 0)];
        let defs2 = [make_location("file:///b.py", 1, 0)];
        let entry1 = make_entry("Foo", Some(&SymbolKind::Class), &defs1, None);
        let entry2 = make_entry("Bar", Some(&SymbolKind::Function), &defs2, None);
        let result = formatter.format_show_results(&[entry1, entry2], &SourceCache::new());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_format_show_results_multiple_csv() {
        let formatter = OutputFormatter::new(OutputFormat::Csv);
        let defs1 = [make_location("file:///a.py", 0, 0)];
        let defs2 = [make_location("file:///b.py", 1, 0)];
        let entry1 = make_entry("Foo", Some(&SymbolKind::Class), &defs1, None);
        let entry2 = make_entry("Bar", Some(&SymbolKind::Function), &defs2, None);
        let result = formatter.format_show_results(&[entry1, entry2], &SourceCache::new());
        assert!(result.starts_with("symbol,section,file,line,column,context\n"));
        assert!(result.contains("Foo,"));
        assert!(result.contains("Bar,"));
    }

    #[test]
    fn test_format_show_results_multiple_paths() {
        let formatter = OutputFormatter::new(OutputFormat::Paths);
        let defs1 = [make_location("file:///a.py", 0, 0)];
        let defs2 = [make_location("file:///b.py", 1, 0)];
        let entry1 = make_entry("Foo", None, &defs1, None);
        let entry2 = make_entry("Bar", None, &defs2, None);
        let result = formatter.format_show_results(&[entry1, entry2], &SourceCache::new());
        assert!(result.contains("a.py"));
        assert!(result.contains("b.py"));
    }

    // ========================================================================
    // format_document_symbols
    // ========================================================================

    #[test]
    fn test_format_document_symbols_human() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let child = make_doc_symbol("method", SymbolKind::Method, 2, 4, None);
        let parent = make_doc_symbol("MyClass", SymbolKind::Class, 0, 5, Some(vec![child]));
        let symbols = vec![parent];
        let result = formatter.format_document_symbols(&symbols);
        assert!(result.contains("MyClass"));
        assert!(result.contains("method"));
    }

    #[test]
    fn test_format_document_symbols_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let symbols = vec![make_doc_symbol("MyClass", SymbolKind::Class, 0, 5, None)];
        let result = formatter.format_document_symbols(&symbols);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_array());
    }

    #[test]
    fn test_format_document_symbols_csv() {
        let formatter = OutputFormatter::new(OutputFormat::Csv);
        let symbols = vec![make_doc_symbol("MyClass", SymbolKind::Class, 0, 5, None)];
        let result = formatter.format_document_symbols(&symbols);
        assert!(result.starts_with("name,kind,line,column\n"));
        assert!(result.contains("MyClass"));
    }

    // ========================================================================
    // format_workspace_symbols
    // ========================================================================

    fn make_symbol_info(name: &str, kind: SymbolKind, uri: &str, line: u32) -> SymbolInformation {
        SymbolInformation {
            name: name.to_string(),
            kind,
            tags: None,
            deprecated: None,
            location: make_location(uri, line, 0),
            container_name: None,
        }
    }

    #[test]
    fn test_format_workspace_symbols_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let symbols = vec![make_symbol_info("MyClass", SymbolKind::Class, "file:///a.py", 0)];
        let result = formatter.format_workspace_symbols(&symbols);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["name"], "MyClass");
    }

    #[test]
    fn test_format_workspace_symbols_csv() {
        let formatter = OutputFormatter::new(OutputFormat::Csv);
        let symbols = vec![make_symbol_info("MyClass", SymbolKind::Class, "file:///a.py", 0)];
        let result = formatter.format_workspace_symbols(&symbols);
        assert!(result.starts_with("name,kind,file,line,column\n"));
        assert!(result.contains("MyClass"));
    }

    #[test]
    fn test_format_workspace_symbols_paths() {
        let formatter = OutputFormatter::new(OutputFormat::Paths);
        let symbols = vec![
            make_symbol_info("A", SymbolKind::Class, "file:///a.py", 0),
            make_symbol_info("B", SymbolKind::Function, "file:///b.py", 0),
        ];
        let result = formatter.format_workspace_symbols(&symbols);
        assert!(result.contains("a.py"));
        assert!(result.contains("b.py"));
    }

    // ========================================================================
    // kind_label
    // ========================================================================

    #[test]
    fn test_kind_label_all_variants() {
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Function), "func");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Method), "method");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Class), "class");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Variable), "var");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Constant), "const");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Module), "module");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Property), "prop");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Field), "field");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Constructor), "ctor");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Enum), "enum");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Interface), "iface");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::Struct), "struct");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::EnumMember), "member");
        assert_eq!(OutputFormatter::kind_label(&SymbolKind::TypeParameter), "type");
    }

    // ========================================================================
    // extract_hover_* helpers
    // ========================================================================

    #[test]
    fn test_extract_hover_text_array() {
        use crate::lsp::protocol::{MarkedString, MarkedStringOrString};

        let contents = HoverContents::Array(vec![
            MarkedStringOrString::String("first".to_string()),
            MarkedStringOrString::MarkedString(MarkedString {
                language: "python".to_string(),
                value: "second".to_string(),
            }),
        ]);
        let result = OutputFormatter::extract_hover_text(&contents);
        assert!(result.contains("first"));
        assert!(result.contains("second"));
    }

    #[test]
    fn test_extract_hover_type_no_fences_no_doc() {
        use crate::lsp::protocol::HoverContents;

        let contents = HoverContents::Scalar("int".to_string());
        let result = OutputFormatter::extract_hover_type(&contents);
        assert_eq!(result, "int");
    }

    #[test]
    fn test_extract_hover_doc_empty_after_separator() {
        use crate::lsp::protocol::{HoverContents, MarkupContent, MarkupKind};

        let contents = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```python\ndef foo()\n```\n---\n".to_string(),
        });
        let result = OutputFormatter::extract_hover_doc(&contents);
        assert!(result.is_none(), "empty doc after separator should return None");
    }

    #[test]
    fn test_extract_hover_doc_with_content() {
        use crate::lsp::protocol::{HoverContents, MarkupContent, MarkupKind};

        let contents = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```python\ndef foo()\n```\n---\nThis is the docstring.".to_string(),
        });
        let result = OutputFormatter::extract_hover_doc(&contents);
        assert_eq!(result.unwrap(), "This is the docstring.");
    }

    // ========================================================================
    // read_definition_context
    // ========================================================================

    #[test]
    fn test_read_definition_context_start_at_keyword() {
        let path = "/tmp/test_def_ctx_keyword.py";
        let content = "@dataclass\n@frozen\nclass Config:\n    host: str\n";
        let cache = SourceCache::from_entries([(path.to_string(), content.to_string())]);

        let ctx = read_definition_context(&cache, path, 0).unwrap();
        assert_eq!(ctx.definition_line, "class Config:");
        assert!(ctx.decorators.is_some());
        let decorators = ctx.decorators.unwrap();
        assert!(decorators.contains("@dataclass"));
        assert!(decorators.contains("@frozen"));
    }

    #[test]
    fn test_read_definition_context_no_def_found() {
        let path = "/tmp/test_def_ctx_no_def.py";
        let content = "@only_decorators\n@more\n";
        let cache = SourceCache::from_entries([(path.to_string(), content.to_string())]);

        let ctx = read_definition_context(&cache, path, 0);
        assert!(ctx.is_none(), "all decorator lines with nothing after should return None");
    }
}
