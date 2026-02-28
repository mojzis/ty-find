use crate::cli::args::{OutputDetail, OutputFormat};
use crate::lsp::protocol::{
    DocumentSymbol, Hover, HoverContents, Location, MarkedStringOrString, SymbolInformation,
    SymbolKind,
};
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// A single inspect result with optional symbol kind.
pub struct InspectEntry<'a> {
    pub symbol: &'a str,
    pub kind: Option<&'a SymbolKind>,
    pub definitions: &'a [Location],
    pub hover: Option<&'a Hover>,
    pub references: &'a [Location],
}

pub struct OutputFormatter {
    format: OutputFormat,
    detail: OutputDetail,
    cwd: PathBuf,
}

/// Read a single line of source code from a file (1-based line number).
fn read_source_line(file_path: &str, line: u32) -> Option<String> {
    let content = std::fs::read_to_string(file_path).ok()?;
    content.lines().nth((line - 1) as usize).map(|s| s.trim().to_string())
}

impl OutputFormatter {
    #[cfg(test)]
    pub fn new(format: OutputFormat) -> Self {
        Self::with_detail(format, OutputDetail::default())
    }

    pub fn with_detail(format: OutputFormat, detail: OutputDetail) -> Self {
        Self { format, detail, cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")) }
    }

    pub fn format_definitions(&self, locations: &[Location], query_info: &str) -> String {
        match self.format {
            OutputFormat::Human => self.format_human(locations, query_info),
            OutputFormat::Json => Self::format_json(locations),
            OutputFormat::Csv => self.format_csv(locations),
            OutputFormat::Paths => self.format_paths(locations),
        }
    }

    fn format_human(&self, locations: &[Location], query_info: &str) -> String {
        if locations.is_empty() {
            return format!("No definitions found for: {query_info}");
        }

        let mut output = format!("Found {} definition(s) for: {query_info}\n\n", locations.len());

        for (i, location) in locations.iter().enumerate() {
            let file_path = self.uri_to_path(&location.uri);
            let line = location.range.start.line + 1;
            let column = location.range.start.character + 1;

            let _ = writeln!(output, "{}. {file_path}:{line}:{column}", i + 1);

            if let Some(src) = read_source_line(&file_path, line) {
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
    pub fn format_find_results(&self, results: &[(String, Vec<Location>)]) -> String {
        if results.len() == 1 {
            let (symbol, locations) = &results[0];
            if locations.is_empty() {
                return format!("No definitions found for: '{symbol}'");
            }
            let query_info = format!("'{symbol}'");
            return self.format_definitions(locations, &query_info);
        }

        match self.format {
            OutputFormat::Human => {
                let mut output = String::new();
                for (symbol, locations) in results {
                    let _ = writeln!(output, "=== {symbol} ===");
                    if locations.is_empty() {
                        output.push_str("No definitions found.\n");
                    } else {
                        output.push_str(&self.format_human(locations, &format!("'{symbol}'")));
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

    pub fn format_references(&self, locations: &[Location], query_info: &str) -> String {
        match self.format {
            OutputFormat::Human => {
                if locations.is_empty() {
                    return format!("No references found for: {query_info}");
                }

                let mut output =
                    format!("Found {} reference(s) for: {query_info}\n\n", locations.len());

                for (i, location) in locations.iter().enumerate() {
                    let file_path = self.uri_to_path(&location.uri);
                    let line = location.range.start.line + 1;
                    let column = location.range.start.character + 1;

                    let _ = writeln!(output, "{}. {file_path}:{line}:{column}", i + 1);

                    if let Some(src) = read_source_line(&file_path, line) {
                        let _ = writeln!(output, "   {src}");
                    }
                    output.push('\n');
                }

                output
            }
            OutputFormat::Json => Self::format_json(locations),
            OutputFormat::Csv => self.format_csv(locations),
            OutputFormat::Paths => self.format_paths(locations),
        }
    }

    /// Format results for one or more symbol reference queries, grouped by symbol.
    pub fn format_references_results(&self, results: &[(String, Vec<Location>)]) -> String {
        if results.len() == 1 {
            let (symbol, locations) = &results[0];
            let query_info = format!("'{symbol}'");
            return self.format_references(locations, &query_info);
        }

        match self.format {
            OutputFormat::Human => {
                let mut output = String::new();
                for (symbol, locations) in results {
                    let _ = writeln!(output, "=== {symbol} ===");
                    if locations.is_empty() {
                        output.push_str("No references found.\n");
                    } else {
                        output.push_str(&self.format_references(locations, &format!("'{symbol}'")));
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
                            "references": locations,
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

    pub fn format_hover(&self, hover: &Hover, query_info: &str) -> String {
        match self.format {
            OutputFormat::Human => {
                let mut output = format!("Hover information for: {query_info}\n\n");

                let content_str = Self::extract_hover_text(&hover.contents);

                output.push_str(&content_str);
                output.push('\n');

                output
            }
            OutputFormat::Json => {
                serde_json::to_string_pretty(hover).unwrap_or_else(|_| "{}".to_string())
            }
            OutputFormat::Csv | OutputFormat::Paths => {
                // CSV and Paths formats don't make sense for hover, fall back to human
                Self::extract_hover_text(&hover.contents)
            }
        }
    }

    pub fn format_workspace_symbols(&self, symbols: &[SymbolInformation]) -> String {
        match self.format {
            OutputFormat::Human => {
                let mut output = String::new();

                for (i, symbol) in symbols.iter().enumerate() {
                    let file_path = self.uri_to_path(&symbol.location.uri);
                    let line = symbol.location.range.start.line + 1;
                    let column = symbol.location.range.start.character + 1;

                    let _ = write!(
                        output,
                        "{}. {} ({:?})\n   {file_path}:{line}:{column}\n\n",
                        i + 1,
                        symbol.name,
                        symbol.kind,
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

    /// Format a single symbol inspect, using the header level appropriate for context.
    /// `h_level` controls markdown heading depth (1 = `#`, 2 = `##`).
    fn format_inspect_human(
        &self,
        symbol: &str,
        kind: Option<&SymbolKind>,
        definitions: &[Location],
        hover: Option<&Hover>,
        references: &[Location],
        h_level: u8,
    ) -> String {
        match self.detail {
            OutputDetail::Condensed => {
                self.format_inspect_condensed(kind, definitions, hover, references, h_level)
            }
            OutputDetail::Full => {
                self.format_inspect_full(symbol, definitions, hover, references, h_level)
            }
        }
    }

    fn format_inspect_condensed(
        &self,
        kind: Option<&SymbolKind>,
        definitions: &[Location],
        hover: Option<&Hover>,
        references: &[Location],
        h_level: u8,
    ) -> String {
        let h = "#".repeat(h_level as usize);
        let mut output = String::new();

        // Def section — paths only, no source snippets (symbol name is already known)
        if let Some(kind) = kind {
            let _ = writeln!(output, "{h} Def ({})", Self::kind_label(kind));
        } else {
            let _ = writeln!(output, "{h} Def");
        }
        if definitions.is_empty() {
            output.push_str("(none)\n");
        } else {
            for location in definitions {
                let file_path = self.uri_to_path(&location.uri);
                let line = location.range.start.line + 1;
                let column = location.range.start.character + 1;
                let _ = writeln!(output, "{file_path}:{line}:{column}");
            }
        }

        // Type section — skip entirely when empty
        if let Some(hover) = hover {
            let _ = writeln!(output, "\n{h} Type");
            output.push_str(&Self::extract_hover_text(&hover.contents));
            output.push('\n');
        }

        // Refs section — paths only, only show when there are actual references
        if !references.is_empty() {
            let _ = writeln!(output, "\n{h} Refs ({})", references.len());
            for location in references {
                let file_path = self.uri_to_path(&location.uri);
                let line = location.range.start.line + 1;
                let column = location.range.start.character + 1;
                let _ = writeln!(output, "{file_path}:{line}:{column}");
            }
        }

        output
    }

    fn format_inspect_full(
        &self,
        symbol: &str,
        definitions: &[Location],
        hover: Option<&Hover>,
        references: &[Location],
        h_level: u8,
    ) -> String {
        let h = "#".repeat(h_level as usize);
        let h2 = "#".repeat(h_level as usize + 1);
        let mut output = format!("{h} Inspect: {symbol}\n\n");

        // Definition section
        let _ = writeln!(output, "{h2} Definition");
        if definitions.is_empty() {
            output.push_str("No definitions found.\n");
        } else {
            for (i, location) in definitions.iter().enumerate() {
                let file_path = self.uri_to_path(&location.uri);
                let line = location.range.start.line + 1;
                let column = location.range.start.character + 1;
                let _ = writeln!(output, "{}. {file_path}:{line}:{column}", i + 1);

                if let Some(src) = read_source_line(&file_path, line) {
                    let _ = writeln!(output, "   {src}");
                }
            }
        }
        output.push('\n');

        // Type section
        let _ = writeln!(output, "{h2} Type Info");
        if let Some(hover) = hover {
            output.push_str(&Self::extract_hover_text(&hover.contents));
            output.push('\n');
        } else {
            output.push_str("No hover information available.\n");
        }
        output.push('\n');

        // References section
        let _ = writeln!(output, "{h2} References");
        if references.is_empty() {
            output.push_str("No references found.\n");
        } else {
            let _ = writeln!(output, "{} reference(s):", references.len());
            for (i, location) in references.iter().enumerate() {
                let file_path = self.uri_to_path(&location.uri);
                let line = location.range.start.line + 1;
                let column = location.range.start.character + 1;
                let _ = writeln!(output, "{}. {file_path}:{line}:{column}", i + 1);

                if let Some(src) = read_source_line(&file_path, line) {
                    let _ = writeln!(output, "   {src}");
                }
            }
        }

        output
    }

    pub fn format_inspect(&self, entry: &InspectEntry<'_>) -> String {
        match self.format {
            OutputFormat::Human => self.format_inspect_human(
                entry.symbol,
                entry.kind,
                entry.definitions,
                entry.hover,
                entry.references,
                1,
            ),
            OutputFormat::Json => {
                let json_val = serde_json::json!({
                    "symbol": entry.symbol,
                    "kind": entry.kind.map(Self::kind_label),
                    "definitions": entry.definitions,
                    "hover": entry.hover,
                    "references": entry.references,
                });
                serde_json::to_string_pretty(&json_val).unwrap_or_else(|_| "{}".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("section,file,line,column\n");
                for location in entry.definitions {
                    let file_path = self.uri_to_path(&location.uri);
                    let line = location.range.start.line + 1;
                    let column = location.range.start.character + 1;
                    let _ = writeln!(output, "definition,{file_path},{line},{column}");
                }
                for location in entry.references {
                    let file_path = self.uri_to_path(&location.uri);
                    let line = location.range.start.line + 1;
                    let column = location.range.start.character + 1;
                    let _ = writeln!(output, "reference,{file_path},{line},{column}");
                }
                output
            }
            OutputFormat::Paths => {
                let mut paths: Vec<String> = entry
                    .definitions
                    .iter()
                    .chain(entry.references.iter())
                    .map(|loc| self.uri_to_path(&loc.uri))
                    .collect();
                paths.sort();
                paths.dedup();
                paths.join("\n")
            }
        }
    }

    /// Format results for one or more symbol inspect queries, grouped by symbol.
    pub fn format_inspect_results(&self, results: &[InspectEntry<'_>]) -> String {
        if results.len() == 1 {
            return self.format_inspect(&results[0]);
        }

        match self.format {
            OutputFormat::Human => {
                let mut output = String::new();
                for entry in results {
                    // Multi-symbol: symbol name gets top-level heading, sections get sub-headings
                    let _ = writeln!(output, "# {}", entry.symbol);
                    output.push_str(&self.format_inspect_human(
                        entry.symbol,
                        entry.kind,
                        entry.definitions,
                        entry.hover,
                        entry.references,
                        2,
                    ));
                    output.push('\n');
                }
                output.trim_end().to_string()
            }
            OutputFormat::Json => {
                let grouped: Vec<serde_json::Value> = results
                    .iter()
                    .map(|entry| {
                        serde_json::json!({
                            "symbol": entry.symbol,
                            "kind": entry.kind.map(Self::kind_label),
                            "definitions": entry.definitions,
                            "hover": entry.hover,
                            "references": entry.references,
                        })
                    })
                    .collect();
                serde_json::to_string_pretty(&grouped).unwrap_or_else(|_| "[]".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("symbol,section,file,line,column\n");
                for entry in results {
                    for location in entry.definitions {
                        let file_path = self.uri_to_path(&location.uri);
                        let line = location.range.start.line + 1;
                        let column = location.range.start.character + 1;
                        let _ = writeln!(
                            output,
                            "{},definition,{file_path},{line},{column}",
                            entry.symbol
                        );
                    }
                    for location in entry.references {
                        let file_path = self.uri_to_path(&location.uri);
                        let line = location.range.start.line + 1;
                        let column = location.range.start.character + 1;
                        let _ = writeln!(
                            output,
                            "{},reference,{file_path},{line},{column}",
                            entry.symbol
                        );
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
                            .chain(entry.references.iter())
                            .map(|loc| self.uri_to_path(&loc.uri))
                    })
                    .collect();
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
    use crate::lsp::protocol::{
        HoverContents, MarkupContent, MarkupKind, Position, Range, SymbolKind,
    };

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
        let result = formatter.format_definitions(&[], "test:1:1");
        assert_eq!(result, "No definitions found for: test:1:1");
    }

    #[test]
    fn test_format_definitions_single() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let locations = [make_location("file:///nonexistent.py", 5, 10)];
        let result = formatter.format_definitions(&locations, "test:6:11");

        assert!(result.contains("Found 1 definition(s)"));
        assert!(result.contains("nonexistent.py:6:11"));
    }

    #[test]
    fn test_format_definitions_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let locations = [make_location("file:///test.py", 0, 0)];
        let result = formatter.format_definitions(&locations, "test");

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["uri"], "file:///test.py");
    }

    #[test]
    fn test_format_definitions_csv() {
        let formatter = OutputFormatter::new(OutputFormat::Csv);
        let locations = [make_location("file:///test.py", 4, 2)];
        let result = formatter.format_definitions(&locations, "test");

        assert!(result.starts_with("file,line,column\n"));
        assert!(result.contains("5,3")); // 0-based -> 1-based
    }

    #[test]
    fn test_format_find_results_single_symbol() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let locations = vec![make_location("file:///test.py", 0, 0)];
        let results = vec![("foo".to_string(), locations)];
        let result = formatter.format_find_results(&results);

        assert!(result.contains("Found 1 definition(s) for: 'foo'"));
    }

    #[test]
    fn test_format_find_results_symbol_not_found() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let results = vec![("missing".to_string(), vec![])];
        let result = formatter.format_find_results(&results);

        assert_eq!(result, "No definitions found for: 'missing'");
    }

    #[test]
    fn test_format_find_results_multiple_symbols() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let results = vec![
            ("foo".to_string(), vec![make_location("file:///test.py", 0, 0)]),
            ("bar".to_string(), vec![]),
        ];
        let result = formatter.format_find_results(&results);

        assert!(result.contains("=== foo ==="));
        assert!(result.contains("=== bar ==="));
        assert!(result.contains("No definitions found."));
    }

    #[test]
    fn test_format_references_empty() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let result = formatter.format_references(&[], "test:1:1");
        assert_eq!(result, "No references found for: test:1:1");
    }

    #[test]
    fn test_format_hover_markup() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let hover = Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "def foo() -> int".to_string(),
            }),
            range: None,
        };
        let result = formatter.format_hover(&hover, "test:1:1");

        assert!(result.contains("Hover information for: test:1:1"));
        assert!(result.contains("def foo() -> int"));
    }

    #[test]
    fn test_format_hover_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let hover = Hover { contents: HoverContents::Scalar("hello".to_string()), range: None };
        let result = formatter.format_hover(&hover, "test");

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_object());
        assert!(parsed.get("contents").is_some());
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
        references: &'a [Location],
    ) -> InspectEntry<'a> {
        InspectEntry { symbol, kind, definitions, hover, references }
    }

    #[test]
    fn test_format_inspect_condensed_empty() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let entry = make_entry("missing", None, &[], None, &[]);
        let result = formatter.format_inspect(&entry);

        // Condensed: no symbol header for single symbol, short section names
        assert!(result.contains("# Def"));
        assert!(result.contains("(none)"));
        // No Type or Refs sections when empty in condensed
        assert!(!result.contains("# Type"));
        assert!(!result.contains("# Refs"));
    }

    #[test]
    fn test_format_inspect_condensed_with_kind() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = make_entry("foo", Some(&SymbolKind::Function), &defs, None, &[]);
        let result = formatter.format_inspect(&entry);

        assert!(result.contains("# Def (func)"));
        assert!(result.contains("test.py:1:1"));
    }

    #[test]
    fn test_format_inspect_full_empty() {
        let formatter = OutputFormatter::with_detail(OutputFormat::Human, OutputDetail::Full);
        let entry = make_entry("missing", None, &[], None, &[]);
        let result = formatter.format_inspect(&entry);

        assert!(result.contains("# Inspect: missing"));
        assert!(result.contains("## Definition"));
        assert!(result.contains("No definitions found."));
        assert!(result.contains("No hover information available."));
        assert!(result.contains("No references found."));
    }

    #[test]
    fn test_format_inspect_condensed_with_defs() {
        let formatter = OutputFormatter::new(OutputFormat::Human);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = make_entry("foo", None, &defs, None, &[]);
        let result = formatter.format_inspect(&entry);

        // Should show path:line:col on one line (no numbering)
        assert!(result.contains("test.py:1:1"));
        assert!(result.contains("# Def"));
        // No symbol name header for single symbol in condensed
        assert!(!result.contains("foo"));
    }

    #[test]
    fn test_format_inspect_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let defs = [make_location("file:///test.py", 0, 0)];
        let entry = make_entry("foo", Some(&SymbolKind::Function), &defs, None, &[]);
        let result = formatter.format_inspect(&entry);

        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["symbol"], "foo");
        assert_eq!(parsed["kind"], "func");
        assert!(parsed["definitions"].is_array());
    }

    #[test]
    fn test_read_source_line_valid() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        std::fs::write(&file, "line 1\n  line 2\nline 3\n").unwrap();

        assert_eq!(read_source_line(file.to_str().unwrap(), 1), Some("line 1".to_string()));
        assert_eq!(read_source_line(file.to_str().unwrap(), 2), Some("line 2".to_string()));
        assert_eq!(read_source_line(file.to_str().unwrap(), 3), Some("line 3".to_string()));
        assert_eq!(read_source_line(file.to_str().unwrap(), 4), None);
    }

    #[test]
    fn test_read_source_line_nonexistent_file() {
        assert_eq!(read_source_line("/nonexistent/file.py", 1), None);
    }
}
