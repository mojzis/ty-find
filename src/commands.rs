use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[cfg(unix)]
use crate::cli::args::DaemonCommands;
use crate::cli::output::{
    find_enclosing_symbol, EnrichedReference, EnrichedReferencesResult, OutputFormatter, ShowEntry,
    SourceCache,
};
#[cfg(unix)]
use crate::daemon::client::{ensure_daemon_running, spawn_daemon, DaemonClient, CLIENT_VERSION};
#[cfg(unix)]
use crate::daemon::protocol::BatchReferencesQuery;
#[cfg(unix)]
use crate::daemon::server::DaemonServer;
use crate::debug::DebugLog;
use crate::lsp::client::TyLspClient;
use crate::lsp::protocol::{DocumentSymbol, Location};
use crate::workspace::navigation::SymbolFinder;

/// Helper: connect to the daemon and attach the debug log if present.
#[cfg(unix)]
async fn connect_daemon(
    timeout: Duration,
    debug_log: Option<&Arc<DebugLog>>,
) -> Result<DaemonClient> {
    let mut client = DaemonClient::connect_with_timeout(timeout).await?;
    if let Some(log) = debug_log {
        let socket_path = crate::daemon::client::get_socket_path()?;
        log.log_daemon_connection(&socket_path.to_string_lossy(), true, None);

        // Log daemon version info via a quick ping
        if let Ok(ping) = client.ping().await {
            log.log_daemon_version(&ping.version, crate::daemon::client::CLIENT_VERSION);
        }

        client.set_debug_log(Arc::clone(log));
    }
    Ok(client)
}

/// Check whether a file URI corresponds to a Python test file.
///
/// Matches common Python test conventions:
/// - Filename: `test_*.py` or `*_test.py`
/// - Filename: `conftest.py`
/// - Any file under a `tests/` directory segment
fn is_test_file(uri: &str) -> bool {
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let p = std::path::Path::new(path);
    let is_py = p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("py"));
    let file_stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if file_name == "conftest.py" {
        return true;
    }
    if is_py && file_stem.starts_with("test_") {
        return true;
    }
    if is_py && file_stem.ends_with("_test") {
        return true;
    }

    // Check for tests/ directory in the path
    path.split('/').any(|segment| segment == "tests")
}

/// Partition locations into `(non_test, test)` based on file URI heuristics.
fn partition_test_locations(locations: Vec<Location>) -> (Vec<Location>, Vec<Location>) {
    let mut non_test = Vec::new();
    let mut test = Vec::new();
    for loc in locations {
        if is_test_file(&loc.uri) {
            test.push(loc);
        } else {
            non_test.push(loc);
        }
    }
    (non_test, test)
}

/// Deduplicate locations by (uri, start line).
fn dedup_locations(locations: &mut Vec<Location>) {
    let mut seen = HashSet::new();
    locations.retain(|loc| seen.insert((loc.uri.clone(), loc.range.start.line)));
}

/// Count unique files in a slice of locations.
fn count_unique_files(locations: &[Location]) -> usize {
    let files: HashSet<&str> = locations.iter().map(|loc| loc.uri.as_str()).collect();
    files.len()
}

/// Enrich a set of locations with enclosing symbol context.
///
/// For each unique file URI in `locations`, fetches document symbols via the daemon
/// and walks the symbol tree to find the tightest enclosing symbol for each reference.
/// Falls back to "module scope" when no enclosing symbol is found or when the
/// documentSymbol call fails.
#[cfg(unix)]
async fn enrich_references(
    locations: &[Location],
    workspace_root: &Path,
    client: &mut DaemonClient,
) -> Vec<EnrichedReference> {
    // Collect unique file URIs to minimize daemon calls
    let unique_uris: Vec<String> =
        {
            let mut seen = HashSet::new();
            locations
                .iter()
                .filter_map(|loc| {
                    if seen.insert(loc.uri.as_str()) {
                        Some(loc.uri.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

    // Fetch document symbols for each unique file, cache results
    let mut symbol_cache: HashMap<String, Vec<DocumentSymbol>> = HashMap::new();
    for uri in &unique_uris {
        let file_path = uri.strip_prefix("file://").unwrap_or(uri);
        match client
            .execute_document_symbols(workspace_root.to_path_buf(), file_path.to_string())
            .await
        {
            Ok(result) => {
                symbol_cache.insert(uri.clone(), result.symbols);
            }
            Err(e) => {
                tracing::debug!("enrich_references: documentSymbol failed for {uri}: {e}");
                // Fall through — missing entry means "module scope" fallback
            }
        }
    }

    // Enrich each location
    locations
        .iter()
        .map(|loc| {
            let context = if let Some(symbols) = symbol_cache.get(&loc.uri) {
                find_enclosing_symbol(symbols, loc.range.start.line, loc.range.start.character)
                    .unwrap_or_else(|| "module scope".to_string())
            } else {
                "module scope".to_string()
            };
            EnrichedReference { location: loc.clone(), context }
        })
        .collect()
}

/// Find the (line, column) where `name` appears, starting at a given 0-indexed line.
///
/// Workspace-symbol responses return the range of the full declaration
/// (e.g. the `class` keyword or a decorator), but hover/references need the
/// cursor on the *name* itself. This helper reads the source and locates the
/// name — first on the reported line, then on a few subsequent lines to handle
/// decorators (`@dataclass`, `@property`, etc.) that shift the symbol start
/// before the actual `class`/`def` keyword.
async fn find_name_column(file_path: &str, line_0: u32, name: &str) -> Option<(u32, u32)> {
    let content = match tokio::fs::read_to_string(file_path).await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("find_name_column: cannot read {file_path}: {e}");
            return None;
        }
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = line_0 as usize;
    if start >= lines.len() {
        tracing::debug!(
            "find_name_column: line {line_0} out of range in {file_path} ({} lines)",
            lines.len()
        );
        return None;
    }

    // Search the reported line first, then up to 10 subsequent lines
    // to skip past decorator stacks like @dataclass, @property, etc.
    for (idx, src_line) in lines.iter().enumerate().skip(start).take(11) {
        if let Some(col) = src_line.find(name) {
            let line = u32::try_from(idx).ok()?;
            let col = u32::try_from(col).ok()?;
            tracing::debug!(
                "find_name_column: found '{name}' at line {line} col {col} in {file_path}"
            );
            return Some((line, col));
        }
    }

    tracing::debug!("find_name_column: '{name}' not found near line {line_0} in {file_path}");
    None
}

/// Parse dotted notation like `Container.member` into `(container, symbol)`.
///
/// Splits on the **last** dot so that `A.B.method` yields `("A.B", "method")`.
/// Returns `None` for bare names (no dot), meaning "search without container filter".
fn parse_dotted_symbol(input: &str) -> Option<(&str, &str)> {
    let dot = input.rfind('.')?;
    let container = &input[..dot];
    let symbol = &input[dot + 1..];
    if container.is_empty() || symbol.is_empty() {
        return None;
    }
    Some((container, symbol))
}

/// Search workspace symbols with dotted-notation support.
///
/// If `symbol` contains a dot (e.g. `Class.method`), splits on the last dot,
/// searches for the member name, then verifies each result is inside the
/// expected container using the document symbol tree.
/// Returns `(search_name, result)` where `search_name` is the symbol part
/// actually searched for (the part after the last dot, or the full name).
#[cfg(unix)]
async fn workspace_symbols_dotted(
    client: &mut DaemonClient,
    workspace: PathBuf,
    symbol: &str,
) -> Result<(String, crate::daemon::protocol::WorkspaceSymbolsResult)> {
    if let Some((container, member)) = parse_dotted_symbol(symbol) {
        let result =
            client.execute_workspace_symbols_exact(workspace.clone(), member.to_string()).await?;

        if result.symbols.is_empty() {
            return Ok((member.to_string(), result));
        }

        // Filter symbols by checking the document symbol tree for each file.
        // A symbol qualifies if find_enclosing_symbol returns a path starting
        // with the container name (e.g. "Calculator.add" starts with "Calculator").
        let mut doc_sym_cache: HashMap<String, Vec<DocumentSymbol>> = HashMap::new();
        let mut filtered = Vec::new();

        for sym_info in result.symbols {
            let file_path = sym_info
                .location
                .uri
                .strip_prefix("file://")
                .unwrap_or(&sym_info.location.uri)
                .to_string();

            let doc_symbols = if let Some(cached) = doc_sym_cache.get(&file_path) {
                cached
            } else {
                let ds = client
                    .execute_document_symbols(workspace.clone(), file_path.clone())
                    .await
                    .map(|r| r.symbols)
                    .unwrap_or_default();
                doc_sym_cache.entry(file_path.clone()).or_insert(ds)
            };

            let line = sym_info.location.range.start.line;
            let character = sym_info.location.range.start.character;
            if let Some(enclosing) = find_enclosing_symbol(doc_symbols, line, character) {
                // enclosing is like "Calculator.add"; container is "Calculator"
                // Check that enclosing starts with container (exact segment match)
                if enclosing == format!("{container}.{member}")
                    || enclosing.starts_with(&format!("{container}."))
                {
                    filtered.push(sym_info);
                }
            }
        }

        Ok((
            member.to_string(),
            crate::daemon::protocol::WorkspaceSymbolsResult { symbols: filtered },
        ))
    } else {
        let result = client.execute_workspace_symbols_exact(workspace, symbol.to_string()).await?;
        Ok((symbol.to_string(), result))
    }
}

/// Try to parse a string as `file:line:col`. Returns `None` if it doesn't match.
fn parse_file_position(input: &str) -> Option<(String, u32, u32)> {
    let last_colon = input.rfind(':')?;
    let col: u32 = input[last_colon + 1..].parse().ok()?;
    let rest = &input[..last_colon];
    let second_colon = rest.rfind(':')?;
    let line: u32 = rest[second_colon + 1..].parse().ok()?;
    let file = &rest[..second_colon];
    if file.is_empty() {
        return None;
    }
    Some((file.to_string(), line, col))
}

/// A resolved reference query ready to send to the daemon.
#[cfg(unix)]
struct ResolvedQuery {
    /// Display label for output grouping
    label: String,
    /// File path for the LSP references request
    file: String,
    /// 0-based line
    line: u32,
    /// 0-based column
    column: u32,
}

/// Resolve symbol names to LSP positions via file search or workspace symbols.
#[cfg(unix)]
async fn resolve_symbols_to_queries(
    symbols: &[String],
    file: Option<&Path>,
    workspace_root: &Path,
    timeout: Duration,
) -> Result<Vec<ResolvedQuery>> {
    let mut resolved = Vec::new();

    if let Some(file) = file {
        let file_str = file.to_string_lossy();
        let finder = SymbolFinder::new(&file_str).await?;

        for symbol in symbols {
            let positions = finder.find_symbol_positions(symbol);
            if positions.is_empty() {
                resolved.push(ResolvedQuery {
                    label: symbol.clone(),
                    file: String::new(),
                    line: 0,
                    column: 0,
                });
            } else {
                for &(ln, col) in &positions {
                    resolved.push(ResolvedQuery {
                        label: symbol.clone(),
                        file: file_str.to_string(),
                        line: ln,
                        column: col,
                    });
                }
            }
        }
    } else {
        let mut client = DaemonClient::connect_with_timeout(timeout).await?;
        for symbol in symbols {
            let (_search_name, result) =
                workspace_symbols_dotted(&mut client, workspace_root.to_path_buf(), symbol).await?;

            if result.symbols.is_empty() {
                resolved.push(ResolvedQuery {
                    label: symbol.clone(),
                    file: String::new(),
                    line: 0,
                    column: 0,
                });
            } else {
                for sym_info in &result.symbols {
                    let file_path = sym_info
                        .location
                        .uri
                        .strip_prefix("file://")
                        .unwrap_or(&sym_info.location.uri)
                        .to_string();
                    let ws_line = sym_info.location.range.start.line;
                    // Workspace-symbol range.start may point at a decorator
                    // or keyword; hover/references need the symbol *name*.
                    let (line, column) = find_name_column(&file_path, ws_line, &sym_info.name)
                        .await
                        .unwrap_or((ws_line, sym_info.location.range.start.character));
                    resolved.push(ResolvedQuery {
                        label: symbol.clone(),
                        file: file_path,
                        line,
                        column,
                    });
                }
            }
        }
    }

    Ok(resolved)
}

/// Send resolved queries to the daemon in a single batch RPC and merge results by label.
#[cfg(unix)]
async fn execute_references_batch(
    resolved: Vec<ResolvedQuery>,
    workspace_root: &Path,
    include_declaration: bool,
    timeout: Duration,
) -> Result<Vec<(String, Vec<Location>)>> {
    // Split into queries the daemon can handle (have a file) and empty ones
    let mut empty_labels: Vec<String> = Vec::new();
    let mut batch_queries: Vec<BatchReferencesQuery> = Vec::new();

    for q in resolved {
        if q.file.is_empty() {
            empty_labels.push(q.label);
        } else {
            batch_queries.push(BatchReferencesQuery {
                label: q.label,
                file: PathBuf::from(q.file),
                line: q.line,
                column: q.column,
            });
        }
    }

    let mut merged: Vec<(String, Vec<Location>)> = Vec::new();

    // Add empty results for unresolved symbols
    for label in empty_labels {
        if !merged.iter().any(|(s, _)| s == &label) {
            merged.push((label, Vec::new()));
        }
    }

    // Send the batch to the daemon in one call
    if !batch_queries.is_empty() {
        let mut client = DaemonClient::connect_with_timeout(timeout).await?;
        let result = client
            .execute_batch_references(
                workspace_root.to_path_buf(),
                batch_queries,
                include_declaration,
            )
            .await?;

        for entry in result.entries {
            if let Some(existing) = merged.iter_mut().find(|(s, _)| s == &entry.label) {
                existing.1.extend(entry.locations);
            } else {
                merged.push((entry.label, entry.locations));
            }
        }
    }

    for (_, locations) in &mut merged {
        dedup_locations(locations);
    }
    Ok(merged)
}

/// Collect query strings from CLI args and optionally stdin.
fn collect_queries(queries: &[String], read_stdin: bool) -> Result<Vec<String>> {
    let mut all = queries.to_vec();
    if read_stdin {
        let stdin = std::io::stdin();
        for line in std::io::BufRead::lines(stdin.lock()) {
            let trimmed = line?.trim().to_string();
            if !trimmed.is_empty() {
                all.push(trimmed);
            }
        }
    }
    Ok(all)
}

/// Classify queries as positions or symbols and resolve to LSP coordinates.
#[cfg(unix)]
async fn classify_and_resolve(
    all_queries: &[String],
    file: Option<&Path>,
    workspace_root: &Path,
    timeout: Duration,
) -> Result<Vec<ResolvedQuery>> {
    let mut resolved: Vec<ResolvedQuery> = Vec::new();
    let mut symbols: Vec<String> = Vec::new();

    for q in all_queries {
        if let Some((f, l, c)) = parse_file_position(q) {
            resolved.push(ResolvedQuery {
                label: q.clone(),
                file: f,
                line: l.saturating_sub(1),
                column: c.saturating_sub(1),
            });
        } else {
            symbols.push(q.clone());
        }
    }

    if !symbols.is_empty() {
        resolved.extend(resolve_symbols_to_queries(&symbols, file, workspace_root, timeout).await?);
    }

    Ok(resolved)
}

#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
pub async fn handle_references_command(
    workspace_root: &Path,
    file: Option<&Path>,
    queries: &[String],
    position: Option<(u32, u32)>,
    read_stdin: bool,
    include_declaration: bool,
    references_limit: usize,
    formatter: &OutputFormatter,
    timeout: Duration,
    show_tests: bool,
    debug_log: Option<Arc<DebugLog>>,
) -> Result<()> {
    ensure_daemon_running().await?;

    // Explicit --file -l -c: single position mode
    if let (Some(file), Some((line, col))) = (file, position) {
        let mut client = connect_daemon(timeout, debug_log.as_ref()).await?;
        let result = client
            .execute_references(
                workspace_root.to_path_buf(),
                file.to_string_lossy().to_string(),
                line.saturating_sub(1),
                col.saturating_sub(1),
                include_declaration,
            )
            .await?;

        if let Some(ref log) = debug_log {
            log.log_result_summary(&format!("{} reference(s) found", result.locations.len()));
        }

        let label = format!("{}:{line}:{col}", file.display());
        let enriched = enrich_and_limit_references(
            &label,
            result.locations,
            references_limit,
            workspace_root,
            &mut client,
            show_tests,
        )
        .await?;
        let cache = SourceCache::from_uris(
            enriched.displayed.iter().map(|e| e.location.uri.as_str()).chain(
                enriched
                    .test_references
                    .iter()
                    .flat_map(|t| t.displayed.iter().map(|e| e.location.uri.as_str())),
            ),
        )
        .await;
        println!("{}", formatter.format_enriched_references_results(&[enriched], &cache));
        return Ok(());
    }

    let all_queries = collect_queries(queries, read_stdin)?;
    if all_queries.is_empty() {
        anyhow::bail!(
            "Provide symbol names, file:line:col positions, or --file with --line/--column.\n\
             Position mode:  tyf refs -f file.py -l 10 -c 5\n\
             Symbol mode:    tyf refs my_func my_class\n\
             Mixed/pipe:     tyf refs file.py:10:5 my_func\n\
             Stdin:          ... | tyf refs --stdin"
        );
    }

    let resolved = classify_and_resolve(&all_queries, file, workspace_root, timeout).await?;
    let merged =
        execute_references_batch(resolved, workspace_root, include_declaration, timeout).await?;

    // Enrich and limit each result group — reuse a single daemon connection
    let mut enriched_results = Vec::new();
    let mut client = DaemonClient::connect_with_timeout(timeout).await?;
    for (label, locations) in merged {
        let enriched = enrich_and_limit_references(
            &label,
            locations,
            references_limit,
            workspace_root,
            &mut client,
            show_tests,
        )
        .await?;
        enriched_results.push(enriched);
    }

    if let Some(ref log) = debug_log {
        let total: usize = enriched_results.iter().map(|r| r.total_count).sum();
        log.log_result_summary(&format!("{total} reference(s) found"));
        let cmd = format!("refs {}", all_queries.join(" "));
        log.log_reproduction_commands(workspace_root, &all_queries, &cmd);
    }

    let cache = SourceCache::from_uris(enriched_results.iter().flat_map(|r| {
        let main = r.displayed.iter().map(|e| e.location.uri.as_str());
        let test = r
            .test_references
            .iter()
            .flat_map(|t| t.displayed.iter().map(|e| e.location.uri.as_str()));
        main.chain(test)
    }))
    .await;
    println!("{}", formatter.format_enriched_references_results(&enriched_results, &cache));

    Ok(())
}

/// Apply limit and enrich displayed references with enclosing symbol context.
///
/// Always partitions into test vs non-test. When `show_tests` is true, test
/// references are enriched and returned in a separate section. When false,
/// only the count is preserved (for the "N hidden" hint).
#[cfg(unix)]
async fn enrich_and_limit_references(
    label: &str,
    locations: Vec<Location>,
    references_limit: usize,
    workspace_root: &Path,
    client: &mut DaemonClient,
    show_tests: bool,
) -> Result<EnrichedReferencesResult> {
    use crate::cli::output::TestReferencesSection;

    let (non_test_locs, test_locs) = partition_test_locations(locations);

    // Process non-test references
    let total_count = non_test_locs.len();
    let display_count =
        if references_limit == 0 { total_count } else { references_limit.min(total_count) };
    let to_display = &non_test_locs[..display_count];
    let remaining_count = total_count - display_count;

    let displayed = if to_display.is_empty() {
        Vec::new()
    } else {
        enrich_references(to_display, workspace_root, client).await
    };

    // Process test references
    let test_references = if test_locs.is_empty() {
        None
    } else if show_tests {
        let test_total = test_locs.len();
        let test_display_count =
            if references_limit == 0 { test_total } else { references_limit.min(test_total) };
        let test_to_display = &test_locs[..test_display_count];
        let test_remaining = test_total - test_display_count;
        let test_displayed = enrich_references(test_to_display, workspace_root, client).await;
        Some(TestReferencesSection {
            total_count: test_total,
            displayed: test_displayed,
            remaining_count: test_remaining,
        })
    } else {
        // Not showing tests, but record count for hint
        Some(TestReferencesSection {
            total_count: test_locs.len(),
            displayed: Vec::new(),
            remaining_count: 0,
        })
    };

    Ok(EnrichedReferencesResult {
        label: label.to_string(),
        total_count,
        displayed,
        remaining_count,
        test_references,
    })
}

#[cfg(not(unix))]
#[allow(clippy::too_many_arguments)]
pub async fn handle_references_command(
    _workspace_root: &Path,
    _file: Option<&Path>,
    _queries: &[String],
    _position: Option<(u32, u32)>,
    _read_stdin: bool,
    _include_declaration: bool,
    _references_limit: usize,
    _formatter: &OutputFormatter,
    _timeout: Duration,
    _show_tests: bool,
    _debug_log: Option<Arc<DebugLog>>,
) -> Result<()> {
    anyhow::bail!(
        "The 'refs' command requires the background daemon, which is only supported on Unix systems"
    )
}

#[allow(clippy::too_many_lines)]
pub async fn handle_find_command(
    workspace_root: &Path,
    file: Option<&Path>,
    symbols: &[String],
    fuzzy: bool,
    formatter: &OutputFormatter,
    timeout: Duration,
    debug_log: Option<Arc<DebugLog>>,
) -> Result<()> {
    // --fuzzy mode: use workspace/symbol pure fuzzy query
    if fuzzy {
        #[cfg(not(unix))]
        {
            let _ = (workspace_root, symbols, timeout, debug_log);
            anyhow::bail!(
                "The --fuzzy flag requires the background daemon, which is only \
                 supported on Unix systems."
            );
        }
        #[cfg(unix)]
        {
            ensure_daemon_running().await?;
            let mut client = connect_daemon(timeout, debug_log.as_ref()).await?;

            for symbol in symbols {
                let result = client
                    .execute_workspace_symbols(workspace_root.to_path_buf(), symbol.clone())
                    .await?;

                if result.symbols.is_empty() {
                    if let Some(ref log) = debug_log {
                        log.log_result_summary(&format!(
                            "0 symbols found matching '{symbol}' (fuzzy)"
                        ));
                    }
                    println!(
                        "{}",
                        formatter.styler().error(&format!("No results found matching '{symbol}'"))
                    );
                } else {
                    if let Some(ref log) = debug_log {
                        log.log_result_summary(&format!(
                            "{} symbol(s) found matching '{symbol}' (fuzzy)",
                            result.symbols.len()
                        ));
                    }
                    if symbols.len() > 1 {
                        let heading =
                            format!("=== {symbol} ({} match(es)) ===", result.symbols.len());
                        println!("{}\n", formatter.styler().symbol(&heading));
                    }
                    println!("{}", formatter.format_workspace_symbols(&result.symbols));
                }
            }
            if let Some(ref log) = debug_log {
                let cmd = format!("find {} --fuzzy", symbols.join(" "));
                log.log_reproduction_commands(workspace_root, symbols, &cmd);
                // Log LSP snippet for each fuzzy query
                for sym in symbols {
                    log.log_lsp_snippet(workspace_root, sym, 0, 0, "workspace/symbol");
                }
            }
            return Ok(());
        }
    }

    let mut results: Vec<(String, Vec<Location>)> = Vec::new();

    if let Some(file) = file {
        let client = TyLspClient::new(&workspace_root.to_string_lossy()).await?;
        let file_str = file.to_string_lossy();
        let finder = SymbolFinder::new(&file_str).await?;
        client.open_document(&file_str).await?;

        for symbol in symbols {
            let positions = finder.find_symbol_positions(symbol);

            if positions.is_empty() {
                results.push((symbol.clone(), Vec::new()));
                continue;
            }

            let mut all_locations = Vec::new();
            for (line, column) in positions {
                let locations =
                    client.goto_definition(&file.to_string_lossy(), line, column).await?;
                all_locations.extend(locations);
            }
            dedup_locations(&mut all_locations);

            results.push((symbol.clone(), all_locations));
        }
    } else {
        #[cfg(not(unix))]
        {
            let _ = (workspace_root, symbols, timeout, debug_log);
            anyhow::bail!(
                "Finding symbols without --file requires the background daemon, which is only \
                 supported on Unix systems. Use --file to search within a specific file instead."
            );
        }
        #[cfg(unix)]
        {
            for symbol in symbols {
                let locations =
                    find_symbol_via_workspace(workspace_root, symbol, timeout, debug_log.as_ref())
                        .await?;
                results.push((symbol.clone(), locations));
            }
        }
    }

    if let Some(ref log) = debug_log {
        let total: usize = results.iter().map(|(_, locs)| locs.len()).sum();
        log.log_result_summary(&format!("{total} definition(s) found"));
        let cmd = format!("find {}", symbols.join(" "));
        log.log_reproduction_commands(workspace_root, symbols, &cmd);
        // Log LSP snippet using the first result location (if any)
        for (sym, locs) in &results {
            if let Some(loc) = locs.first() {
                let file_path = loc.uri.strip_prefix("file://").unwrap_or(&loc.uri);
                log.log_lsp_snippet(
                    workspace_root,
                    file_path,
                    loc.range.start.line,
                    loc.range.start.character,
                    "textDocument/definition",
                );
            } else if file.is_none() {
                log.log_lsp_snippet(workspace_root, sym, 0, 0, "workspace/symbol");
            }
        }
    }

    let cache =
        SourceCache::from_uris(results.iter().flat_map(|(_, locs)| locs).map(|l| l.uri.as_str()))
            .await;
    println!("{}", formatter.format_find_results(&results, &cache));

    Ok(())
}

/// Find a symbol's location(s) using workspace symbols search.
#[cfg(unix)]
async fn find_symbol_via_workspace(
    workspace_root: &Path,
    symbol: &str,
    timeout: Duration,
    debug_log: Option<&Arc<DebugLog>>,
) -> Result<Vec<Location>> {
    ensure_daemon_running().await?;
    let mut client = connect_daemon(timeout, debug_log).await?;

    // Use exact_name filter (with optional container filter for dotted notation)
    // so the daemon only returns symbols with matching names.
    let (_search_name, result) =
        workspace_symbols_dotted(&mut client, workspace_root.to_path_buf(), symbol).await?;

    // If exact matches found, use them; otherwise fall back to fuzzy search
    // (only for bare names — dotted notation never falls back to avoid confusion).
    if !result.symbols.is_empty() {
        return Ok(result.symbols.into_iter().map(|s| s.location).collect());
    }

    if parse_dotted_symbol(symbol).is_some() {
        // Dotted notation: no fallback to fuzzy search
        return Ok(Vec::new());
    }

    // Fallback: fuzzy search (no exact_name filter), reuse the same connection
    let result =
        client.execute_workspace_symbols(workspace_root.to_path_buf(), symbol.to_string()).await?;
    Ok(result.symbols.into_iter().map(|s| s.location).collect())
}

#[cfg(unix)]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn handle_show_command(
    workspace_root: &Path,
    file: Option<&Path>,
    symbols: &[String],
    formatter: &OutputFormatter,
    timeout: Duration,
    show_individual_refs: bool,
    references_limit: usize,
    show_tests: bool,
    show_doc: bool,
    debug_log: Option<Arc<DebugLog>>,
) -> Result<()> {
    ensure_daemon_running().await?;

    let mut results: Vec<InspectResult> = Vec::new();
    for symbol in symbols {
        // Always fetch references for the count summary
        let result = inspect_single_symbol(workspace_root, file, symbol, timeout, true).await?;
        results.push(result);
    }

    if let Some(ref log) = debug_log {
        for r in &results {
            let has_hover = if r.hover.is_some() { "yes" } else { "no" };
            log.log_result_summary(&format!(
                "show '{}': {} definition(s), hover={has_hover}, {} reference(s)",
                r.symbol,
                r.definitions.len(),
                r.references.len(),
            ));
        }
        let cmd = format!("show {}", symbols.join(" "));
        log.log_reproduction_commands(workspace_root, symbols, &cmd);
    }

    // Build enriched entries — reuse a single daemon connection for all enrichment
    let mut entries: Vec<ShowEntry<'_>> = Vec::new();
    let needs_enrichment = show_individual_refs && results.iter().any(|r| !r.references.is_empty());
    let mut enrich_client = if needs_enrichment {
        Some(DaemonClient::connect_with_timeout(timeout).await?)
    } else {
        None
    };
    for r in &results {
        // Partition into non-test and test references
        let (non_test_refs, test_refs) = partition_test_locations(r.references.clone());

        let total_reference_count = non_test_refs.len();
        let total_reference_files = count_unique_files(&non_test_refs);

        let (displayed_references, remaining_reference_count) =
            if show_individual_refs && !non_test_refs.is_empty() {
                let display_count = if references_limit == 0 {
                    non_test_refs.len()
                } else {
                    references_limit.min(non_test_refs.len())
                };
                let to_display = &non_test_refs[..display_count];
                let remaining = non_test_refs.len() - display_count;

                let enriched = enrich_references(
                    to_display,
                    workspace_root,
                    enrich_client.as_mut().expect("client created above"),
                )
                .await;
                (enriched, remaining)
            } else {
                (Vec::new(), 0)
            };

        // Build test references section
        let test_references = if test_refs.is_empty() {
            None
        } else if show_tests {
            let test_total = test_refs.len();
            let (test_displayed, test_remaining) = if show_individual_refs && !test_refs.is_empty()
            {
                let test_display_count = if references_limit == 0 {
                    test_total
                } else {
                    references_limit.min(test_total)
                };
                let test_to_display = &test_refs[..test_display_count];
                let remaining = test_total - test_display_count;
                let enriched = enrich_references(
                    test_to_display,
                    workspace_root,
                    enrich_client.as_mut().expect("client created above"),
                )
                .await;
                (enriched, remaining)
            } else {
                (Vec::new(), 0)
            };
            Some(crate::cli::output::TestReferencesSection {
                total_count: test_total,
                displayed: test_displayed,
                remaining_count: test_remaining,
            })
        } else {
            // Not showing tests, but record count for hint
            Some(crate::cli::output::TestReferencesSection {
                total_count: test_refs.len(),
                displayed: Vec::new(),
                remaining_count: 0,
            })
        };

        entries.push(ShowEntry {
            symbol: r.symbol.as_str(),
            kind: r.kind.as_ref(),
            definitions: r.definitions.as_slice(),
            hover: r.hover.as_ref(),
            total_reference_count,
            total_reference_files,
            displayed_references,
            remaining_reference_count,
            show_individual_refs,
            show_doc,
            test_references,
        });
    }

    let cache = SourceCache::from_uris(entries.iter().flat_map(|e| {
        let defs = e.definitions.iter().map(|l| l.uri.as_str());
        let refs = e.displayed_references.iter().map(|r| r.location.uri.as_str());
        let test = e
            .test_references
            .iter()
            .flat_map(|t| t.displayed.iter().map(|r| r.location.uri.as_str()));
        defs.chain(refs).chain(test)
    }))
    .await;
    println!("{}", formatter.format_show_results(&entries, &cache));

    Ok(())
}

#[cfg(not(unix))]
#[allow(clippy::too_many_arguments)]
pub async fn handle_show_command(
    _workspace_root: &Path,
    _file: Option<&Path>,
    _symbols: &[String],
    _formatter: &OutputFormatter,
    _timeout: Duration,
    _show_individual_refs: bool,
    _references_limit: usize,
    _show_tests: bool,
    _show_doc: bool,
    _debug_log: Option<Arc<DebugLog>>,
) -> Result<()> {
    anyhow::bail!(
        "The 'show' command requires the background daemon, which is only supported on Unix systems"
    )
}

#[cfg(unix)]
struct InspectResult {
    symbol: String,
    kind: Option<crate::lsp::protocol::SymbolKind>,
    definitions: Vec<Location>,
    hover: Option<crate::lsp::protocol::Hover>,
    references: Vec<Location>,
}

#[cfg(unix)]
async fn inspect_single_symbol(
    workspace_root: &Path,
    file: Option<&Path>,
    symbol: &str,
    timeout: Duration,
    include_references: bool,
) -> Result<InspectResult> {
    // Step 1: Find the symbol's location(s)
    let (mut client, definition_file, def_line, def_col, all_definitions, symbol_kind) =
        if let Some(file) = file {
            let file_str = file.to_string_lossy();
            let finder = SymbolFinder::new(&file_str).await?;
            let positions = finder.find_symbol_positions(symbol);

            if positions.is_empty() {
                return Ok(InspectResult {
                    symbol: symbol.to_string(),
                    kind: None,
                    definitions: Vec::new(),
                    hover: None,
                    references: Vec::new(),
                });
            }

            let (first_line, first_col) = positions[0];

            let mut client = DaemonClient::connect_with_timeout(timeout).await?;
            let mut all_definitions = Vec::new();
            for (line, column) in &positions {
                let result = client
                    .execute_definition(
                        workspace_root.to_path_buf(),
                        file_str.to_string(),
                        *line,
                        *column,
                    )
                    .await?;
                if let Some(loc) = result.location {
                    all_definitions.push(loc);
                }
            }
            dedup_locations(&mut all_definitions);

            // File-based search doesn't provide symbol kind
            (client, file_str.to_string(), first_line, first_col, all_definitions, None)
        } else {
            // Use exact_name filter (with optional container for dotted notation)
            let mut client = DaemonClient::connect_with_timeout(timeout).await?;
            let (_search_name, result) =
                workspace_symbols_dotted(&mut client, workspace_root.to_path_buf(), symbol).await?;

            let matched = &result.symbols;

            if matched.is_empty() {
                return Ok(InspectResult {
                    symbol: symbol.to_string(),
                    kind: None,
                    definitions: Vec::new(),
                    hover: None,
                    references: Vec::new(),
                });
            }

            let first = &matched[0];
            let file_path =
                first.location.uri.strip_prefix("file://").unwrap_or(&first.location.uri);
            let ws_line = first.location.range.start.line;
            let ws_col = first.location.range.start.character;
            // Workspace-symbol range.start may point at a decorator or keyword;
            // hover/references need the symbol *name* position.
            let name_pos = find_name_column(file_path, ws_line, &first.name).await;
            let (def_line, def_col) = name_pos.unwrap_or((ws_line, ws_col));
            tracing::debug!(
                "inspect: workspace-symbol line={ws_line} col={ws_col}, resolved line={def_line} col={def_col} for '{}'",
                first.name
            );
            let all_definitions: Vec<Location> =
                matched.iter().map(|s| s.location.clone()).collect();

            (
                client,
                file_path.to_string(),
                def_line,
                def_col,
                all_definitions,
                Some(first.kind.clone()),
            )
        };

    // Steps 2 & 3: Get hover info (and optionally references) via single daemon call
    tracing::debug!(
        "inspect: querying hover/refs at {definition_file}:{def_line}:{def_col} for '{symbol}'"
    );
    let inspect = client
        .execute_inspect(
            workspace_root.to_path_buf(),
            definition_file,
            def_line,
            def_col,
            include_references,
        )
        .await?;

    tracing::debug!(
        "inspect: hover={}, refs={}",
        if inspect.hover.is_some() { "present" } else { "NONE" },
        inspect.references.len()
    );

    Ok(InspectResult {
        symbol: symbol.to_string(),
        kind: symbol_kind,
        definitions: all_definitions,
        hover: inspect.hover,
        references: inspect.references,
    })
}

#[cfg(unix)]
pub async fn handle_document_symbols_command(
    workspace_root: &Path,
    file: &Path,
    formatter: &OutputFormatter,
    timeout: Duration,
    debug_log: Option<Arc<DebugLog>>,
) -> Result<()> {
    ensure_daemon_running().await?;
    let mut client = connect_daemon(timeout, debug_log.as_ref()).await?;

    let result = client
        .execute_document_symbols(workspace_root.to_path_buf(), file.to_string_lossy().to_string())
        .await?;

    if let Some(ref log) = debug_log {
        log.log_result_summary(&format!(
            "{} symbol(s) found in {}",
            result.symbols.len(),
            file.display()
        ));
        let cmd = format!("list {}", file.display());
        log.log_reproduction_commands(workspace_root, &[], &cmd);
    }

    if result.symbols.is_empty() {
        println!(
            "{}",
            formatter.styler().error(&format!("No symbols found in {}", file.display()))
        );
    } else {
        println!("Document outline for {}:\n", file.display());
        println!("{}", formatter.format_document_symbols(&result.symbols));
    }

    Ok(())
}

#[cfg(not(unix))]
pub async fn handle_document_symbols_command(
    _workspace_root: &Path,
    _file: &Path,
    _formatter: &OutputFormatter,
    _timeout: Duration,
    _debug_log: Option<Arc<DebugLog>>,
) -> Result<()> {
    anyhow::bail!(
        "The 'list' command requires the background daemon, which is only supported on Unix systems"
    )
}

#[cfg(unix)]
pub async fn handle_members_command(
    workspace_root: &Path,
    file: Option<&Path>,
    symbols: &[String],
    include_all: bool,
    formatter: &OutputFormatter,
    timeout: Duration,
    debug_log: Option<Arc<DebugLog>>,
) -> Result<()> {
    ensure_daemon_running().await?;

    let mut results: Vec<crate::daemon::protocol::MembersResult> = Vec::new();

    for symbol in symbols {
        let result =
            members_single_class(workspace_root, file, symbol, include_all, timeout).await?;
        results.push(result);
    }

    // Check for non-class symbols and print appropriate errors
    let mut has_output = false;
    let mut valid_results: Vec<crate::daemon::protocol::MembersResult> = Vec::new();

    for result in results {
        match result.symbol_kind.as_ref() {
            None => {
                eprintln!("No symbol '{}' found in the project.", result.class_name);
                has_output = true;
            }
            Some(kind) if !matches!(kind, crate::lsp::protocol::SymbolKind::Class) => {
                let kind_name = match kind {
                    crate::lsp::protocol::SymbolKind::Function => "a function",
                    crate::lsp::protocol::SymbolKind::Method => "a method",
                    crate::lsp::protocol::SymbolKind::Variable => "a variable",
                    crate::lsp::protocol::SymbolKind::Constant => "a constant",
                    crate::lsp::protocol::SymbolKind::Module => "a module",
                    _ => "not a class",
                };
                eprintln!(
                    "'{}' is {kind_name}, not a class. Use 'show' instead.",
                    result.class_name
                );
                has_output = true;
            }
            Some(_) => {
                valid_results.push(result);
            }
        }
    }

    if let Some(ref log) = debug_log {
        for r in &valid_results {
            log.log_result_summary(&format!(
                "members '{}': {} member(s)",
                r.class_name,
                r.members.len(),
            ));
        }
        let cmd = format!("members {}", symbols.join(" "));
        log.log_reproduction_commands(workspace_root, symbols, &cmd);
    }

    if !valid_results.is_empty() {
        if has_output {
            // Separate error messages from valid output
            eprintln!();
        }
        println!("{}", formatter.format_members_results(&valid_results));
    }

    Ok(())
}

/// Look up a single class's members via the daemon.
#[cfg(unix)]
async fn members_single_class(
    workspace_root: &Path,
    file: Option<&Path>,
    symbol: &str,
    include_all: bool,
    timeout: Duration,
) -> Result<crate::daemon::protocol::MembersResult> {
    if let Some(file) = file {
        // File-based: pass directly to daemon
        let mut client = DaemonClient::connect_with_timeout(timeout).await?;
        client
            .execute_members(
                workspace_root.to_path_buf(),
                file.to_string_lossy().to_string(),
                symbol.to_string(),
                include_all,
            )
            .await
    } else {
        // Workspace-based: find the class via workspace symbols first
        let mut client = DaemonClient::connect_with_timeout(timeout).await?;
        let ws_result = client
            .execute_workspace_symbols_exact(workspace_root.to_path_buf(), symbol.to_string())
            .await?;

        if ws_result.symbols.is_empty() {
            return Ok(crate::daemon::protocol::MembersResult {
                class_name: symbol.to_string(),
                file_uri: String::new(),
                class_line: 0,
                class_column: 0,
                symbol_kind: None,
                members: Vec::new(),
            });
        }

        let first = &ws_result.symbols[0];
        let file_path =
            first.location.uri.strip_prefix("file://").unwrap_or(&first.location.uri).to_string();

        client
            .execute_members(
                workspace_root.to_path_buf(),
                file_path,
                symbol.to_string(),
                include_all,
            )
            .await
    }
}

#[cfg(not(unix))]
pub async fn handle_members_command(
    _workspace_root: &Path,
    _file: Option<&Path>,
    _symbols: &[String],
    _include_all: bool,
    _formatter: &OutputFormatter,
    _timeout: Duration,
    _debug_log: Option<Arc<DebugLog>>,
) -> Result<()> {
    anyhow::bail!(
        "The 'members' command requires the background daemon, which is only supported on Unix systems"
    )
}

#[cfg(unix)]
pub async fn handle_daemon_command(command: DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Start { foreground } => {
            if foreground {
                // We are the spawned child process — actually run the daemon server
                let socket_path = DaemonServer::get_socket_path()?;
                let server = DaemonServer::new(socket_path);
                server.start().await?;
                return Ok(());
            }

            // Check if daemon is already running
            let socket_path = crate::daemon::client::get_socket_path()?;
            let pidfile_path = crate::daemon::pidfile::get_pidfile_path()?;

            if socket_path.exists() || pidfile_path.exists() {
                if DaemonClient::connect().await.is_ok() {
                    println!("Daemon is already running");
                    return Ok(());
                }
                // Stale files — clean up
                let _ = std::fs::remove_file(&socket_path);
                let _ = std::fs::remove_file(&pidfile_path);
            }

            // Spawn daemon in background
            spawn_daemon()?;

            // Wait for daemon to start
            println!("Starting daemon...");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // Verify it started
            match DaemonClient::connect().await {
                Ok(_) => println!("Daemon started successfully"),
                Err(e) => println!("Failed to start daemon: {e}"),
            }
        }

        DaemonCommands::Stop => match DaemonClient::connect().await {
            Ok(mut client) => {
                client.shutdown().await?;
                println!("Daemon stopped successfully");
            }
            Err(_) => {
                println!("Daemon is not running");
            }
        },

        DaemonCommands::Restart => {
            // Stop the running daemon (if any)
            let socket_path = crate::daemon::client::get_socket_path()?;
            let pidfile_path = crate::daemon::pidfile::get_pidfile_path()?;

            match DaemonClient::connect().await {
                Ok(mut client) => {
                    let _ = client.shutdown().await;
                    println!("Stopped existing daemon");
                    // Give the old daemon a moment to release the socket
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
                Err(_) => {
                    println!("No running daemon found");
                }
            }

            // Clean up stale files
            let _ = std::fs::remove_file(&socket_path);
            let _ = std::fs::remove_file(&pidfile_path);

            // Spawn a fresh daemon
            spawn_daemon()?;
            println!("Starting daemon...");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            match DaemonClient::connect().await {
                Ok(_) => println!("Daemon restarted successfully"),
                Err(e) => println!("Failed to start daemon: {e}"),
            }
        }

        DaemonCommands::Status => match DaemonClient::connect().await {
            Ok(mut client) => {
                let status = client.ping().await?;
                let uptime_secs = status.uptime;
                let mins = uptime_secs / 60;
                let secs = uptime_secs % 60;
                let uptime_str =
                    if mins > 0 { format!("{mins}m {secs}s") } else { format!("{secs}s") };

                println!("Daemon running (v{})", status.version);
                if status.version != CLIENT_VERSION {
                    println!(
                        "  ⚠ Version mismatch: daemon v{}, client v{} — run `tyf daemon restart` to update",
                        status.version, CLIENT_VERSION,
                    );
                }
                println!("PID: {}", status.pid);
                if let Some(ref cwd) = status.cwd {
                    println!("  Working dir: {cwd}");
                }
                if let Some(ref sock) = status.socket_path {
                    println!("  Unix socket: {sock}");
                }
                if let Some(port) = status.tcp_port {
                    println!("  TCP: 127.0.0.1:{port}");
                }
                println!("  Uptime: {uptime_str}");
                println!("  Active workspaces: {}", status.active_workspaces);
                if !status.workspace_paths.is_empty() {
                    for ws in &status.workspace_paths {
                        println!("    - {ws}  (src.include: [\"**\"] overridden)");
                    }
                }
            }
            Err(_) => {
                println!("Daemon: not running");
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_test_file_test_prefix() {
        assert!(is_test_file("file:///project/test_utils.py"));
        assert!(is_test_file("file:///project/test_models.py"));
        assert!(is_test_file("/some/path/test_foo.py"));
    }

    #[test]
    fn test_is_test_file_test_suffix() {
        assert!(is_test_file("file:///project/models_test.py"));
        assert!(is_test_file("file:///project/utils_test.py"));
        assert!(is_test_file("/some/path/foo_test.py"));
    }

    #[test]
    fn test_is_test_file_conftest() {
        assert!(is_test_file("file:///project/conftest.py"));
        assert!(is_test_file("file:///project/tests/conftest.py"));
        assert!(is_test_file("/project/conftest.py"));
    }

    #[test]
    fn test_is_test_file_tests_directory() {
        assert!(is_test_file("file:///project/tests/test_foo.py"));
        assert!(is_test_file("file:///project/tests/utils.py"));
        assert!(is_test_file("file:///project/tests/sub/helper.py"));
        assert!(is_test_file("/project/tests/fixtures.py"));
    }

    #[test]
    fn test_is_test_file_non_test() {
        assert!(!is_test_file("file:///project/models.py"));
        assert!(!is_test_file("file:///project/src/utils.py"));
        assert!(!is_test_file("file:///project/main.py"));
        assert!(!is_test_file("/project/src/handler.py"));
    }

    #[test]
    fn test_is_test_file_edge_cases() {
        // "contest" is not "conftest"
        assert!(!is_test_file("file:///project/contest.py"));
        // "testing" directory != "tests" directory
        assert!(!is_test_file("file:///project/testing/utils.py"));
        // "test" alone is not a match for test_ prefix
        assert!(!is_test_file("file:///project/test.py"));
    }

    #[test]
    fn test_partition_test_locations() {
        use crate::lsp::protocol::{Position, Range};

        let locations = vec![
            Location {
                uri: "file:///project/src/utils.py".to_string(),
                range: Range {
                    start: Position { line: 1, character: 0 },
                    end: Position { line: 1, character: 10 },
                },
            },
            Location {
                uri: "file:///project/tests/test_utils.py".to_string(),
                range: Range {
                    start: Position { line: 5, character: 0 },
                    end: Position { line: 5, character: 10 },
                },
            },
            Location {
                uri: "file:///project/conftest.py".to_string(),
                range: Range {
                    start: Position { line: 10, character: 0 },
                    end: Position { line: 10, character: 10 },
                },
            },
            Location {
                uri: "file:///project/src/main.py".to_string(),
                range: Range {
                    start: Position { line: 3, character: 0 },
                    end: Position { line: 3, character: 10 },
                },
            },
        ];

        let (non_test, test) = partition_test_locations(locations);
        assert_eq!(non_test.len(), 2);
        assert_eq!(test.len(), 2);
        assert!(non_test[0].uri.contains("utils.py"));
        assert!(non_test[1].uri.contains("main.py"));
        assert!(test[0].uri.contains("test_utils.py"));
        assert!(test[1].uri.contains("conftest.py"));
    }

    #[test]
    fn test_parse_file_position_valid() {
        assert_eq!(parse_file_position("file.py:10:5"), Some(("file.py".to_string(), 10, 5)));
        assert_eq!(
            parse_file_position("src/foo/bar.py:1:1"),
            Some(("src/foo/bar.py".to_string(), 1, 1))
        );
        assert_eq!(
            parse_file_position("/absolute/path.py:100:20"),
            Some(("/absolute/path.py".to_string(), 100, 20))
        );
    }

    #[test]
    fn test_parse_file_position_symbol_names() {
        assert_eq!(parse_file_position("my_function"), None);
        assert_eq!(parse_file_position("MyClass"), None);
        assert_eq!(parse_file_position("foo_bar_baz"), None);
    }

    #[test]
    fn test_parse_file_position_edge_cases() {
        // Only one colon
        assert_eq!(parse_file_position("file.py:10"), None);
        // Empty file part
        assert_eq!(parse_file_position(":10:5"), None);
        // Non-numeric parts
        assert_eq!(parse_file_position("file.py:abc:5"), None);
        assert_eq!(parse_file_position("file.py:10:abc"), None);
    }

    #[tokio::test]
    async fn test_find_name_column_class() {
        // "class Animal:" — "Animal" starts at line 0 column 6
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        std::fs::write(&file, "class Animal:\n    pass\n").unwrap();
        assert_eq!(find_name_column(file.to_str().unwrap(), 0, "Animal").await, Some((0, 6)));
    }

    #[tokio::test]
    async fn test_find_name_column_function() {
        // "def create_dog(name):" — "create_dog" starts at line 0 column 4
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        std::fs::write(&file, "def create_dog(name):\n    pass\n").unwrap();
        assert_eq!(find_name_column(file.to_str().unwrap(), 0, "create_dog").await, Some((0, 4)));
    }

    #[tokio::test]
    async fn test_find_name_column_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        std::fs::write(&file, "x = 1\n").unwrap();
        assert_eq!(find_name_column(file.to_str().unwrap(), 0, "Animal").await, None);
    }

    #[tokio::test]
    async fn test_find_name_column_nonexistent_file() {
        assert_eq!(find_name_column("/nonexistent/file.py", 0, "Animal").await, None);
    }

    #[tokio::test]
    async fn test_find_name_column_decorated_class() {
        // Workspace symbol points at line 0 (@dataclass), but name is on line 1
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        std::fs::write(&file, "@dataclass\nclass Config:\n    host: str\n").unwrap();
        assert_eq!(find_name_column(file.to_str().unwrap(), 0, "Config").await, Some((1, 6)));
    }

    #[tokio::test]
    async fn test_find_name_column_multi_decorator() {
        // Multiple decorators stacked
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.py");
        std::fs::write(&file, "@some_decorator\n@another_decorator\ndef my_func():\n    pass\n")
            .unwrap();
        assert_eq!(find_name_column(file.to_str().unwrap(), 0, "my_func").await, Some((2, 4)));
    }

    #[test]
    fn test_dedup_locations_removes_same_uri_and_line() {
        use crate::lsp::protocol::{Position, Range};

        let mut locations = vec![
            Location {
                uri: "file:///a.py".to_string(),
                range: Range {
                    start: Position { line: 5, character: 0 },
                    end: Position { line: 5, character: 10 },
                },
            },
            Location {
                uri: "file:///a.py".to_string(),
                range: Range {
                    start: Position { line: 5, character: 3 },
                    end: Position { line: 5, character: 8 },
                },
            },
        ];
        dedup_locations(&mut locations);
        assert_eq!(locations.len(), 1);
    }

    #[test]
    fn test_dedup_locations_keeps_different_lines() {
        use crate::lsp::protocol::{Position, Range};

        let mut locations = vec![
            Location {
                uri: "file:///a.py".to_string(),
                range: Range {
                    start: Position { line: 1, character: 0 },
                    end: Position { line: 1, character: 5 },
                },
            },
            Location {
                uri: "file:///a.py".to_string(),
                range: Range {
                    start: Position { line: 2, character: 0 },
                    end: Position { line: 2, character: 5 },
                },
            },
        ];
        dedup_locations(&mut locations);
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn test_dedup_locations_keeps_different_uris() {
        use crate::lsp::protocol::{Position, Range};

        let mut locations = vec![
            Location {
                uri: "file:///a.py".to_string(),
                range: Range {
                    start: Position { line: 5, character: 0 },
                    end: Position { line: 5, character: 10 },
                },
            },
            Location {
                uri: "file:///b.py".to_string(),
                range: Range {
                    start: Position { line: 5, character: 0 },
                    end: Position { line: 5, character: 10 },
                },
            },
        ];
        dedup_locations(&mut locations);
        assert_eq!(locations.len(), 2);
    }

    #[test]
    fn test_dedup_locations_empty() {
        let mut locations: Vec<Location> = vec![];
        dedup_locations(&mut locations);
        assert!(locations.is_empty());
    }

    #[test]
    fn test_dedup_locations_preserves_first_occurrence() {
        use crate::lsp::protocol::{Position, Range};

        let mut locations = vec![
            Location {
                uri: "file:///a.py".to_string(),
                range: Range {
                    start: Position { line: 5, character: 0 },
                    end: Position { line: 5, character: 10 },
                },
            },
            Location {
                uri: "file:///a.py".to_string(),
                range: Range {
                    start: Position { line: 5, character: 99 },
                    end: Position { line: 5, character: 100 },
                },
            },
        ];
        dedup_locations(&mut locations);
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].range.start.character, 0, "first occurrence should be preserved");
    }

    #[test]
    fn test_count_unique_files_distinct() {
        use crate::lsp::protocol::{Position, Range};

        let r = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 5 },
        };
        let locations = vec![
            Location { uri: "file:///a.py".to_string(), range: r.clone() },
            Location { uri: "file:///b.py".to_string(), range: r.clone() },
            Location { uri: "file:///a.py".to_string(), range: r },
        ];
        assert_eq!(count_unique_files(&locations), 2);
    }

    #[test]
    fn test_count_unique_files_all_same() {
        use crate::lsp::protocol::{Position, Range};

        let r = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 5 },
        };
        let locations = vec![
            Location { uri: "file:///a.py".to_string(), range: r.clone() },
            Location { uri: "file:///a.py".to_string(), range: r.clone() },
            Location { uri: "file:///a.py".to_string(), range: r },
        ];
        assert_eq!(count_unique_files(&locations), 1);
    }

    #[test]
    fn test_count_unique_files_empty() {
        let locations: Vec<Location> = vec![];
        assert_eq!(count_unique_files(&locations), 0);
    }

    #[test]
    fn test_collect_queries_args_only() {
        let args = vec!["foo".to_string(), "bar".to_string()];
        let result = collect_queries(&args, false).unwrap();
        assert_eq!(result, vec!["foo", "bar"]);
    }

    #[test]
    fn test_parse_dotted_symbol_simple() {
        assert_eq!(parse_dotted_symbol("Class.method"), Some(("Class", "method")));
    }

    #[test]
    fn test_parse_dotted_symbol_multiple_dots() {
        // Split on last dot: A.B.method → ("A.B", "method")
        assert_eq!(parse_dotted_symbol("A.B.method"), Some(("A.B", "method")));
    }

    #[test]
    fn test_parse_dotted_symbol_bare_name() {
        assert_eq!(parse_dotted_symbol("my_function"), None);
        assert_eq!(parse_dotted_symbol("MyClass"), None);
    }

    #[test]
    fn test_parse_dotted_symbol_edge_cases() {
        // Leading dot → empty container
        assert_eq!(parse_dotted_symbol(".method"), None);
        // Trailing dot → empty symbol
        assert_eq!(parse_dotted_symbol("Class."), None);
        // Just a dot
        assert_eq!(parse_dotted_symbol("."), None);
    }
}
