use crate::cli::args::OutputFormat;
use crate::lsp::protocol::{
    DocumentSymbol, Hover, HoverContents, Location, MarkedStringOrString, SymbolInformation,
};
use serde_json;
use std::path::{Path, PathBuf};

pub struct OutputFormatter {
    format: OutputFormat,
    cwd: PathBuf,
}

impl OutputFormatter {
    pub fn new(format: OutputFormat) -> Self {
        Self {
            format,
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
        }
    }

    pub fn format_definitions(&self, locations: &[Location], query_info: &str) -> String {
        match self.format {
            OutputFormat::Human => self.format_human(locations, query_info),
            OutputFormat::Json => self.format_json(locations),
            OutputFormat::Csv => self.format_csv(locations),
            OutputFormat::Paths => self.format_paths(locations),
        }
    }

    fn format_human(&self, locations: &[Location], query_info: &str) -> String {
        if locations.is_empty() {
            return format!("No definitions found for: {}", query_info);
        }

        let mut output = format!(
            "Found {} definition(s) for: {}\n\n",
            locations.len(),
            query_info
        );

        for (i, location) in locations.iter().enumerate() {
            let file_path = self.uri_to_path(&location.uri);
            let line = location.range.start.line + 1;
            let column = location.range.start.character + 1;

            output.push_str(&format!("{}. {}:{}:{}\n", i + 1, file_path, line, column));

            if let Ok(content) = std::fs::read_to_string(&file_path) {
                let lines: Vec<&str> = content.lines().collect();
                if let Some(line_content) = lines.get((line - 1) as usize) {
                    output.push_str(&format!("   {}\n", line_content.trim()));
                }
            }
            output.push('\n');
        }

        output
    }

    fn format_json(&self, locations: &[Location]) -> String {
        serde_json::to_string_pretty(locations).unwrap_or_else(|_| "[]".to_string())
    }

    fn format_csv(&self, locations: &[Location]) -> String {
        let mut output = String::from("file,line,column\n");
        for location in locations {
            let file_path = self.uri_to_path(&location.uri);
            let line = location.range.start.line + 1;
            let column = location.range.start.character + 1;
            output.push_str(&format!("{},{},{}\n", file_path, line, column));
        }
        output
    }

    fn format_paths(&self, locations: &[Location]) -> String {
        locations
            .iter()
            .map(|loc| self.uri_to_path(&loc.uri))
            .collect::<Vec<_>>()
            .join("\n")
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

    pub fn format_references(&self, locations: &[Location], query_info: &str) -> String {
        match self.format {
            OutputFormat::Human => {
                if locations.is_empty() {
                    return format!("No references found for: {}", query_info);
                }

                let mut output = format!(
                    "Found {} reference(s) for: {}\n\n",
                    locations.len(),
                    query_info
                );

                for (i, location) in locations.iter().enumerate() {
                    let file_path = self.uri_to_path(&location.uri);
                    let line = location.range.start.line + 1;
                    let column = location.range.start.character + 1;

                    output.push_str(&format!("{}. {}:{}:{}\n", i + 1, file_path, line, column));

                    if let Ok(content) = std::fs::read_to_string(&file_path) {
                        let lines: Vec<&str> = content.lines().collect();
                        if let Some(line_content) = lines.get((line - 1) as usize) {
                            output.push_str(&format!("   {}\n", line_content.trim()));
                        }
                    }
                    output.push('\n');
                }

                output
            }
            OutputFormat::Json => self.format_json(locations),
            OutputFormat::Csv => self.format_csv(locations),
            OutputFormat::Paths => self.format_paths(locations),
        }
    }

    pub fn format_hover(&self, hover: &Hover, query_info: &str) -> String {
        match self.format {
            OutputFormat::Human => {
                let mut output = format!("Hover information for: {}\n\n", query_info);

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

                    output.push_str(&format!(
                        "{}. {} ({:?})\n   {}:{}:{}\n\n",
                        i + 1,
                        symbol.name,
                        symbol.kind,
                        file_path,
                        line,
                        column
                    ));
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
                    output.push_str(&format!(
                        "{},{:?},{},{},{}\n",
                        symbol.name, symbol.kind, file_path, line, column
                    ));
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
                self.format_document_symbols_recursive(symbols, 0, &mut output);
                output
            }
            OutputFormat::Json => {
                serde_json::to_string_pretty(symbols).unwrap_or_else(|_| "[]".to_string())
            }
            OutputFormat::Csv => {
                let mut output = String::from("name,kind,line,column\n");
                self.format_document_symbols_csv(symbols, &mut output);
                output
            }
            OutputFormat::Paths => {
                // Paths format doesn't make sense for document symbols, fall back to human
                let mut output = String::new();
                self.format_document_symbols_recursive(symbols, 0, &mut output);
                output
            }
        }
    }

    #[allow(clippy::only_used_in_recursion)]
    fn format_document_symbols_recursive(
        &self,
        symbols: &[DocumentSymbol],
        indent: usize,
        output: &mut String,
    ) {
        for symbol in symbols {
            let line = symbol.range.start.line + 1;
            let column = symbol.range.start.character + 1;
            let indent_str = "  ".repeat(indent);

            output.push_str(&format!(
                "{}{} ({:?}) - line {}, col {}\n",
                indent_str, symbol.name, symbol.kind, line, column
            ));

            if let Some(children) = &symbol.children {
                self.format_document_symbols_recursive(children, indent + 1, output);
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

    #[allow(clippy::only_used_in_recursion)]
    fn format_document_symbols_csv(&self, symbols: &[DocumentSymbol], output: &mut String) {
        for symbol in symbols {
            let line = symbol.range.start.line + 1;
            let column = symbol.range.start.character + 1;

            output.push_str(&format!(
                "{},{:?},{},{}\n",
                symbol.name, symbol.kind, line, column
            ));

            if let Some(children) = &symbol.children {
                self.format_document_symbols_csv(children, output);
            }
        }
    }
}
