//! `tyf guide` prints embedded instructions and must never touch the daemon.
//!
//! Every daemon-backed command auto-starts the daemon on first use. The guide
//! is the one command an agent runs *before* the project is set up, often via
//! `uvx ty-find guide` in a directory with no ty at all, so it has to
//! short-circuit ahead of workspace detection, the socket, and the LSP.
//!
//! Page content rules (line cap, ASCII, the `next:` line) are unit tests in
//! `src/guide.rs`; this file covers what only a real process can prove:
//! daemon isolation, cwd-based detection, and the second binary.

#[path = "common.rs"]
mod common;

use assert_cmd::cargo::{cargo_bin, cargo_bin_cmd};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where a project install puts the binary; mirrors `VENV_BINARY` in
/// `src/guide.rs`.
#[cfg(unix)]
const VENV_BINARY: &str = ".venv/bin/tyf";
#[cfg(not(unix))]
const VENV_BINARY: &str = ".venv/Scripts/tyf.exe";

/// Everything the daemon leaves in `/tmp`, so a test can prove it never ran.
#[cfg(unix)]
fn daemon_files() -> Vec<PathBuf> {
    vec![common::daemon_socket_path(), common::daemon_pidfile_path()]
}

#[cfg(not(unix))]
fn daemon_files() -> Vec<PathBuf> {
    Vec::new()
}

/// Snapshot of daemon state: the contents of each of its files (`None` when
/// absent) and the daemon processes started from `bin`. Compared before and
/// after, so a daemon the developer already has running neither masks nor
/// fakes a failure: a freshly started daemon rewrites the pidfile with its
/// own pid, and spawns from `current_exe()`, which is `bin`.
#[derive(Debug, PartialEq, Eq)]
struct DaemonState {
    files: Vec<(PathBuf, Option<Vec<u8>>)>,
    pids: HashSet<String>,
}

fn daemon_state(bin: &Path) -> DaemonState {
    DaemonState {
        files: daemon_files().into_iter().map(|p| (p.clone(), std::fs::read(&p).ok())).collect(),
        pids: common::daemon_pids(bin),
    }
}

/// A private copy of the built `tyf`, so the process check matches only
/// daemons this test could have spawned, not ones the other integration
/// suites start from the shared `target/debug/tyf` at the same time.
fn private_binary(dir: &Path) -> PathBuf {
    let copy = dir.join("tyf");
    std::fs::copy(cargo_bin!("tyf"), &copy).expect("copy tyf into the tempdir");
    copy
}

/// Run `tyf <args>` in `cwd` and return stdout, asserting success and that
/// the daemon state is byte-for-byte what it was before.
fn run_without_daemon(cwd: &Path, args: &[&str]) -> String {
    let scratch = tempfile::tempdir().expect("tempdir");
    let bin = private_binary(scratch.path());
    let before = daemon_state(&bin);

    let output =
        Command::new(&bin).args(args).current_dir(cwd).output().expect("failed to run tyf");

    let after = daemon_state(&bin);
    assert_eq!(
        before,
        after,
        "`tyf {}` must not start the daemon or create its files",
        args.join(" ")
    );
    assert!(
        output.status.success(),
        "`tyf {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn write(dir: &Path, rel: &str, contents: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().expect("has parent")).expect("mkdir");
    std::fs::write(path, contents).expect("write");
}

/// A project that has ty-find installed: `pyproject.toml` plus the venv
/// binary maturin/uv put there.
fn configured_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "pyproject.toml", "[project]\nname = \"demo\"\n");
    write(dir.path(), VENV_BINARY, "");
    dir
}

#[test]
fn guide_with_no_project_prints_setup_and_starts_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = run_without_daemon(dir.path(), &["guide"]);
    assert!(out.starts_with("# ty-find: setup"), "expected the setup page, got:\n{out}");
    assert!(out.contains("uv add --dev"), "setup must say how to install:\n{out}");
}

#[test]
fn guide_in_a_project_without_the_venv_binary_prints_setup() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "pyproject.toml", "[project]\nname = \"demo\"\n");
    let out = run_without_daemon(dir.path(), &["guide"]);
    assert!(out.starts_with("# ty-find: setup"), "expected the setup page, got:\n{out}");
}

#[test]
fn guide_in_a_configured_project_prints_use() {
    let dir = configured_project();
    let out = run_without_daemon(dir.path(), &["guide"]);
    assert!(out.starts_with("# ty-find: use"), "expected the use page, got:\n{out}");
    assert!(out.contains("daemon status"), "use must point at daemon status:\n{out}");
}

/// Detection walks up from cwd: a subdirectory of a configured project is
/// still that project.
#[test]
fn guide_detects_the_project_from_a_subdirectory() {
    let dir = configured_project();
    let sub = dir.path().join("src").join("demo");
    std::fs::create_dir_all(&sub).expect("mkdir");
    let out = run_without_daemon(&sub, &["guide"]);
    assert!(out.starts_with("# ty-find: use"), "expected the use page, got:\n{out}");
}

#[test]
fn guide_pages_are_always_reachable_by_name() {
    let empty = tempfile::tempdir().expect("tempdir");
    let out = run_without_daemon(empty.path(), &["guide", "use"]);
    assert!(out.starts_with("# ty-find: use"), "explicit `use` must win over detection:\n{out}");

    let configured = configured_project();
    let out = run_without_daemon(configured.path(), &["guide", "setup"]);
    assert!(
        out.starts_with("# ty-find: setup"),
        "explicit `setup` must win over detection:\n{out}"
    );
}

#[test]
fn help_and_version_start_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let help = run_without_daemon(dir.path(), &["--help"]);
    assert!(help.contains("guide"), "guide must be listed in --help:\n{help}");
    let version = run_without_daemon(dir.path(), &["--version"]);
    assert!(version.contains(env!("CARGO_PKG_VERSION")), "unexpected --version output: {version}");
}

/// `uvx ty-find` looks for an executable named after the package, so the
/// crate ships `ty-find`: a wrapper that execs the `tyf` next to it.
#[test]
fn ty_find_binary_runs_the_guide_too() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = cargo_bin_cmd!("ty-find")
        .arg("guide")
        .current_dir(dir.path())
        .output()
        .expect("failed to run ty-find");
    assert!(output.status.success());
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.starts_with("# ty-find: setup"), "got:\n{out}");
}

/// The wrapper forwards arguments and the exit code unchanged: a usage error
/// in `tyf` is a usage error in `ty-find`.
#[test]
fn ty_find_binary_forwards_exit_codes() {
    let output = cargo_bin_cmd!("ty-find")
        .args(["guide", "no-such-page"])
        .output()
        .expect("failed to run ty-find");
    assert_eq!(output.status.code(), Some(2), "clap usage errors exit 2");
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("no-such-page"), "stderr should name the bad page:\n{err}");
}
