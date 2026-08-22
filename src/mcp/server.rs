//! The `tyf mcp` server: five tools, each a thin translation of a CLI command.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::service::ServerInitializeError;
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::cli::args::{OutputDetail, OutputFormat};
use crate::cli::classify_error;
use crate::cli::output::OutputFormatter;
use crate::cli::style::Styler;
use crate::commands::{self, CommandOutput, RefsOptions, ShowOptions};

/// The MCP revision this server implements.
const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::V_2026_07_28;

/// Reference cap for `show`, matching the CLI's `--references-limit` default so
/// both frontends render the same text for the same query.
const REFERENCES_LIMIT: usize = 20;

/// One line, because it is injected into the agent's context alongside the
/// tool schemas.
const INSTRUCTIONS: &str =
    "Type-aware Python navigation by symbol name — no file paths or line numbers needed. \
     Batch several symbols into one call.";

/// Render a handler outcome as a tool result.
///
/// A handler error becomes a *tool* error (`isError: true`) carrying the same
/// message the CLI prints to stderr — a bad invocation or an unreachable
/// daemon is the caller's problem to see, not a protocol fault. A query that
/// simply matched nothing is a success with the "not found" text, matching the
/// CLI's exit 0.
fn tool_result(outcome: Result<CommandOutput>) -> CallToolResult {
    match outcome {
        Ok(output) => {
            CallToolResult::success(vec![ContentBlock::text(normalize(&output.combined()))])
        }
        Err(error) => CallToolResult::error(vec![ContentBlock::text(error_text(&error))]),
    }
}

/// Trailing newlines are the one difference between a terminal write and a
/// text content block; strip them so tool output is otherwise byte-identical
/// to the CLI's.
fn normalize(text: &str) -> String {
    text.trim_end().to_string()
}

/// The message the CLI would print for this error, without the ANSI styling.
fn error_text(error: &anyhow::Error) -> String {
    classify_error(error).message
}

/// The CLI's arg parser rejects an invocation with no symbols. An empty array
/// over MCP is the same mistake, so it gets the same kind of usage error
/// rather than a successful, empty result.
fn reject_empty_batch(values: &[String], field: &str) -> Option<CallToolResult> {
    values.is_empty().then(|| {
        CallToolResult::error(vec![ContentBlock::text(format!(
            "tyf: '{field}' must list at least one entry"
        ))])
    })
}

/// Symbol names, or `file:line:col` positions where a tool accepts them.
type Queries = Vec<String>;

/// Parameters for the `show` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShowParams {
    /// Symbol names. `Class.member` narrows to one class member.
    pub symbols: Queries,
    /// Narrow to this file, relative to the workspace root or absolute.
    #[serde(default)]
    pub file: Option<String>,
    /// List individual usage locations, not just the count.
    #[serde(default)]
    pub references: bool,
    /// Include the docstring.
    #[serde(default)]
    pub doc: bool,
    /// Everything: docstring, usages, and test usages.
    #[serde(default)]
    pub all: bool,
}

/// Parameters for the `find` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindParams {
    /// Symbol names. `Class.member` narrows to one class member.
    pub symbols: Queries,
    /// Narrow to this file, relative to the workspace root or absolute.
    #[serde(default)]
    pub file: Option<String>,
    /// Match by prefix instead of exact name.
    #[serde(default)]
    pub fuzzy: bool,
}

/// Parameters for the `refs` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RefsParams {
    /// Symbol names or `file:line:col` positions, auto-detected per entry.
    pub queries: Queries,
    /// Narrow symbol lookups to this file, relative to the workspace root.
    #[serde(default)]
    pub file: Option<String>,
    /// Count the declaration itself as a usage. Default false (the CLI's
    /// `--include-declaration` defaults to true).
    #[serde(default)]
    pub include_declaration: bool,
}

/// Parameters for the `members` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MembersParams {
    /// Class names.
    pub symbols: Queries,
    /// Narrow to this file, relative to the workspace root or absolute.
    #[serde(default)]
    pub file: Option<String>,
    /// Include dunder and private members.
    #[serde(default)]
    pub all: bool,
}

/// Parameters for the `list` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListParams {
    /// Python file to outline, relative to the workspace root or absolute.
    pub file: String,
}

/// The MCP frontend: one workspace, one daemon connection path, five tools.
#[derive(Clone)]
pub struct TyFindMcpServer {
    workspace_root: PathBuf,
    /// The condensed, uncoloured renderer, identical to what
    /// `tyf --detail condensed --color never` uses.
    formatter: OutputFormatter,
    timeout: Duration,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl TyFindMcpServer {
    /// Build a server bound to an already-resolved workspace root.
    pub fn new(workspace_root: PathBuf, timeout: Duration) -> Self {
        let formatter = OutputFormatter::with_detail(
            OutputFormat::Human,
            OutputDetail::Condensed,
            Styler::no_color(),
        );
        Self { workspace_root, formatter, timeout, tool_router: Self::tool_router() }
    }

    #[tool(description = "Where Python symbols are defined, their type signature, and how often \
                       they are used. Pass several symbols in one call.")]
    async fn show(
        &self,
        Parameters(params): Parameters<ShowParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(error) = reject_empty_batch(&params.symbols, "symbols") {
            return Ok(error);
        }
        let file = params.file.map(PathBuf::from);
        Ok(tool_result(
            commands::handle_show_command(
                &self.workspace_root,
                file.as_deref(),
                &params.symbols,
                &self.formatter,
                ShowOptions {
                    references: params.references || params.all,
                    references_limit: REFERENCES_LIMIT,
                    tests: params.all,
                    doc: params.doc || params.all,
                },
                self.timeout,
                None,
            )
            .await,
        ))
    }

    #[tool(
        description = "Locate where Python symbols are defined. Pass several symbols in one call."
    )]
    async fn find(
        &self,
        Parameters(params): Parameters<FindParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(error) = reject_empty_batch(&params.symbols, "symbols") {
            return Ok(error);
        }
        let file = params.file.map(PathBuf::from);
        Ok(tool_result(
            commands::handle_find_command(
                &self.workspace_root,
                file.as_deref(),
                &params.symbols,
                params.fuzzy,
                &self.formatter,
                self.timeout,
                None,
            )
            .await,
        ))
    }

    #[tool(description = "Every usage of Python symbols across the project. Pass several queries \
                       in one call.")]
    async fn refs(
        &self,
        Parameters(params): Parameters<RefsParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(error) = reject_empty_batch(&params.queries, "queries") {
            return Ok(error);
        }
        let file = params.file.map(PathBuf::from);
        Ok(tool_result(
            commands::handle_references_command(
                &self.workspace_root,
                file.as_deref(),
                &params.queries,
                &self.formatter,
                RefsOptions {
                    position: None,
                    read_stdin: false,
                    include_declaration: params.include_declaration,
                    references_limit: REFERENCES_LIMIT,
                    tests: false,
                },
                self.timeout,
                None,
            )
            .await,
        ))
    }

    #[tool(
        description = "Public interface of Python classes: methods, properties, class variables. \
                       Pass several classes in one call."
    )]
    async fn members(
        &self,
        Parameters(params): Parameters<MembersParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(error) = reject_empty_batch(&params.symbols, "symbols") {
            return Ok(error);
        }
        let file = params.file.map(PathBuf::from);
        Ok(tool_result(
            commands::handle_members_command(
                &self.workspace_root,
                file.as_deref(),
                &params.symbols,
                params.all,
                &self.formatter,
                self.timeout,
                None,
            )
            .await,
        ))
    }

    #[tool(description = "Outline one Python file: every function, class, and variable in it.")]
    async fn list(
        &self,
        Parameters(params): Parameters<ListParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(tool_result(
            commands::handle_document_symbols_command(
                &self.workspace_root,
                Path::new(&params.file),
                &self.formatter,
                self.timeout,
                None,
            )
            .await,
        ))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TyFindMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(PROTOCOL_VERSION)
            .with_server_info(Implementation::new("tyf", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// Serve MCP on stdio until stdin closes, then exit.
///
/// The daemon is left running on exit: its idle timeout retires it, exactly as
/// after CLI use.
pub async fn serve_stdio(workspace_root: PathBuf, timeout: Duration) -> Result<()> {
    let server = TyFindMcpServer::new(workspace_root, timeout);
    let running = match server.serve(rmcp::transport::stdio()).await {
        Ok(running) => running,
        // A harness that closes stdin before initializing has simply gone away;
        // that is the documented shutdown signal, not a startup failure.
        Err(ServerInitializeError::ConnectionClosed(_) | ServerInitializeError::Cancelled) => {
            return Ok(())
        }
        Err(e) => return Err(anyhow::Error::new(e).context("Failed to start the MCP server")),
    };
    running.waiting().await.context("MCP server stopped unexpectedly")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::UsageError;

    /// The tool table this server exposes.
    fn tools() -> Vec<rmcp::model::Tool> {
        TyFindMcpServer::tool_router().list_all()
    }

    /// Ceiling on the serialized `tools/list` response, in characters.
    ///
    /// Tool definitions are injected into an agent's context every session, so
    /// schema bloat is a product regression rather than a style nit. ~4000
    /// chars stands in for the ~1000-token budget. If this fails, tighten the
    /// descriptions — do not raise the ceiling.
    const TOOL_LIST_CHAR_CEILING: usize = 4000;

    fn serialized_tool_list() -> String {
        let tools = tools();
        let result = rmcp::model::ListToolsResult::with_all_items(tools);
        serde_json::to_string(&result).expect("tools/list must serialize")
    }

    #[test]
    fn tool_list_stays_within_the_token_budget() {
        let serialized = serialized_tool_list();
        let size = serialized.chars().count();
        assert!(
            size <= TOOL_LIST_CHAR_CEILING,
            "serialized tools/list is {size} chars, over the {TOOL_LIST_CHAR_CEILING} ceiling:\n{serialized}"
        );
    }

    #[test]
    fn exposes_exactly_the_five_cli_mirroring_tools() {
        let mut names: Vec<String> = tools().iter().map(|t| t.name.to_string()).collect();
        names.sort();
        assert_eq!(names, vec!["find", "list", "members", "refs", "show"]);
    }

    #[test]
    fn every_tool_has_a_description_and_no_output_schema() {
        for tool in tools() {
            let description = tool.description.as_deref().unwrap_or_default();
            assert!(!description.is_empty(), "tool '{}' needs a description", tool.name);
            assert!(
                tool.output_schema.is_none(),
                "tool '{}' must not declare an outputSchema — results are text only",
                tool.name
            );
        }
    }

    #[test]
    fn tool_descriptions_tell_the_agent_to_batch() {
        for tool in tools() {
            if tool.name == "list" {
                continue; // single-file tool; nothing to batch
            }
            let description = tool.description.as_deref().unwrap_or_default();
            assert!(
                description.contains("in one call"),
                "tool '{}' should tell the agent to batch, got: {description}",
                tool.name
            );
        }
    }

    #[test]
    fn advertises_the_stateless_protocol_revision() {
        let server = TyFindMcpServer::new(PathBuf::from("/tmp"), Duration::from_secs(30));
        assert_eq!(server.get_info().protocol_version, ProtocolVersion::V_2026_07_28);
    }

    #[test]
    fn usage_errors_become_tool_errors_with_the_cli_message() {
        let error = anyhow::Error::from(UsageError("tyf: bad dotted token".to_string()));
        assert_eq!(error_text(&error), "tyf: bad dotted token");
    }

    #[test]
    fn other_errors_keep_the_cli_error_prefix_and_chain() {
        let error = anyhow::anyhow!("root cause").context("outer");
        assert_eq!(error_text(&error), "Error: outer\n  Caused by: root cause");
    }

    #[test]
    fn an_empty_batch_is_rejected_like_the_cli_rejects_no_symbols() {
        let error = reject_empty_batch(&[], "symbols").expect("empty batch must be rejected");
        assert_eq!(error.is_error, Some(true));
    }

    #[test]
    fn a_non_empty_batch_is_not_rejected() {
        assert!(reject_empty_batch(&["MyClass".to_string()], "symbols").is_none());
    }

    #[test]
    fn normalize_strips_only_trailing_whitespace() {
        assert_eq!(normalize("a\nb\n\n"), "a\nb");
        assert_eq!(normalize("  leading kept\n"), "  leading kept");
    }
}
