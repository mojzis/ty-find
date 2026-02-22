use crate::cli::args::OutputFormat;
use crate::lsp::protocol::{
    DocumentSymbol, Hover, HoverContents, Location, MarkedStringOrString, SymbolInformation,
};
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// A single inspect result: (`symbol_name`, definitions, hover, references).
pub type InspectEntry<'a> = (&'a str, &'a [Location], Option<&'a Hover>, &'a [Location]);

pub struct OutputFormatter {
    format: OutputFormat,
    cwd: PathBuf,
}

impl OutputFormatter {
    pub fn new(format: OutputFormat) -> Self {
        Self { format, cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")) }
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

            if let Ok(content) = std::fs::read_to_string(&file_path) {
                let lines: Vec<&str> = content.lines().collect();
                if let Some(line_content) = lines.get((line - 1) as usize) {
                    let _ = writeln!(output, "   {}", line_content.trim());
                }
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

                    if let Ok(content) = std::fs::read_to_string(&file_path) {
                        let lines: Vec<&str> = content.lines().collect();
                        if let Some(line_content) = lines.get((line - 1) as usize) {
                            let _ = writeln!(output, "   {}", line_content.trim());
                        }
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

    pub fn format_inspect(
        &self,
        symbol: &str,
        definitions: &[Location],
        hover: Option<&Hover>,
        references: &[Location],
    ) -> String {
        match self.format {
            OutputFormat::Human => {
                let mut output = format!("=== Inspect: {symbol} ===\n\n");

                // Definition section
                output.push_str("--- Definition ---\n");
                if definitions.is_empty() {
                    output.push_str("No definitions found.\n");
                } else {
                    for (i, location) in definitions.iter().enumerate() {
                        let file_path = self.uri_to_path(&location.uri);
                        let line = location.range.start.line + 1;
                        let column = location.range.start.character + 1;
                        let _ = writeln!(output, "{}. {file_path}:{line}:{column}", i + 1);

                        if let Ok(content) = std::fs::read_to_string(&file_path) {
                            let lines: Vec<&str> = content.lines().collect();
                            if let Some(line_content) = lines.get((line - 1) as usize) {
                                let _ = writeln!(output, "   {}", line_content.trim());
                            }
                        }
                    }
                }
                output.push('\n');

                // Hover section
                output.push_str("--- Type Info ---\n");
                if let Some(hover) = hover {
                    output.push_str(&Self::extract_hover_text(&hover.contents));
                    output.push('\n');
                } else {
                    output.push_str("No hover information available.\n");
                }
                output.push('\n');

                // References section
                output.push_str("--- References ---\n");
                if references.is_empty() {
                    output.push_str("No references found.\n");
                } else {
                    let _ = writeln!(output, "{} reference(s):", references.len());
                    for (i, location) in references.iter().enumerate() {
                        let file_path = self.uri_to_path(&location.uri);
                        let line = location.range.start.line + 1;
                        let column = location.range.start.character + 1;
                        let _ = writeln!(output, "{}. {file_path}:{line}:{column}", i + 1);

                        if let Ok(content) = std::fs::read_to_string(&file_path) {
                            let lines: Vec<&str> = content.lines().collect();
                            if let Some(line_content) = lines.get((line - 1) as usize) {
                                let _ = writeln!(output, "   {}", line_content.trim());
                            }
                        }
                    }
                }

                output
            }
            OutputFormat::Json => {
                let json_val = serde_json::json!({
                    "symbol": symbol,
                    "definitions": definitions,
                    "hover": hover,
                    "references": references,
                });
                serde_json::to_string_pretty(&json_val).unwrap_or_else(|_| "{}".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("section,file,line,column\n");
                for location in definitions {
                    let file_path = self.uri_to_path(&location.uri);
                    let line = location.range.start.line + 1;
                    let column = location.range.start.character + 1;
                    let _ = writeln!(output, "definition,{file_path},{line},{column}");
                }
                for location in references {
                    let file_path = self.uri_to_path(&location.uri);
                    let line = location.range.start.line + 1;
                    let column = location.range.start.character + 1;
                    let _ = writeln!(output, "reference,{file_path},{line},{column}");
                }
                output
            }
            OutputFormat::Paths => {
                let mut paths: Vec<String> = definitions
                    .iter()
                    .chain(references.iter())
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
            let (symbol, definitions, hover, references) = &results[0];
            return self.format_inspect(symbol, definitions, *hover, references);
        }

        match self.format {
            OutputFormat::Human => {
                let mut output = String::new();
                for (symbol, definitions, hover, references) in results {
                    output.push_str(&self.format_inspect(symbol, definitions, *hover, references));
                    output.push('\n');
                }
                output.trim_end().to_string()
            }
            OutputFormat::Json => {
                let grouped: Vec<serde_json::Value> = results
                    .iter()
                    .map(|(symbol, definitions, hover, references)| {
                        serde_json::json!({
                            "symbol": symbol,
                            "definitions": definitions,
                            "hover": hover,
                            "references": references,
                        })
                    })
                    .collect();
                serde_json::to_string_pretty(&grouped).unwrap_or_else(|_| "[]".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("symbol,section,file,line,column\n");
                for (symbol, definitions, _, references) in results {
                    for location in *definitions {
                        let file_path = self.uri_to_path(&location.uri);
                        let line = location.range.start.line + 1;
                        let column = location.range.start.character + 1;
                        let _ =
                            writeln!(output, "{symbol},definition,{file_path},{line},{column}",);
                    }
                    for location in *references {
                        let file_path = self.uri_to_path(&location.uri);
                        let line = location.range.start.line + 1;
                        let column = location.range.start.character + 1;
                        let _ = writeln!(output, "{symbol},reference,{file_path},{line},{column}",);
                    }
                }
                output
            }
            OutputFormat::Paths => {
                let mut paths: Vec<String> = results
                    .iter()
                    .flat_map(|(_, definitions, _, references)| {
                        definitions
                            .iter()
                            .chain(references.iter())
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
