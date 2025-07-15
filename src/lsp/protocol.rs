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