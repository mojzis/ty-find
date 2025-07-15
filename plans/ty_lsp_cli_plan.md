# CLI Tool for Function Definition Finding using ty's LSP Server

## Project Overview

Build a command-line tool called `ty-find` that interfaces with ty's LSP server to provide go-to-definition functionality for Python functions, classes, and variables from the terminal.

## Architecture Design

### Core Components

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   CLI Tool      │◄──►│   LSP Client    │◄──►│  ty LSP Server  │
│   (ty-find)     │    │   (JSON-RPC)    │    │   (ty lsp)      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│  Argument       │    │  Protocol       │    │  Python         │
│  Parser         │    │  Handler        │    │  Workspace      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

### Technology Stack

- **Language**: Rust (for performance and ecosystem alignment with ty)
- **LSP Client**: Custom JSON-RPC client using `tokio` and `serde_json`
- **Process Management**: `tokio::process` for ty LSP server lifecycle
- **CLI Framework**: `clap` for argument parsing
- **Async Runtime**: Tokio for non-blocking operations

## Phase 1: Project Setup

### 1.1 Project Structure
```
ty-find/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── args.rs          # Command line argument parsing
│   │   └── output.rs        # Output formatting
│   ├── lsp/
│   │   ├── mod.rs
│   │   ├── client.rs        # LSP client implementation
│   │   ├── protocol.rs      # LSP protocol types
│   │   └── server.rs        # ty LSP server management
│   ├── workspace/
│   │   ├── mod.rs
│   │   ├── detection.rs     # Python workspace detection
│   │   └── navigation.rs    # Code navigation utilities
│   └── utils/
│       ├── mod.rs
│       ├── error.rs         # Error handling
│       └── config.rs        # Configuration management
├── tests/
│   ├── integration/
│   └── fixtures/
└── README.md
```

### 1.2 Dependencies (Cargo.toml)
```toml
[package]
name = "ty-find"
version = "0.1.0"
edition = "2021"
description = "CLI tool for finding Python function definitions using ty's LSP server"

[dependencies]
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
clap = { version = "4.0", features = ["derive"] }
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1.0", features = ["v4"] }
futures = "0.3"
thiserror = "1.0"

[dev-dependencies]
tempfile = "3.0"
assert_cmd = "2.0"
predicates = "3.0"
```

## Phase 2: LSP Client Implementation

### 2.1 LSP Protocol Types
```rust
// src/lsp/protocol.rs
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

#[derive(Serialize, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

#[derive(Serialize, Deserialize)]
pub struct TextDocumentPositionParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

#[derive(Serialize, Deserialize)]
pub struct GotoDefinitionParams {
    #[serde(flatten)]
    pub text_document_position_params: TextDocumentPositionParams,
    #[serde(rename = "workDoneToken", skip_serializing_if = "Option::is_none")]
    pub work_done_token: Option<String>,
    #[serde(rename = "partialResultToken", skip_serializing_if = "Option::is_none")]
    pub partial_result_token: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct LSPRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct LSPResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<LSPError>,
}

#[derive(Serialize, Deserialize)]
pub struct LSPError {
    pub code: i32,
    pub message: String,
}
```

### 2.2 ty LSP Server Management
```rust
// src/lsp/server.rs
use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use anyhow::Result;

pub struct TyLspServer {
    process: Child,
    workspace_root: String,
}

impl TyLspServer {
    pub async fn start(workspace_root: &str) -> Result<Self> {
        // Check if ty is available
        let ty_check = Command::new("ty")
            .arg("--version")
            .output()
            .await?;

        if !ty_check.status.success() {
            anyhow::bail!("ty is not installed or not available in PATH");
        }

        // Start ty LSP server
        let mut process = Command::new("ty")
            .arg("lsp")
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        Ok(Self {
            process,
            workspace_root: workspace_root.to_string(),
        })
    }

    pub fn stdin(&mut self) -> &mut tokio::process::ChildStdin {
        self.process.stdin.as_mut().unwrap()
    }

    pub fn stdout(&mut self) -> BufReader<tokio::process::ChildStdout> {
        BufReader::new(self.process.stdout.take().unwrap())
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.process.kill().await?;
        Ok(())
    }
}

impl Drop for TyLspServer {
    fn drop(&mut self) {
        let _ = self.process.start_kill();
    }
}
```

### 2.3 LSP Client Implementation
```rust
// src/lsp/client.rs
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::oneshot;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use serde_json::Value;
use anyhow::Result;

use crate::lsp::{protocol::*, server::TyLspServer};

pub struct TyLspClient {
    server: TyLspServer,
    request_id: AtomicU64,
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<LSPResponse>>>>,
}

impl TyLspClient {
    pub async fn new(workspace_root: &str) -> Result<Self> {
        let server = TyLspServer::start(workspace_root).await?;
        let client = Self {
            server,
            request_id: AtomicU64::new(1),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        };

        // Initialize LSP connection
        client.initialize(workspace_root).await?;
        Ok(client)
    }

    async fn initialize(&self, workspace_root: &str) -> Result<()> {
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "rootPath": workspace_root,
            "rootUri": format!("file://{}", workspace_root),
            "capabilities": {
                "textDocument": {
                    "definition": {
                        "dynamicRegistration": false,
                        "linkSupport": true
                    }
                }
            }
        });

        let response = self.send_request("initialize", init_params).await?;
        
        // Send initialized notification
        self.send_notification("initialized", serde_json::json!({})).await?;
        
        Ok(())
    }

    pub async fn goto_definition(&self, file_path: &str, line: u32, character: u32) -> Result<Vec<Location>> {
        let uri = format!("file://{}", std::fs::canonicalize(file_path)?.display());
        
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position { line, character },
            },
            work_done_token: None,
            partial_result_token: None,
        };

        let response = self.send_request("textDocument/definition", serde_json::to_value(params)?).await?;
        
        if let Some(result) = response.result {
            // Handle both single Location and array of Locations
            let locations: Vec<Location> = match result {
                Value::Array(arr) => serde_json::from_value(Value::Array(arr))?,
                Value::Object(_) => vec![serde_json::from_value(result)?],
                _ => vec![],
            };
            Ok(locations)
        } else {
            Ok(vec![])
        }
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<LSPResponse> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        // Store the pending request
        {
            let mut pending = self.pending_requests.lock().unwrap();
            pending.insert(id, tx);
        }

        let request = LSPRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::Number(id.into()),
            method: method.to_string(),
            params,
        };

        self.send_message(&request).await?;

        // Wait for response
        let response = rx.await?;
        Ok(response)
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        self.send_raw_message(&notification.to_string()).await
    }

    async fn send_message<T: serde::Serialize>(&self, message: &T) -> Result<()> {
        let content = serde_json::to_string(message)?;
        self.send_raw_message(&content).await
    }

    async fn send_raw_message(&self, content: &str) -> Result<()> {
        let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);
        self.server.stdin().write_all(message.as_bytes()).await?;
        self.server.stdin().flush().await?;
        Ok(())
    }

    // Response handler would run in background task
    pub async fn start_response_handler(&self) -> Result<()> {
        let mut stdout = self.server.stdout();
        let pending_requests = Arc::clone(&self.pending_requests);

        tokio::spawn(async move {
            let mut buffer = String::new();
            while let Ok(_) = stdout.read_line(&mut buffer).await {
                if buffer.starts_with("Content-Length:") {
                    // Parse LSP message
                    // Implementation details for parsing Content-Length and JSON response
                }
            }
        });

        Ok(())
    }
}
```

## Phase 3: CLI Interface Implementation

### 3.1 Command Line Arguments
```rust
// src/cli/args.rs
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ty-find")]
#[command(about = "Find Python function definitions using ty's LSP server")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    
    /// Workspace root directory (defaults to current directory)
    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,
    
    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,
    
    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Find definition of symbol at specific position
    Definition {
        /// Python file path
        file: PathBuf,
        
        /// Line number (1-based)
        #[arg(short, long)]
        line: u32,
        
        /// Column number (1-based)  
        #[arg(short, long)]
        column: u32,
    },
    
    /// Find definition by symbol name (searches in file)
    Find {
        /// Python file path
        file: PathBuf,
        
        /// Symbol name to find
        symbol: String,
    },
    
    /// Interactive mode - find definitions for multiple queries
    Interactive {
        /// Python file path
        file: Option<PathBuf>,
    },
}

#[derive(Clone, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable output
    Human,
    /// JSON output
    Json,
    /// CSV output  
    Csv,
    /// Just file paths
    Paths,
}
```

### 3.2 Symbol Detection
```rust
// src/workspace/navigation.rs
use std::fs;
use anyhow::Result;

pub struct SymbolFinder {
    content: String,
    lines: Vec<String>,
}

impl SymbolFinder {
    pub fn new(file_path: &str) -> Result<Self> {
        let content = fs::read_to_string(file_path)?;
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        
        Ok(Self { content, lines })
    }

    /// Find all occurrences of a symbol in the file
    pub fn find_symbol_positions(&self, symbol: &str) -> Vec<(u32, u32)> {
        let mut positions = Vec::new();
        
        for (line_idx, line) in self.lines.iter().enumerate() {
            let mut char_pos = 0;
            while let Some(pos) = line[char_pos..].find(symbol) {
                let actual_pos = char_pos + pos;
                
                // Check if this is a whole word match
                if self.is_whole_word_match(line, actual_pos, symbol) {
                    positions.push((line_idx as u32, actual_pos as u32));
                }
                
                char_pos = actual_pos + 1;
            }
        }
        
        positions
    }

    fn is_whole_word_match(&self, line: &str, pos: usize, symbol: &str) -> bool {
        let chars: Vec<char> = line.chars().collect();
        
        // Check character before
        if pos > 0 {
            let prev_char = chars[pos.saturating_sub(1)];
            if prev_char.is_alphanumeric() || prev_char == '_' {
                return false;
            }
        }
        
        // Check character after
        let end_pos = pos + symbol.len();
        if end_pos < chars.len() {
            let next_char = chars[end_pos];
            if next_char.is_alphanumeric() || next_char == '_' {
                return false;
            }
        }
        
        true
    }

    /// Get the line content at a specific line number
    pub fn get_line(&self, line_number: u32) -> Option<&String> {
        self.lines.get(line_number as usize)
    }
}
```

### 3.3 Output Formatting
```rust
// src/cli/output.rs
use crate::cli::args::OutputFormat;
use crate::lsp::protocol::Location;
use serde_json;
use std::path::Path;

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
            let line = location.range.start.line + 1; // Convert to 1-based
            let column = location.range.start.character + 1;

            output.push_str(&format!(
                "{}. {}:{}:{}\n",
                i + 1,
                file_path,
                line,
                column
            ));

            // Try to show the actual line content
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
```

## Phase 4: Main Application Logic

### 4.1 Main Entry Point
```rust
// src/main.rs
use clap::Parser;
use std::path::PathBuf;
use anyhow::Result;

mod cli;
mod lsp;
mod workspace;
mod utils;

use cli::args::{Cli, Commands};
use cli::output::OutputFormatter;
use lsp::client::TyLspClient;
use workspace::navigation::SymbolFinder;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("ty_find=debug")
            .init();
    }

    // Determine workspace root
    let workspace_root = cli.workspace
        .unwrap_or_else(|| std::env::current_dir().unwrap())
        .canonicalize()?;

    let formatter = OutputFormatter::new(cli.format);

    match cli.command {
        Commands::Definition { file, line, column } => {
            handle_definition_command(&workspace_root, &file, line, column, &formatter).await?;
        }
        Commands::Find { file, symbol } => {
            handle_find_command(&workspace_root, &file, &symbol, &formatter).await?;
        }
        Commands::Interactive { file } => {
            handle_interactive_command(&workspace_root, file, &formatter).await?;
        }
    }

    Ok(())
}

async fn handle_definition_command(
    workspace_root: &PathBuf,
    file: &PathBuf,
    line: u32,
    column: u32,
    formatter: &OutputFormatter,
) -> Result<()> {
    let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;
    
    // Convert to 0-based for LSP
    let locations = client.goto_definition(
        &file.to_string_lossy(),
        line.saturating_sub(1),
        column.saturating_sub(1),
    ).await?;

    let query_info = format!("{}:{}:{}", file.display(), line, column);
    println!("{}", formatter.format_definitions(&locations, &query_info));

    Ok(())
}

async fn handle_find_command(
    workspace_root: &PathBuf,
    file: &PathBuf,
    symbol: &str,
    formatter: &OutputFormatter,
) -> Result<()> {
    let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;
    let finder = SymbolFinder::new(&file.to_string_lossy())?;
    
    let positions = finder.find_symbol_positions(symbol);
    
    if positions.is_empty() {
        println!("Symbol '{}' not found in {}", symbol, file.display());
        return Ok(());
    }

    println!("Found {} occurrence(s) of '{}' in {}:\n", positions.len(), symbol, file.display());

    for (line, column) in positions {
        let locations = client.goto_definition(
            &file.to_string_lossy(),
            line,
            column,
        ).await?;

        if !locations.is_empty() {
            let query_info = format!("{}:{}:{}", file.display(), line + 1, column + 1);
            println!("{}", formatter.format_definitions(&locations, &query_info));
        }
    }

    Ok(())
}

async fn handle_interactive_command(
    workspace_root: &PathBuf,
    initial_file: Option<PathBuf>,
    formatter: &OutputFormatter,
) -> Result<()> {
    let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;
    
    println!("ty-find interactive mode");
    println!("Commands: <file>:<line>:<column>, find <file> <symbol>, quit");
    
    let stdin = std::io::stdin();
    let mut current_file = initial_file;

    loop {
        print!("> ");
        use std::io::Write;
        std::io::stdout().flush().unwrap();

        let mut input = String::new();
        stdin.read_line(&mut input)?;
        let input = input.trim();

        if input == "quit" || input == "q" {
            break;
        }

        // Parse different command formats
        if input.starts_with("find ") {
            // Handle "find <file> <symbol>" command
            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.len() >= 3 {
                let file = PathBuf::from(parts[1]);
                let symbol = parts[2];
                
                if let Err(e) = handle_find_command(workspace_root, &file, symbol, formatter).await {
                    eprintln!("Error: {}", e);
                }
            } else {
                eprintln!("Usage: find <file> <symbol>");
            }
        } else if let Some(pos) = input.rfind(':') {
            // Handle "<file>:<line>:<column>" format
            if let Some(second_pos) = input[..pos].rfind(':') {
                let file_part = &input[..second_pos];
                let line_part = &input[second_pos + 1..pos];
                let column_part = &input[pos + 1..];

                if let (Ok(line), Ok(column)) = (line_part.parse::<u32>(), column_part.parse::<u32>()) {
                    let file = PathBuf::from(file_part);
                    if let Err(e) = handle_definition_command(workspace_root, &file, line, column, formatter).await {
                        eprintln!("Error: {}", e);
                    }
                } else {
                    eprintln!("Invalid line or column number");
                }
            } else {
                eprintln!("Usage: <file>:<line>:<column>");
            }
        } else {
            eprintln!("Unknown command. Use: <file>:<line>:<column>, find <file> <symbol>, or quit");
        }
    }

    println!("Goodbye!");
    Ok(())
}
```

## Phase 5: Error Handling and Utilities

### 5.1 Error Types
```rust
// src/utils/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TyFindError {
    #[error("ty LSP server not found or failed to start")]
    TyNotAvailable,
    
    #[error("File not found: {path}")]
    FileNotFound { path: String },
    
    #[error("Invalid position: line {line}, column {column}")]
    InvalidPosition { line: u32, column: u32 },
    
    #[error("LSP communication error: {message}")]
    LspError { message: String },
    
    #[error("Workspace detection failed: {path}")]
    WorkspaceError { path: String },
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
```

### 5.2 Workspace Detection
```rust
// src/workspace/detection.rs
use std::path::{Path, PathBuf};
use anyhow::Result;

pub struct WorkspaceDetector;

impl WorkspaceDetector {
    /// Find the root of a Python workspace by looking for common markers
    pub fn find_workspace_root(start_path: &Path) -> Option<PathBuf> {
        let mut current = start_path;
        
        loop {
            // Check for Python project markers
            if Self::has_python_markers(current) {
                return Some(current.to_path_buf());
            }
            
            // Move up one directory
            if let Some(parent) = current.parent() {
                current = parent;
            } else {
                break;
            }
        }
        
        None
    }

    fn has_python_markers(path: &Path) -> bool {
        let markers = [
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
            "Pipfile",
            "poetry.lock",
            ".git",
            "src",
        ];

        markers.iter().any(|marker| path.join(marker).exists())
    }

    /// Check if ty is available and can be used
    pub async fn check_ty_availability() -> Result<String> {
        let output = tokio::process::Command::new("ty")
            .arg("--version")
            .output()
            .await?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            Ok(version.trim().to_string())
        } else {
            anyhow::bail!("ty is not available or failed to run")
        }
    }
}
```

## Phase 6: Testing Strategy

### 6.1 Integration Tests
```rust
// tests/integration/test_basic.rs
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_definition_command() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.py");
    
    fs::write(&test_file, r#"
def hello_world():
    return "Hello, World!"

def main():
    result = hello_world()
    print(result)
"#).unwrap();

    let mut cmd = Command::cargo_bin("ty-find").unwrap();
    cmd.arg("definition")
        .arg(&test_file)
        .arg("--line").arg("6")
        .arg("--column").arg("15")
        .arg("--workspace").arg(temp_dir.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("hello_world"));
}

#[tokio::test]
async fn test_find_command() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.py");
    
    fs::write(&test_file, r#"
class Calculator:
    def add(self, a, b):
        return a + b
    
    def multiply(self, a, b):
        return a * b

calc = Calculator()
result = calc.add(1, 2)
"#).unwrap();

    let mut cmd = Command::cargo_bin("ty-find").unwrap();
    cmd.arg("find")
        .arg(&test_file)
        .arg("add")
        .arg("--workspace").arg(temp_dir.path());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("def add"));
}

#[test]
fn test_json_output() {
    let mut cmd = Command::cargo_bin("ty-find").unwrap();
    cmd.arg("definition")
        .arg("nonexistent.py")
        .arg("--line").arg("1")
        .arg("--column").arg("1")
        .arg("--format").arg("json");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}
```

### 6.2 Unit Tests
```rust
// src/workspace/navigation.rs (add tests)
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_symbol_finder() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "def test_function():").unwrap();
        writeln!(temp_file, "    return test_function()").unwrap();
        writeln!(temp_file, "").unwrap();
        writeln!(temp_file, "result = test_function()").unwrap();

        let finder = SymbolFinder::new(temp_file.path().to_str().unwrap()).unwrap();
        let positions = finder.find_symbol_positions("test_function");

        assert_eq!(positions.len(), 3);
        assert_eq!(positions[0], (0, 4));  // def test_function():
        assert_eq!(positions[1], (1, 11)); //     return test_function()
        assert_eq!(positions[2], (3, 9));  // result = test_function()
    }
}
```

## Phase 7: Installation and Distribution

### 7.1 Installation Script
```bash
#!/bin/bash
# install.sh

set -e

echo "Installing ty-find..."

# Check if Rust is installed
if ! command -v rustc &> /dev/null; then
    echo "Error: Rust is not installed. Please install Rust first:"
    echo "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Check if ty is installed
if ! command -v ty &> /dev/null; then
    echo "Error: ty is not installed. Please install ty first:"
    echo "pip install ty"
    exit 1
fi

# Clone and build
git clone https://github.com/user/ty-find.git
cd ty-find
cargo build --release

# Install to user's local bin
mkdir -p ~/.local/bin
cp target/release/ty-find ~/.local/bin/

echo "ty-find installed successfully!"
echo "Make sure ~/.local/bin is in your PATH"
```

### 7.2 Package Configuration
```toml
# Cargo.toml additions for distribution
[package.metadata.wix]
upgrade-guid = "..." # Generate UUID
path-guid = "..."    # Generate UUID
license = "false"

[[bin]]
name = "ty-find"
path = "src/main.rs"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

## Phase 8: Documentation and Examples

### 8.1 README.md
```markdown
# ty-find

A command-line tool for finding Python function definitions using ty's LSP server.

## Installation

### Prerequisites
- [ty](https://github.com/astral-sh/ty) type checker installed
- Rust toolchain (for building from source)

### From Source
```bash
git clone https://github.com/user/ty-find.git
cd ty-find
cargo install --path .
```

## Usage

### Find definition at specific position
```bash
ty-find definition myfile.py --line 10 --column 5
```

### Find all definitions of a symbol
```bash
ty-find find myfile.py function_name
```

### Interactive mode
```bash
ty-find interactive
> myfile.py:10:5
> find myfile.py function_name
> quit
```

### Output formats
```bash
ty-find definition myfile.py -l 10 -c 5 --format json
ty-find definition myfile.py -l 10 -c 5 --format csv
ty-find definition myfile.py -l 10 -c 5 --format paths
```

## Examples

### Basic usage
```bash
# Find where 'calculate_total' is defined
ty-find find src/calculator.py calculate_total

# Find definition at line 25, column 10
ty-find definition src/main.py --line 25 --column 10
```

### With workspace specification
```bash
ty-find definition src/app.py -l 15 -c 8 --workspace /path/to/project
```
```

### 8.2 Usage Examples
```bash
# Example 1: Find function definition
$ ty-find definition calculator.py --line 10 --column 5
Found 1 definition(s) for: calculator.py:10:5

1. calculator.py:3:5
   def calculate_sum(a, b):

# Example 2: Find all references to a symbol
$ ty-find find calculator.py calculate_sum
Found 3 occurrence(s) of 'calculate_sum' in calculator.py:

Found 1 definition(s) for: calculator.py:3:5
1. calculator.py:3:5
   def calculate_sum(a, b):

Found 1 definition(s) for: calculator.py:10:12
1. calculator.py:3:5
   def calculate_sum(a, b):

# Example 3: JSON output
$ ty-find definition calculator.py -l 10 -c 5 --format json
[
  {
    "uri": "file:///path/to/calculator.py",
    "range": {
      "start": {
        "line": 2,
        "character": 4
      },
      "end": {
        "start": {
          "line": 2,
          "character": 18
        }
      }
    }
  }
]
```

## Timeline and Milestones

### Week 1-2: Foundation
- [ ] Project setup and basic structure
- [ ] LSP protocol types and client scaffolding
- [ ] ty LSP server management

### Week 3-4: Core Functionality
- [ ] LSP client implementation with go-to-definition
- [ ] Basic CLI argument parsing
- [ ] Symbol finding in Python files

### Week 5-6: Advanced Features
- [ ] Interactive mode
- [ ] Multiple output formats
- [ ] Workspace detection and management

### Week 7-8: Polish and Testing
- [ ] Comprehensive test suite
- [ ] Error handling and user experience
- [ ] Documentation and examples

### Week 9-10: Distribution
- [ ] Installation scripts and packaging
- [ ] CI/CD pipeline for releases
- [ ] Community feedback integration

## Success Criteria

- **Functionality**: Successfully find definitions using ty's LSP server
- **Performance**: Sub-second response times for typical operations
- **Usability**: Clear CLI interface and helpful error messages
- **Reliability**: Robust error handling and graceful failure modes
- **Integration**: Seamless interaction with ty's LSP capabilities

This tool will provide Python developers with a fast, command-line interface to leverage ty's powerful semantic analysis capabilities for code navigation and exploration.