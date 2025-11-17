use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
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