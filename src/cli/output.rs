use crate::cli::args::OutputFormat;
use crate::lsp::protocol::Location;
use serde_json;

pub struct OutputFormatter {
    format: OutputFormat,
}

impl OutputFormatter {
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
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

        let mut output = format!("Found {} definition(s) for: {}\n\n", locations.len(), query_info);

        for (i, location) in locations.iter().enumerate() {
            let file_path = self.uri_to_path(&location.uri);
            let line = location.range.start.line + 1;
            let column = location.range.start.character + 1;

            output.push_str(&format!(
                "{}. {}:{}:{}\n",
                i + 1,
                file_path,
                line,
                column
            ));

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
        if uri.starts_with("file://") {
            uri[7..].to_string()
        } else {
            uri.to_string()
        }
    }
}