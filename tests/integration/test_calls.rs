//! Integration tests for `tyf calls` — the call-hierarchy command.
//!
//! These drive the real `tyf` binary against a real `ty` LSP server over
//! `test_project/call_chain.py`, per the project convention (no mocking; the
//! only mocks in the codebase fake the *daemon* wire protocol in unit tests).
//!
//! # Dependence on `ty` behavior
//!
//! Every assertion here rests on behavior established empirically in
//! [`docs/dev/call-hierarchy-spike.md`]. Each test names the spike finding it
//! depends on, so when a `ty` release breaks one it is clear whether the
//! product regressed or `ty` changed:
//!
//! * **Gate** — `callHierarchyProvider` exists from `ty 0.0.41`; the whole
//!   suite skips below that (`common::has_call_hierarchy`).
//! * **Position** — `prepareCallHierarchy` needs the *name token*, not the
//!   `def` keyword. Exercised implicitly by every test: they all resolve
//!   through `workspace/symbol`, which points at the keyword, so a regression
//!   in the `find_name_column` adjustment turns every test red.
//! * **Finding 1** — a decorator application is reported as an outgoing call,
//!   so a decorated function has an extra child. Tests never assert an exact
//!   child count for `charge_payment`.
//! * **Finding 2** — overloaded stdlib functions arrive once per overload;
//!   tyf collapses external leaves by name. Tests assert presence, not count.
//! * **Finding 3** — callbacks are invisible to static analysis.
//! * **Finding 4** — a class has no calls in either direction.

#[path = "common.rs"]
mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use std::path::PathBuf;

/// Workspace root pointing at the `test_project` directory.
fn test_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_project")
}

/// Run `tyf` against `test_project` and return stdout, asserting success.
fn run_tyf(args: &[&str]) -> String {
    let (stdout, stderr, status) = run_tyf_raw(args);
    assert!(status.success(), "tyf {args:?} failed:\nstdout:\n{stdout}\nstderr:\n{stderr}");
    stdout
}

/// Run `tyf` and return `(stdout, stderr, status)` without asserting.
fn run_tyf_raw(args: &[&str]) -> (String, String, std::process::ExitStatus) {
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(test_project_root());
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("failed to run tyf");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status,
    )
}

/// Absolute path to a file inside `test_project` — `--file` is resolved
/// against the process's working directory, not the workspace root.
fn fixture(name: &str) -> String {
    test_project_root().join(name).to_string_lossy().to_string()
}

/// Indentation level of a line in the rendered tree (2 spaces per level).
fn indent_level(line: &str) -> usize {
    (line.len() - line.trim_start().len()) / 2
}

/// Find the line naming `symbol` at the top level of the tree body, and return
/// its indent level. Panics with the whole tree if it is absent.
fn level_of(out: &str, symbol: &str) -> usize {
    let Some(line) = out.lines().find(|l| l.trim_start().starts_with(symbol)) else {
        panic!("'{symbol}' not in tree:\n{out}");
    };
    indent_level(line)
}

/// Related sub-cases are grouped into one `#[tokio::test]` rather than split
/// one assertion per test: every test in this process talks to the same daemon
/// socket, so fewer, larger tests mean fewer concurrent clients. The grouping
/// is by concern (direction, dotted notation, formats) rather than one giant
/// test, so a failure still names what broke.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_calls_outgoing() {
    common::require_ty();
    if !common::has_call_hierarchy() {
        return;
    }

    // --- depth 1: only the direct callees --------------------------------
    let out = run_tyf(&["calls", "process_order", "--depth", "1"]);
    assert_eq!(level_of(&out, "process_order"), 0, "root is unindented:\n{out}");
    assert_eq!(level_of(&out, "validate_order"), 1, "direct callee at level 1:\n{out}");
    assert_eq!(level_of(&out, "charge_payment"), 1, "direct callee at level 1:\n{out}");
    assert!(!out.contains("check_inventory"), "depth 1 must not reach the grandchild:\n{out}");

    // --- depth 2 (the default): the 3-deep chain -------------------------
    let default_out = run_tyf(&["calls", "process_order"]);
    assert_eq!(
        default_out,
        run_tyf(&["calls", "process_order", "--depth", "2"]),
        "depth 2 is the default"
    );
    assert_eq!(level_of(&default_out, "check_inventory"), 2, "got:\n{default_out}");

    // --- depth 3: reaches the stdlib leaf under check_inventory ----------
    let deep = run_tyf(&["calls", "process_order", "--depth", "3"]);
    assert!(
        deep.contains("floor (external)"),
        "depth 3 reaches math.floor as an external leaf:\n{deep}"
    );
    // Spike finding 2: `math.floor` has two overloads; tyf collapses them.
    assert_eq!(
        deep.matches("floor (external)").count(),
        1,
        "overloaded external callees collapse to one leaf:\n{deep}"
    );

    // --- explicit --out matches the default ------------------------------
    assert_eq!(run_tyf(&["calls", "process_order", "--out"]), default_out);

    // --- the diamond: check_inventory is expanded once, then marked ------
    // `validate_order` and `charge_payment` both call `check_inventory`.
    assert_eq!(
        default_out.matches("check_inventory").count(),
        2,
        "the join point appears under both parents:\n{default_out}"
    );
    assert!(
        default_out.contains("check_inventory \u{2191}"),
        "the second occurrence carries the dedup marker:\n{default_out}"
    );
    assert_eq!(
        default_out.matches("check_inventory \u{2191}").count(),
        1,
        "exactly one occurrence is deduped, the other is expanded:\n{default_out}"
    );

    // Spike finding 1: `charge_payment` is decorated, so `audit` shows up as an
    // extra outgoing call alongside the real body callee. Assert the body
    // callee is reached, not an exact child count.
    let decorated = run_tyf(&["calls", "charge_payment", "--depth", "1"]);
    assert!(
        decorated.contains("check_inventory"),
        "decoration must not hide the body's callee:\n{decorated}"
    );

    // --- direct recursion: a self-cycle ----------------------------------
    let recursive = run_tyf(&["calls", "countdown"]);
    assert!(recursive.contains("countdown (cycle)"), "got:\n{recursive}");

    // --- mutual recursion: A -> B -> A -----------------------------------
    let mutual = run_tyf(&["calls", "is_even", "--depth", "3"]);
    assert_eq!(level_of(&mutual, "is_odd"), 1, "got:\n{mutual}");
    assert!(
        mutual.contains("is_even (cycle)"),
        "the walk back to the root is a cycle, not an expansion:\n{mutual}"
    );

    // --- method chain through `self` -------------------------------------
    let methods = run_tyf(&["calls", "OrderPipeline.run", "--depth", "2"]);
    assert_eq!(level_of(&methods, "prepare"), 1, "self.prepare() resolves:\n{methods}");
    assert_eq!(level_of(&methods, "normalize"), 2, "self.normalize() resolves:\n{methods}");

    // --- depth clamp: 99 behaves as 5, and is not an error ---------------
    let clamped = run_tyf(&["calls", "OrderPipeline.run", "--depth", "99"]);
    let at_five = run_tyf(&["calls", "OrderPipeline.run", "--depth", "5"]);
    assert_eq!(clamped, at_five, "--depth 99 is clamped to the 5-level maximum");
    assert!(
        clamped.lines().map(indent_level).max().unwrap_or(0) <= 5,
        "nothing renders deeper than 5 levels:\n{clamped}"
    );

    // --- external leaves: name only by default, located with --external ---
    let plain = run_tyf(&["calls", "check_inventory", "--depth", "1"]);
    assert!(plain.contains("floor (external)"), "got:\n{plain}");
    assert!(
        !plain.contains(".pyi"),
        "no location for an external leaf without --external:\n{plain}"
    );

    let located = run_tyf(&["calls", "check_inventory", "--depth", "1", "--external"]);
    assert!(located.contains("(external)"), "got:\n{located}");
    assert!(located.contains(".pyi"), "--external adds the location:\n{located}");
    let external_line = located
        .lines()
        .find(|l| l.contains("floor"))
        .unwrap_or_else(|| panic!("no floor line:\n{located}"));
    assert_eq!(
        indent_level(external_line),
        1,
        "an external node is never expanded, so nothing sits below it:\n{located}"
    );

    // Spike finding 3: a callback passed as an argument is invisible.
    let chain_file = fixture("call_chain.py");
    let callback = run_tyf(&["calls", "apply_twice", "--file", &chain_file]);
    assert!(
        callback.contains("(no outgoing calls)"),
        "calling a parameter has no static target:\n{callback}"
    );

    // --- several symbols batch into one invocation -----------------------
    let batched = run_tyf(&["calls", "countdown", "validate_order", "--depth", "1"]);
    assert!(batched.contains("countdown (cycle)"), "got:\n{batched}");
    assert!(batched.contains("check_inventory"), "got:\n{batched}");
}

#[tokio::test]
async fn test_calls_incoming() {
    common::require_ty();
    if !common::has_call_hierarchy() {
        return;
    }

    // `--in` on the bottom of the chain returns the callers, one level per hop.
    let out = run_tyf(&["calls", "check_inventory", "--in", "--depth", "2"]);
    assert_eq!(level_of(&out, "check_inventory"), 0, "got:\n{out}");
    assert_eq!(level_of(&out, "validate_order"), 1, "direct caller:\n{out}");
    assert_eq!(level_of(&out, "charge_payment"), 1, "the other direct caller:\n{out}");
    assert_eq!(level_of(&out, "process_order"), 2, "the caller's caller:\n{out}");

    // Dedup applies in this direction too: `process_order` calls both
    // mid-level functions, so it is reached twice.
    assert!(
        out.contains("process_order \u{2191}"),
        "the repeated caller is marked, not re-expanded:\n{out}"
    );

    // `--in` differs from the default direction on the same symbol.
    assert_ne!(out, run_tyf(&["calls", "check_inventory", "--depth", "2"]));

    // Spike finding 3: `double` is only ever *passed* to `apply_twice`, so it
    // has no static callers.
    let chain_file = fixture("call_chain.py");
    let callback = run_tyf(&["calls", "double", "--in", "--file", &chain_file]);
    assert!(
        callback.contains("(no incoming calls)"),
        "a callback target has no statically visible callers:\n{callback}"
    );
}

#[tokio::test]
async fn test_calls_dotted_notation_and_usage_errors() {
    common::require_ty();
    if !common::has_call_hierarchy() {
        return;
    }

    // Happy path: `Class.method` narrows to the member, identically to show/refs.
    let out = run_tyf(&["calls", "OrderPipeline.prepare", "--depth", "1"]);
    assert_eq!(level_of(&out, "prepare"), 0, "got:\n{out}");
    assert_eq!(level_of(&out, "normalize"), 1, "got:\n{out}");

    // The standard usage errors, with the same exit code (2) and message as
    // show/find/refs — `calls` reuses the shared parser rather than its own.
    for token in ["Outer.Inner.method", ".leading", "trailing."] {
        let (_, stderr, status) = run_tyf_raw(&["calls", token]);
        assert_eq!(
            status.code(),
            Some(2),
            "'{token}' must be a usage error, got status {status:?}"
        );
        assert!(
            stderr.contains("dotted notation supports one level only"),
            "'{token}' should explain the one-level rule, got: {stderr}"
        );
    }

    // A well-formed but unmatched symbol is not an error: it exits 0 and says so.
    let (stdout, _, status) = run_tyf_raw(&["calls", "NoSuchClass.no_such_method"]);
    assert!(status.success(), "a valid-but-unmatched query exits 0");
    assert!(stdout.contains("No symbol found"), "got:\n{stdout}");

    // Same for a bare name that matches nothing.
    let (stdout, _, status) = run_tyf_raw(&["calls", "definitely_not_a_symbol_xyz"]);
    assert!(status.success(), "a valid-but-unmatched query exits 0");
    assert!(stdout.contains("No symbol found"), "got:\n{stdout}");
}

#[tokio::test]
async fn test_calls_output_formats() {
    common::require_ty();
    if !common::has_call_hierarchy() {
        return;
    }

    // JSON: nested, with the markers as explicit booleans.
    let json_out = run_tyf(&["--format", "json", "calls", "process_order"]);
    let parsed: serde_json::Value = serde_json::from_str(&json_out)
        .unwrap_or_else(|e| panic!("invalid JSON ({e}):\n{json_out}"));
    assert_eq!(parsed["direction"], "outgoing");
    assert_eq!(parsed["depth"], 2);

    let root = &parsed["entries"][0]["roots"][0];
    assert_eq!(root["name"], "process_order");
    assert_eq!(root["cycle"], false);
    assert_eq!(root["deduped"], false);
    assert_eq!(root["external"], false);

    // The diamond's second arm is flagged rather than duplicated.
    let deduped_names: Vec<&str> = root["children"]
        .as_array()
        .expect("children array")
        .iter()
        .flat_map(|c| c["children"].as_array().map_or(&[][..], Vec::as_slice))
        .filter(|n| n["deduped"] == true)
        .filter_map(|n| n["name"].as_str())
        .collect();
    assert_eq!(deduped_names, vec!["check_inventory"], "got:\n{json_out}");

    // CSV: flat rows carrying depth and the flag.
    let csv_out = run_tyf(&["--format", "csv", "calls", "process_order"]);
    let mut lines = csv_out.lines();
    assert_eq!(lines.next(), Some("symbol,depth,name,file,line,column,flag"));
    let rows: Vec<&str> = lines.filter(|l| !l.trim().is_empty()).collect();
    // Paths render relative to the process's working directory, as every other
    // tyf command does.
    assert!(
        rows[0].starts_with("process_order,0,process_order,") && rows[0].contains("call_chain.py,"),
        "root row: {}",
        rows[0]
    );
    assert!(
        rows.iter().any(|r| r.ends_with(",deduped") && r.contains("check_inventory")),
        "the deduped row carries its flag:\n{csv_out}"
    );
    assert!(
        rows.iter().all(|r| r.split(',').count() == 7),
        "every row has all 7 columns:\n{csv_out}"
    );

    // --detail full adds the call sites from `fromRanges`.
    let full = run_tyf(&["--detail", "full", "calls", "process_order", "--depth", "1"]);
    assert!(full.contains("[called at"), "got:\n{full}");
    let condensed = run_tyf(&["calls", "process_order", "--depth", "1"]);
    assert!(!condensed.contains("[called at"), "condensed omits call sites:\n{condensed}");
}

/// Regression: an empty `callHierarchy/*Calls` result is the *normal* answer
/// for a leaf function, not a cold-start miss.
///
/// Treating it as one means retrying with the full warm-up back-off at every
/// childless node — around 1.5 s each, multiplied by the fan-out. An 8-way
/// fan-out took ~12 s that way, and a 24-way one exceeded the 30 s request
/// timeout and failed outright, at the *default* depth.
///
/// The root `prepareCallHierarchy` already retries, and its non-empty result
/// proves the server is warm, so the walk itself must not retry.
#[tokio::test]
async fn test_calls_fanout_of_childless_callees_is_fast() {
    common::require_ty();
    if !common::has_call_hierarchy() {
        return;
    }

    // Warm the daemon first so this measures the walk, not daemon start-up.
    let _ = run_tyf(&["calls", "process_order", "--depth", "1"]);

    let started = std::time::Instant::now();
    let out = run_tyf(&["calls", "fan_out"]);
    let elapsed = started.elapsed();

    for i in 0..8 {
        let leaf = format!("fan_leaf_{i}");
        assert_eq!(level_of(&out, &leaf), 1, "{leaf} should be a direct callee:\n{out}");
    }

    // Generous bound: the fix runs this in tens of milliseconds, the bug in
    // ~12 s. Anything under 10 s cannot be the per-node retry ladder.
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "8 childless callees took {elapsed:?} — the walk is retrying empty results"
    );
}

/// Regression: dedup must not suppress a node that still had depth budget.
///
/// `budget_shared` is reached twice — first via `budget_mid` at the depth-2
/// boundary (no budget left, so no children), then directly at level 1 (budget
/// remaining). Marking the second occurrence `\u{2191}` claims its subtree was
/// shown above when it was not, so `budget_leaf` — legitimately two hops from
/// the root — disappears from a `--depth 2` walk entirely.
#[tokio::test]
async fn test_calls_dedup_respects_remaining_depth() {
    common::require_ty();
    if !common::has_call_hierarchy() {
        return;
    }

    let out = run_tyf(&["calls", "budget_root", "--depth", "2"]);

    assert_eq!(level_of(&out, "budget_mid"), 1, "got:\n{out}");
    assert_eq!(
        out.matches("budget_shared").count(),
        2,
        "budget_shared is reached down both branches:\n{out}"
    );
    assert!(
        out.contains("budget_leaf"),
        "budget_leaf is 2 hops from the root and must appear in a depth-2 walk:\n{out}"
    );
    // The truncated first encounter must not suppress the one that still had
    // budget — that is what used to drop `budget_leaf` from the tree.
    assert!(
        !out.contains('\u{2191}'),
        "the deeper encounter was truncated, so it suppresses nothing:\n{out}"
    );
}

/// Dedup is scoped to one tree. An up-arrow in one symbol's output pointing
/// into a *different* symbol's output would be unreadable — each tree has to
/// stand on its own, which is what the rendered legend and the docs promise.
#[tokio::test]
async fn test_calls_dedup_does_not_leak_between_symbols() {
    common::require_ty();
    if !common::has_call_hierarchy() {
        return;
    }

    // Both callers reach `check_inventory`; queried together, each tree must
    // show it in full.
    let out = run_tyf(&["calls", "validate_order", "charge_payment", "--depth", "1"]);

    assert_eq!(
        out.matches("check_inventory").count(),
        2,
        "each tree resolves check_inventory independently:\n{out}"
    );
    assert!(!out.contains('\u{2191}'), "no dedup marker across separate symbols:\n{out}");

    // Queried alone, the second symbol renders identically.
    let alone = run_tyf(&["calls", "charge_payment", "--depth", "1"]);
    assert!(out.contains(alone.trim()), "batched output matches the standalone tree:\n{out}");
}

/// `--depth 0` is clamped up to 1, mirroring how the upper bound is clamped
/// rather than rejected. Rendering a bare root would make the formatter report
/// "no outgoing calls" for a function that has two.
#[tokio::test]
async fn test_calls_depth_zero_is_clamped_to_one() {
    common::require_ty();
    if !common::has_call_hierarchy() {
        return;
    }

    let (out, _, status) = run_tyf_raw(&["calls", "process_order", "--depth", "0"]);
    assert!(status.success(), "--depth 0 is clamped, not an error");
    assert_eq!(out, run_tyf(&["calls", "process_order", "--depth", "1"]));
    assert!(!out.contains("no outgoing calls"), "process_order has outgoing calls:\n{out}");
}

/// Spike finding 4: `prepareCallHierarchy` succeeds on a class but both
/// directions are empty — construction is not modelled as a call. Reporting
/// that plainly (and exiting 0) is the honest outcome.
#[tokio::test]
async fn test_calls_on_a_class_reports_no_calls() {
    common::require_ty();
    if !common::has_call_hierarchy() {
        return;
    }

    let (stdout, _, status) = run_tyf_raw(&["calls", "OrderPipeline"]);
    assert!(status.success(), "a class is not an error");
    assert!(stdout.contains("OrderPipeline"), "got:\n{stdout}");
    assert!(stdout.contains("(no outgoing calls)"), "got:\n{stdout}");
}
