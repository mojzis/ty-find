//! Comprehensive regression tests exercising the expanded `test_project/` fixture.
//!
//! Strategy:
//! 1. `list` every file and verify expected symbols appear.
//! 2. `find` each symbol and verify its file is reported.
//! 3. `inspect` each symbol and verify hover info is present.
//! 4. `members` each class and verify expected members.
//! 5. `refs` for symbols used across files.
//!
//! All sub-cases live inside a single `#[tokio::test]` to avoid flaky failures
//! from concurrent daemon access (each test process talks to the shared daemon
//! socket, so parallel execution can cause race conditions).

#[path = "common.rs"]
mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::path::PathBuf;

// ── helpers ──────────────────────────────────────────────────────────

fn test_project_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("test_project")
}

/// Run tyf with the given arguments against `test_project` and return stdout.
/// Panics if the command exits with a non-zero status.
fn run_tyf(args: &[&str]) -> String {
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(test_project_root());
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("failed to run tyf");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "tyf {args:?} failed.\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

/// Run tyf and return stderr (for commands that report errors to stderr).
fn run_tyf_stderr(args: &[&str]) -> String {
    let mut cmd = cargo_bin_cmd!("tyf");
    cmd.arg("--workspace").arg(test_project_root());
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("failed to run tyf");
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn file_path(name: &str) -> String {
    test_project_root().join(name).to_string_lossy().to_string()
}

// ── assertion helpers ────────────────────────────────────────────────

fn assert_contains(haystack: &str, needle: &str, context: &str) {
    assert!(
        predicate::str::contains(needle).eval(haystack),
        "{context} — expected to contain '{needle}', got:\n{haystack}"
    );
}

fn assert_not_contains(haystack: &str, needle: &str, context: &str) {
    assert!(
        !predicate::str::contains(needle).eval(haystack),
        "{context} — expected NOT to contain '{needle}', got:\n{haystack}"
    );
}

/// Verify `list` output contains all expected symbol names.
fn assert_list_contains_symbols(file: &str, expected: &[&str]) {
    let out = run_tyf(&["list", &file_path(file)]);
    for sym in expected {
        assert_contains(&out, sym, &format!("list {file}"));
    }
}

/// Verify `find` locates a symbol in the expected file.
fn assert_find(symbol: &str, expected_file: &str) {
    let out = run_tyf(&["find", symbol]);
    assert_contains(&out, expected_file, &format!("find {symbol}"));
}

/// Verify `find --file` locates a symbol in the expected file.
fn assert_find_in_file(symbol: &str, file: &str) {
    let fpath = file_path(file);
    let out = run_tyf(&["find", symbol, "--file", &fpath]);
    assert_contains(&out, file, &format!("find {symbol} --file {file}"));
}

/// Verify `inspect` returns hover info (not "(none)" / "No hover").
fn assert_inspect(symbol: &str) {
    let out = run_tyf(&["inspect", symbol]);
    assert_not_contains(&out, "No hover information", &format!("inspect {symbol}"));
    // At minimum the symbol name or its file should appear
    assert!(out.len() > 20, "inspect {symbol} returned suspiciously short output:\n{out}");
}

/// Verify `inspect --file` returns hover info.
fn assert_inspect_in_file(symbol: &str, file: &str) {
    let fpath = file_path(file);
    let out = run_tyf(&["inspect", symbol, "--file", &fpath]);
    assert_not_contains(&out, "No hover information", &format!("inspect {symbol} --file {file}"));
}

/// Verify `members` for a class shows expected member names.
fn assert_members(class_name: &str, expected_members: &[&str]) {
    let out = run_tyf(&["members", class_name]);
    assert_contains(&out, class_name, &format!("members {class_name}"));
    for member in expected_members {
        assert_contains(&out, member, &format!("members {class_name}"));
    }
}

/// Verify `members --file` for a class shows expected member names.
fn assert_members_in_file(class_name: &str, file: &str, expected_members: &[&str]) {
    let fpath = file_path(file);
    let out = run_tyf(&["members", class_name, "--file", &fpath]);
    assert_contains(&out, class_name, &format!("members {class_name} --file {file}"));
    for member in expected_members {
        assert_contains(&out, member, &format!("members {class_name} --file {file}"));
    }
}

/// Verify `members --all` includes dunder methods.
fn assert_members_all(class_name: &str, expected_dunders: &[&str]) {
    let out = run_tyf(&["members", class_name, "--all"]);
    for dunder in expected_dunders {
        assert_contains(&out, dunder, &format!("members {class_name} --all"));
    }
}

/// Verify `refs` finds references across files.
fn assert_refs(symbol: &str, expected_files: &[&str]) {
    let out = run_tyf(&["refs", symbol]);
    assert_not_contains(&out, "No references found", &format!("refs {symbol}"));
    for f in expected_files {
        assert_contains(&out, f, &format!("refs {symbol}"));
    }
}

/// Verify `refs --file` finds references.
fn assert_refs_in_file(symbol: &str, file: &str) {
    let fpath = file_path(file);
    let out = run_tyf(&["refs", symbol, "--file", &fpath]);
    assert_not_contains(&out, "No references found", &format!("refs {symbol} --file {file}"));
}

// ── The test ─────────────────────────────────────────────────────────

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_complex_project_comprehensive() {
    common::require_ty();

    // ================================================================
    // 1. LIST — verify document symbols for every file
    // ================================================================

    // models.py
    assert_list_contains_symbols(
        "models.py",
        &[
            "Animal",
            "Dog",
            "Cat",
            "ServiceDog",
            "Config",
            "AppConfig",
            "MAX_ANIMALS",
            "create_dog",
            "create_cat",
        ],
    );

    // main.py
    assert_list_contains_symbols(
        "main.py",
        &[
            "list_animals",
            "demo_models",
            "demo_decorators",
            "demo_protocols",
            "demo_enums",
            "demo_generics",
            "demo_patterns",
            "demo_typing",
            "demo_exceptions",
            "main",
        ],
    );

    // decorators.py
    assert_list_contains_symbols(
        "decorators.py",
        &[
            "log_calls",
            "retry",
            "validate_positive",
            "singleton",
            "greet",
            "fetch_data",
            "square_root",
            "DatabaseConnection",
            "Cached",
            "MathService",
        ],
    );

    // protocols.py
    assert_list_contains_symbols(
        "protocols.py",
        &[
            "Shape",
            "Circle",
            "Rectangle",
            "Drawable",
            "Serializable",
            "Canvas",
            "Widget",
            "Button",
            "TextInput",
            "calculate_total_area",
            "render_widgets",
        ],
    );

    // enums.py
    assert_list_contains_symbols(
        "enums.py",
        &[
            "Color",
            "Priority",
            "HttpMethod",
            "TaskStatus",
            "Permission",
            "DEFAULT_COLOR",
            "DEFAULT_PRIORITY",
            "format_task",
            "parse_permissions",
        ],
    );

    // generics.py
    assert_list_contains_symbols(
        "generics.py",
        &[
            "Stack",
            "TreeNode",
            "Registry",
            "Page",
            "first",
            "clamp",
            "merge_registries",
            "paginate",
        ],
    );

    // patterns.py
    assert_list_contains_symbols(
        "patterns.py",
        &[
            "Outer",
            "make_multiplier",
            "make_accumulator",
            "compose",
            "fibonacci",
            "chunked",
            "RangeIterator",
            "Timer",
            "temporary_value",
            "Point",
            "Point3D",
            "SingletonMeta",
            "AppConfig",
            "MAX_RETRIES",
            "DEFAULT_TIMEOUT",
            "identity",
        ],
    );

    // async_code.py
    assert_list_contains_symbols(
        "async_code.py",
        &[
            "fetch_url",
            "fetch_all",
            "process_item",
            "stream_items",
            "filtered_stream",
            "AsyncResource",
            "AsyncPool",
            "AsyncIterableRange",
            "DEFAULT_POOL_SIZE",
            "CONNECTION_TIMEOUT",
        ],
    );

    // exceptions.py
    assert_list_contains_symbols(
        "exceptions.py",
        &[
            "AppError",
            "ValidationError",
            "NotFoundError",
            "AuthenticationError",
            "AuthorizationError",
            "RateLimitError",
            "DatabaseError",
            "ConnectionError",
            "QueryError",
            "validate_email",
            "get_user",
            "ERROR_CODES",
        ],
    );

    // typing_extras.py
    assert_list_contains_symbols(
        "typing_extras.py",
        &[
            "Address",
            "UserProfile",
            "Coordinate",
            "RGB",
            "move",
            "log",
            "to_number",
            "first_or_none",
            "flatten_config",
            "transpose",
            "Config",
            "ORIGIN",
            "WHITE",
            "BLACK",
        ],
    );

    // ================================================================
    // 2. FIND — locate definitions via workspace symbols
    // ================================================================

    // models.py symbols
    assert_find("Animal", "models.py");
    assert_find("Dog", "models.py");
    assert_find("Cat", "models.py");
    assert_find("ServiceDog", "models.py");
    assert_find("create_dog", "models.py");
    assert_find("create_cat", "models.py");
    assert_find("MAX_ANIMALS", "models.py");

    // decorators.py symbols
    assert_find("log_calls", "decorators.py");
    assert_find("retry", "decorators.py");
    assert_find("singleton", "decorators.py");
    assert_find("greet", "decorators.py");
    assert_find("fetch_data", "decorators.py");
    assert_find("DatabaseConnection", "decorators.py");
    assert_find("MathService", "decorators.py");

    // protocols.py symbols
    assert_find("Shape", "protocols.py");
    assert_find("Circle", "protocols.py");
    assert_find("Rectangle", "protocols.py");
    assert_find("Button", "protocols.py");
    assert_find("TextInput", "protocols.py");
    assert_find("calculate_total_area", "protocols.py");

    // enums.py symbols
    assert_find("Color", "enums.py");
    assert_find("Priority", "enums.py");
    assert_find("HttpMethod", "enums.py");
    assert_find("TaskStatus", "enums.py");
    assert_find("Permission", "enums.py");
    assert_find("format_task", "enums.py");

    // generics.py symbols
    assert_find("Stack", "generics.py");
    assert_find("TreeNode", "generics.py");
    assert_find("Registry", "generics.py");
    assert_find("Page", "generics.py");
    assert_find("paginate", "generics.py");

    // patterns.py symbols
    assert_find("Outer", "patterns.py");
    assert_find("fibonacci", "patterns.py");
    assert_find("Timer", "patterns.py");
    assert_find("Point", "patterns.py");
    assert_find("Point3D", "patterns.py");
    assert_find("SingletonMeta", "patterns.py");

    // async_code.py symbols
    assert_find("fetch_url", "async_code.py");
    assert_find("fetch_all", "async_code.py");
    assert_find("AsyncResource", "async_code.py");
    assert_find("AsyncPool", "async_code.py");
    assert_find("AsyncIterableRange", "async_code.py");

    // exceptions.py symbols
    assert_find("AppError", "exceptions.py");
    assert_find("ValidationError", "exceptions.py");
    assert_find("NotFoundError", "exceptions.py");
    assert_find("DatabaseError", "exceptions.py");
    assert_find("validate_email", "exceptions.py");

    // typing_extras.py symbols
    assert_find("Coordinate", "typing_extras.py");
    assert_find("RGB", "typing_extras.py");
    assert_find("transpose", "typing_extras.py");

    // ================================================================
    // 3. FIND --file — file-scoped lookups
    // ================================================================

    assert_find_in_file("Animal", "models.py");
    assert_find_in_file("greet", "decorators.py");
    assert_find_in_file("Circle", "protocols.py");
    assert_find_in_file("Color", "enums.py");
    assert_find_in_file("Stack", "generics.py");
    assert_find_in_file("Outer", "patterns.py");
    assert_find_in_file("fetch_url", "async_code.py");
    assert_find_in_file("AppError", "exceptions.py");
    assert_find_in_file("Coordinate", "typing_extras.py");
    assert_find_in_file("list_animals", "main.py");

    // ================================================================
    // 4. INSPECT — hover info for every kind of symbol
    // ================================================================

    // Plain classes
    assert_inspect("Animal");
    assert_inspect("Dog");
    assert_inspect("Cat");
    assert_inspect("ServiceDog");

    // Dataclass
    assert_inspect_in_file("Config", "models.py");
    assert_inspect_in_file("AppConfig", "models.py");

    // Decorated function
    assert_inspect("greet");

    // Decorator factory
    assert_inspect("retry");

    // Singleton class via decorator
    assert_inspect("DatabaseConnection");

    // ABC and Protocol
    assert_inspect("Shape");
    assert_inspect("Circle");
    assert_inspect("Rectangle");

    // Widget hierarchy
    assert_inspect("Widget");
    assert_inspect("Button");
    assert_inspect("TextInput");

    // Enums
    assert_inspect("Color");
    assert_inspect("Priority");
    assert_inspect("HttpMethod");
    assert_inspect("TaskStatus");

    // Generics
    assert_inspect("Stack");
    assert_inspect("TreeNode");
    assert_inspect("Registry");
    assert_inspect("Page");

    // Patterns
    assert_inspect("Outer");
    assert_inspect("Timer");
    assert_inspect("Point");
    assert_inspect("Point3D");
    assert_inspect("SingletonMeta");

    // Async
    assert_inspect("AsyncResource");
    assert_inspect("AsyncPool");
    assert_inspect("AsyncIterableRange");

    // Exceptions
    assert_inspect("AppError");
    assert_inspect("ValidationError");
    assert_inspect("NotFoundError");
    assert_inspect("DatabaseError");

    // Typing extras
    assert_inspect("Coordinate");
    assert_inspect("RGB");

    // Functions
    assert_inspect("create_dog");
    assert_inspect("create_cat");
    assert_inspect("calculate_total_area");
    assert_inspect("format_task");
    assert_inspect("paginate");
    assert_inspect("fibonacci");
    assert_inspect("validate_email");
    assert_inspect("transpose");

    // ================================================================
    // 5. INSPECT --file (file-scoped inspect)
    // ================================================================

    assert_inspect_in_file("Animal", "models.py");
    assert_inspect_in_file("greet", "decorators.py");
    assert_inspect_in_file("Shape", "protocols.py");
    assert_inspect_in_file("Color", "enums.py");
    assert_inspect_in_file("Stack", "generics.py");
    assert_inspect_in_file("Outer", "patterns.py");
    assert_inspect_in_file("AsyncPool", "async_code.py");
    assert_inspect_in_file("AppError", "exceptions.py");
    assert_inspect_in_file("Coordinate", "typing_extras.py");

    // ================================================================
    // 6. MEMBERS — class public interfaces
    // ================================================================

    // models.py classes
    assert_members("Animal", &["speak"]);
    assert_members("Dog", &["fetch"]);
    assert_members("Cat", &["purr"]);
    assert_members_in_file(
        "ServiceDog",
        "models.py",
        &["assist", "report_status", "is_certified", "from_registry"],
    );

    // decorators.py classes
    assert_members_in_file(
        "DatabaseConnection",
        "decorators.py",
        &["connect", "disconnect", "is_connected"],
    );
    assert_members_in_file(
        "MathService",
        "decorators.py",
        &["expensive_computation", "add", "create_default"],
    );

    // protocols.py classes
    assert_members_in_file("Shape", "protocols.py", &["area", "perimeter", "describe"]);
    assert_members_in_file("Circle", "protocols.py", &["area", "perimeter", "diameter"]);
    assert_members_in_file("Rectangle", "protocols.py", &["area", "perimeter", "is_square"]);
    assert_members_in_file("Button", "protocols.py", &["render", "click", "display_name"]);
    assert_members_in_file("TextInput", "protocols.py", &["render", "set_value", "display_name"]);

    // enums.py classes
    assert_members_in_file("Color", "enums.py", &["hex_code"]);
    assert_members_in_file("Priority", "enums.py", &["label", "is_urgent"]);
    assert_members_in_file("HttpMethod", "enums.py", &["is_safe", "is_idempotent", "from_string"]);
    assert_members_in_file("TaskStatus", "enums.py", &["can_transition_to", "is_terminal"]);

    // generics.py classes
    assert_members_in_file("Stack", "generics.py", &["push", "pop", "peek", "is_empty", "size"]);
    assert_members_in_file("TreeNode", "generics.py", &["is_leaf", "depth"]);
    assert_members_in_file(
        "Registry",
        "generics.py",
        &["register", "lookup", "all_keys", "all_values", "count"],
    );
    assert_members_in_file("Page", "generics.py", &["total_pages", "has_next", "has_previous"]);

    // patterns.py classes
    assert_members_in_file("Outer", "patterns.py", &["create_inner"]);
    assert_members_in_file(
        "RangeIterator",
        "patterns.py",
        &[], // only has dunder methods publicly
    );
    assert_members_in_file("Timer", "patterns.py", &[]);
    assert_members_in_file("Point", "patterns.py", &["distance_to"]);
    assert_members_in_file("Point3D", "patterns.py", &["distance_to"]);

    // async_code.py classes
    assert_members_in_file("AsyncResource", "async_code.py", &["read"]);
    assert_members_in_file(
        "AsyncPool",
        "async_code.py",
        &["acquire", "release", "execute", "active_count", "close_all"],
    );

    // exceptions.py classes
    assert_members_in_file("AppError", "exceptions.py", &["error_id"]);
    assert_members_in_file("RateLimitError", "exceptions.py", &["retry_after"]);

    // typing_extras.py classes
    assert_members_in_file("Coordinate", "typing_extras.py", &["distance_to"]);
    assert_members_in_file("RGB", "typing_extras.py", &["hex", "from_hex"]);
    assert_members_in_file("Config", "typing_extras.py", &["to_dict", "from_dict"]);

    // ================================================================
    // 7. MEMBERS --all (dunder/private inclusion)
    // ================================================================

    assert_members_all("Animal", &["__init__"]);
    assert_members_all("Point", &["__repr__"]);
    assert_members_all("AsyncResource", &["__aenter__", "__aexit__"]);

    // ================================================================
    // 8. MEMBERS on non-class — expect error
    // ================================================================

    let stderr = run_tyf_stderr(&["members", "create_dog", "--file", &file_path("models.py")]);
    assert_contains(&stderr, "not a class", "members on function");

    let stderr =
        run_tyf_stderr(&["members", "validate_email", "--file", &file_path("exceptions.py")]);
    assert_contains(&stderr, "not a class", "members on function");

    // ================================================================
    // 9. REFS — cross-file reference tracking
    // ================================================================

    // Animal is defined in models.py and imported/used in main.py
    assert_refs("Animal", &["models.py", "main.py"]);

    // create_dog is defined in models.py and used in main.py
    assert_refs("create_dog", &["models.py", "main.py"]);

    // Circle is defined in protocols.py and used in main.py
    assert_refs("Circle", &["protocols.py", "main.py"]);

    // Stack is defined in generics.py and used in main.py
    assert_refs("Stack", &["generics.py", "main.py"]);

    // ValidationError is defined in exceptions.py and used in main.py
    assert_refs("ValidationError", &["exceptions.py", "main.py"]);

    // greet is defined in decorators.py and used in main.py
    assert_refs("greet", &["decorators.py", "main.py"]);

    // ================================================================
    // 10. REFS --file (file-scoped references)
    // ================================================================

    assert_refs_in_file("Animal", "models.py");
    assert_refs_in_file("greet", "decorators.py");
    assert_refs_in_file("Shape", "protocols.py");

    // ================================================================
    // 11. FIND --fuzzy (prefix/partial matching)
    // ================================================================

    // "fetch_" should match fetch_url, fetch_all, fetch_data
    let out = run_tyf(&["find", "fetch_", "--fuzzy"]);
    assert_contains(&out, "fetch_", "fuzzy find fetch_");

    // "Async" should match AsyncResource, AsyncPool, AsyncIterableRange
    let out = run_tyf(&["find", "Async", "--fuzzy"]);
    assert_contains(&out, "Async", "fuzzy find Async");

    // "validate" should match validate_email, validate_positive
    let out = run_tyf(&["find", "validate", "--fuzzy"]);
    assert_contains(&out, "validate", "fuzzy find validate");

    // ================================================================
    // 12. MULTI-SYMBOL — batch operations
    // ================================================================

    // find multiple symbols in one call
    let out = run_tyf(&["find", "Animal", "Circle", "Stack"]);
    assert_contains(&out, "models.py", "multi find Animal");
    assert_contains(&out, "protocols.py", "multi find Circle");
    assert_contains(&out, "generics.py", "multi find Stack");

    // inspect multiple symbols in one call
    let out = run_tyf(&["inspect", "Animal", "Circle"]);
    assert_contains(&out, "Animal", "multi inspect");
    assert_contains(&out, "Circle", "multi inspect");

    // members multiple classes in one call
    let out = run_tyf(&["members", "Animal", "Dog"]);
    assert_contains(&out, "Animal", "multi members");
    assert_contains(&out, "Dog", "multi members");

    // refs multiple symbols in one call
    let out = run_tyf(&["refs", "Animal", "create_dog"]);
    assert_contains(&out, "Animal", "multi refs");
    assert_contains(&out, "create_dog", "multi refs");

    // ================================================================
    // 13. OUTPUT FORMATS — verify JSON and CSV for various commands
    // ================================================================

    // JSON find
    let out = run_tyf(&["--format", "json", "find", "Animal"]);
    assert_contains(&out, "\"uri\"", "json find");

    // JSON inspect
    let out = run_tyf(&["--format", "json", "inspect", "Animal"]);
    assert_contains(&out, "\"symbol\"", "json inspect");

    // JSON members
    let out =
        run_tyf(&["--format", "json", "members", "Animal", "--file", &file_path("models.py")]);
    assert_contains(&out, "\"class_name\"", "json members");
    assert_contains(&out, "\"members\"", "json members");

    // JSON refs (enriched format uses "file" instead of "uri")
    let out = run_tyf(&["--format", "json", "refs", "Animal", "--file", &file_path("models.py")]);
    assert_contains(&out, "\"file\"", "json refs");
    assert_contains(&out, "\"reference_count\"", "json refs");
    assert_contains(&out, "\"context\"", "json refs");

    // CSV find
    let out = run_tyf(&["--format", "csv", "find", "Animal"]);
    assert_contains(&out, "file,line,column", "csv find header");

    // CSV members
    let out = run_tyf(&["--format", "csv", "members", "Animal", "--file", &file_path("models.py")]);
    assert_contains(&out, "class,member,kind,signature,line,column", "csv members header");

    // Paths format
    let out = run_tyf(&["--format", "paths", "find", "Animal"]);
    assert_contains(&out, "models.py", "paths find");
    // paths format should be clean file paths, not JSON
    assert_not_contains(&out, "\"uri\"", "paths should not have JSON");
}
