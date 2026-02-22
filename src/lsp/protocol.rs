use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

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

#[derive(Serialize, Deserialize, Debug)]
pub struct LSPError {
    pub code: i32,
    pub message: String,
}

// Hover support
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Hover {
    pub contents: HoverContents,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum HoverContents {
    Markup(MarkupContent),
    MarkedString(MarkedString),
    Array(Vec<MarkedStringOrString>),
    Scalar(String),
}

/// A `MarkedString` is either a plain string or a language-tagged code block.
/// LSP spec: `MarkedString` = string | { language: string; value: string }
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarkedString {
    pub language: String,
    pub value: String,
}

/// Represents either a plain string or a `MarkedString` object in arrays.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum MarkedStringOrString {
    MarkedString(MarkedString),
    String(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarkupContent {
    pub kind: MarkupKind,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum MarkupKind {
    PlainText,
    Markdown,
}

// Symbol support
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SymbolInformation {
    pub name: String,
    pub kind: SymbolKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<SymbolTag>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    pub location: Location,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "containerName")]
    pub container_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DocumentSymbol {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub kind: SymbolKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<SymbolTag>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    pub range: Range,
    #[serde(rename = "selectionRange")]
    pub selection_range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Self>>,
}

#[derive(Serialize_repr, Deserialize_repr, Clone, Debug)]
#[repr(u8)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

#[derive(Serialize_repr, Deserialize_repr, Clone, Debug)]
#[repr(u8)]
pub enum SymbolTag {
    Deprecated = 1,
}

// Hover request params
#[derive(Serialize, Deserialize)]
pub struct HoverParams {
    #[serde(flatten)]
    pub text_document_position_params: TextDocumentPositionParams,
    #[serde(rename = "workDoneToken", skip_serializing_if = "Option::is_none")]
    pub work_done_token: Option<String>,
}

// Workspace symbols request params
#[derive(Serialize, Deserialize)]
pub struct WorkspaceSymbolParams {
    pub query: String,
    #[serde(rename = "workDoneToken", skip_serializing_if = "Option::is_none")]
    pub work_done_token: Option<String>,
    #[serde(rename = "partialResultToken", skip_serializing_if = "Option::is_none")]
    pub partial_result_token: Option<String>,
}

// References request params
#[derive(Serialize, Deserialize)]
pub struct ReferenceParams {
    #[serde(flatten)]
    pub text_document_position_params: TextDocumentPositionParams,
    pub context: ReferenceContext,
    #[serde(rename = "workDoneToken", skip_serializing_if = "Option::is_none")]
    pub work_done_token: Option<String>,
    #[serde(rename = "partialResultToken", skip_serializing_if = "Option::is_none")]
    pub partial_result_token: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ReferenceContext {
    #[serde(rename = "includeDeclaration")]
    pub include_declaration: bool,
}

// Document symbols request params
#[derive(Serialize, Deserialize)]
pub struct DocumentSymbolParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    #[serde(rename = "workDoneToken", skip_serializing_if = "Option::is_none")]
    pub work_done_token: Option<String>,
    #[serde(rename = "partialResultToken", skip_serializing_if = "Option::is_none")]
    pub partial_result_token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_kind_deserialize_from_integer() {
        let json = r"12";
        let kind: SymbolKind = serde_json::from_str(json).unwrap();
        assert!(matches!(kind, SymbolKind::Function));
    }

    #[test]
    fn test_symbol_kind_serialize_to_integer() {
        let json = serde_json::to_string(&SymbolKind::Function).unwrap();
        assert_eq!(json, "12");
    }

    #[test]
    fn test_symbol_information_with_integer_kind() {
        let json = r#"{
            "name": "calculate_sum",
            "kind": 12,
            "location": {
                "uri": "file:///test.py",
                "range": {
                    "start": {"line": 3, "character": 4},
                    "end": {"line": 3, "character": 17}
                }
            }
        }"#;
        let info: SymbolInformation = serde_json::from_str(json).unwrap();
        assert_eq!(info.name, "calculate_sum");
        assert!(matches!(info.kind, SymbolKind::Function));
    }

    #[test]
    fn test_document_symbol_with_integer_kind() {
        let json = r#"{
            "name": "Animal",
            "kind": 5,
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 10, "character": 0}
            },
            "selectionRange": {
                "start": {"line": 0, "character": 6},
                "end": {"line": 0, "character": 12}
            },
            "children": [
                {
                    "name": "__init__",
                    "kind": 6,
                    "range": {
                        "start": {"line": 2, "character": 4},
                        "end": {"line": 4, "character": 0}
                    },
                    "selectionRange": {
                        "start": {"line": 2, "character": 8},
                        "end": {"line": 2, "character": 16}
                    }
                }
            ]
        }"#;
        let symbol: DocumentSymbol = serde_json::from_str(json).unwrap();
        assert_eq!(symbol.name, "Animal");
        assert!(matches!(symbol.kind, SymbolKind::Class));
        let children = symbol.children.unwrap();
        assert_eq!(children.len(), 1);
        assert!(matches!(children[0].kind, SymbolKind::Method));
    }

    #[test]
    fn test_symbol_tag_deserialize_from_integer() {
        let json = r"[1]";
        let tags: Vec<SymbolTag> = serde_json::from_str(json).unwrap();
        assert_eq!(tags.len(), 1);
        assert!(matches!(tags[0], SymbolTag::Deprecated));
    }

    #[test]
    fn test_hover_contents_markup() {
        let json = r#"{"kind": "markdown", "value": "```python\ndef foo(): ...\n```"}"#;
        let contents: HoverContents = serde_json::from_str(json).unwrap();
        match contents {
            HoverContents::Markup(markup) => {
                assert!(matches!(markup.kind, MarkupKind::Markdown));
                assert_eq!(markup.value, "```python\ndef foo(): ...\n```");
            }
            other => panic!("Expected Markup, got {other:?}"),
        }
    }

    #[test]
    fn test_hover_contents_marked_string() {
        let json = r#"{"language": "python", "value": "def foo(): ..."}"#;
        let contents: HoverContents = serde_json::from_str(json).unwrap();
        match contents {
            HoverContents::MarkedString(ms) => {
                assert_eq!(ms.language, "python");
                assert_eq!(ms.value, "def foo(): ...");
            }
            other => panic!("Expected MarkedString, got {other:?}"),
        }
    }

    #[test]
    fn test_hover_contents_scalar() {
        let json = r#""some hover text""#;
        let contents: HoverContents = serde_json::from_str(json).unwrap();
        match contents {
            HoverContents::Scalar(s) => assert_eq!(s, "some hover text"),
            other => panic!("Expected Scalar, got {other:?}"),
        }
    }

    #[test]
    fn test_hover_contents_array_mixed() {
        let json = r#"[{"language": "python", "value": "def foo(): ..."}, "plain text"]"#;
        let contents: HoverContents = serde_json::from_str(json).unwrap();
        match contents {
            HoverContents::Array(arr) => {
                assert_eq!(arr.len(), 2);
                let MarkedStringOrString::MarkedString(ms) = &arr[0] else {
                    panic!("Expected MarkedString, got {:?}", arr[0]);
                };
                assert_eq!(ms.language, "python");
                assert_eq!(ms.value, "def foo(): ...");

                let MarkedStringOrString::String(s) = &arr[1] else {
                    panic!("Expected String, got {:?}", arr[1]);
                };
                assert_eq!(s, "plain text");
            }
            other => panic!("Expected Array, got {other:?}"),
        }
    }
}
