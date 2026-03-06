use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{Context, Result};

/// A debug log writer that captures the full request lifecycle to a temp file.
///
/// When `--debug` is passed, a `DebugLog` is created and threaded through the
/// call chain. Each method appends a timestamped section to the log file.
/// At the end of execution, the path is printed so the user can inspect it.
pub struct DebugLog {
    writer: Mutex<BufWriter<File>>,
    path: PathBuf,
    start: Instant,
}

impl DebugLog {
    /// Create a new debug log file in `/tmp/tyf-debug-{timestamp}-{pid}.log`.
    pub fn create() -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let pid = std::process::id();
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);

        let path = PathBuf::from(format!("/tmp/tyf-debug-{timestamp}-{pid}-{seq}.log"));
        let file = File::create(&path)
            .with_context(|| format!("Failed to create debug log at {}", path.display()))?;
        let writer = Mutex::new(BufWriter::new(file));

        Ok(Self { writer, path, start: Instant::now() })
    }

    /// Path to the debug log file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write a timestamped line to the log.
    fn write_line(&self, line: &str) {
        let elapsed = self.start.elapsed();
        let secs = elapsed.as_secs();
        let millis = elapsed.subsec_millis();
        if let Ok(mut w) = self.writer.lock() {
            let _ = writeln!(w, "[{secs:>3}.{millis:03}s] {line}");
        }
    }

    /// Write raw text without timestamp prefix.
    fn write_raw(&self, text: &str) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = write!(w, "{text}");
        }
    }

    /// Log the CLI arguments.
    pub fn log_cli_args(&self, args: &[String]) {
        self.write_line(&format!("CLI args: {}", args.join(" ")));
    }

    /// Log workspace resolution details.
    pub fn log_workspace_resolution(
        &self,
        cwd: &Path,
        workspace_root: &Path,
        explicit_workspace: Option<&Path>,
        detection_method: &str,
    ) {
        self.write_line("Workspace resolution:");
        self.write_raw(&format!("           CWD: {}\n", cwd.display()));
        self.write_raw(&format!(
            "           Detected workspace root: {}\n",
            workspace_root.display()
        ));
        if let Some(ws) = explicit_workspace {
            self.write_raw(&format!("           (overridden by --workspace {})\n", ws.display()));
        }
        self.write_raw(&format!("           Detection method: {detection_method}\n"));
    }

    /// Log daemon connection details.
    pub fn log_daemon_connection(&self, socket_path: &str, connected: bool, error: Option<&str>) {
        self.write_line("Daemon connection:");
        self.write_raw(&format!("           Socket path: {socket_path}\n"));
        if connected {
            self.write_raw("           Connected: yes\n".to_string().as_str());
        } else if let Some(err) = error {
            self.write_raw(&format!("           Connection failed: {err}\n"));
        }
    }

    /// Log daemon version info.
    pub fn log_daemon_version(&self, daemon_version: &str, client_version: &str) {
        let matches = daemon_version == client_version;
        self.write_raw(&format!(
            "           Daemon version: {daemon_version} (matches CLI: {matches})\n"
        ));
    }

    /// Log an outgoing RPC request with the full JSON payload.
    pub fn log_rpc_request(&self, method: &str, params_json: &str) {
        self.write_line("RPC request sent:");
        self.write_raw(&format!("           Method: {method}\n"));
        self.write_raw(&format!("           Params: {params_json}\n"));
    }

    /// Log an incoming RPC response with timing and the full JSON payload.
    pub fn log_rpc_response(&self, elapsed_ms: u128, success: bool, response_json: &str) {
        let status = if success { "success" } else { "error" };
        self.write_line(&format!("RPC response received ({elapsed_ms}ms):"));
        self.write_raw(&format!("           Status: {status}\n"));
        self.write_raw(&format!("           Result: {response_json}\n"));
    }

    /// Log the final result summary.
    pub fn log_result_summary(&self, summary: &str) {
        self.write_line(&format!("Result: {summary}"));
    }

    /// Write the reproduction commands section at the end of the log.
    pub fn log_reproduction_commands(
        &self,
        workspace_root: &Path,
        symbols: &[String],
        command: &str,
    ) {
        self.write_raw("\n--- Reproduction commands ---\n");

        let mut cmds = String::new();
        let ws = workspace_root.display();

        let _ = writeln!(cmds, "# To test workspace detection:");
        let _ = writeln!(cmds, "tyf daemon status");
        let _ = writeln!(cmds, "tyf --workspace {ws} {command}");

        if !symbols.is_empty() {
            let _ = writeln!(cmds, "\n# To verify ty can see these symbols directly:");
            for sym in symbols {
                let _ = writeln!(cmds, "tyf --workspace {ws} find {sym}");
                let _ = writeln!(cmds, "tyf --workspace {ws} find {sym} --fuzzy");
            }
        }

        let _ = writeln!(cmds, "\n# For daemon-side LSP details, run with RUST_LOG:");
        let _ = writeln!(cmds, "RUST_LOG=ty_find=trace tyf {command}");

        self.write_raw(&cmds);
    }

    /// Flush the log writer.
    pub fn flush(&self) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_log_creates_file_and_logs_sections() {
        let log = DebugLog::create().expect("should create debug log");
        assert!(log.path().exists(), "log file should exist");

        log.log_cli_args(&["tyf".to_string(), "find".to_string(), "calculate_sum".to_string()]);

        log.log_workspace_resolution(
            Path::new("/home/user/monorepo"),
            Path::new("/home/user/monorepo/services/api"),
            None,
            "found pyproject.toml at /home/user/monorepo/services/api/pyproject.toml",
        );

        log.log_daemon_connection("/tmp/ty-find-1000.sock", true, None);
        log.log_daemon_version("0.3.0", "0.3.0");

        log.log_rpc_request(
            "workspace_symbols",
            r#"{"query": "calculate_sum", "workspace": "/home/user/monorepo/services/api"}"#,
        );

        log.log_rpc_response(42, true, r#"{"symbols": []}"#);

        log.log_result_summary("0 definitions found");

        log.log_reproduction_commands(
            Path::new("/home/user/monorepo/services/api"),
            &["calculate_sum".to_string()],
            "find calculate_sum",
        );

        log.flush();

        let content = std::fs::read_to_string(log.path()).expect("should read log");

        // Check for key sections (not line-by-line, just markers)
        assert!(content.contains("CLI args:"), "should contain CLI args section");
        assert!(
            content.contains("Workspace resolution:"),
            "should contain workspace resolution section"
        );
        assert!(content.contains("Daemon connection:"), "should contain daemon connection section");
        assert!(content.contains("RPC request sent:"), "should contain RPC request section");
        assert!(content.contains("RPC response received"), "should contain RPC response section");
        assert!(content.contains("Result:"), "should contain result summary");
        assert!(content.contains("Reproduction commands"), "should contain reproduction commands");
        assert!(content.contains("tyf daemon status"), "should contain daemon status command");
        assert!(content.contains("RUST_LOG=ty_find=trace"), "should contain RUST_LOG hint");

        // Cleanup
        let _ = std::fs::remove_file(log.path());
    }

    #[test]
    fn debug_log_contains_workspace_override() {
        let log = DebugLog::create().expect("should create debug log");

        log.log_workspace_resolution(
            Path::new("/home/user/monorepo"),
            Path::new("/custom/path"),
            Some(Path::new("/custom/path")),
            "explicit --workspace flag",
        );

        log.flush();

        let content = std::fs::read_to_string(log.path()).expect("should read log");
        assert!(content.contains("overridden by --workspace"), "should note workspace override");

        let _ = std::fs::remove_file(log.path());
    }
}
