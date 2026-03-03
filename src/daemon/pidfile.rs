//! Pidfile management for the daemon.
//!
//! The pidfile stores daemon metadata in a simple key-value format:
//!
//! ```text
//! pid=12345
//! socket=/tmp/ty-find-1000.sock
//! tcp_port=52341
//! version=0.2.2
//! ```
//!
//! This allows clients to discover both the Unix socket and TCP fallback port
//! without any configuration.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Daemon metadata stored in the pidfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PidfileData {
    /// Daemon process ID.
    pub pid: u32,

    /// Path to the Unix domain socket.
    pub socket: PathBuf,

    /// TCP port the daemon listens on (127.0.0.1).
    pub tcp_port: u16,

    /// Daemon binary version.
    pub version: String,
}

impl PidfileData {
    /// Write the pidfile atomically (write to temp file, then rename).
    pub fn write(&self, path: &Path) -> Result<()> {
        let content = format!(
            "pid={}\nsocket={}\ntcp_port={}\nversion={}\n",
            self.pid,
            self.socket.display(),
            self.tcp_port,
            self.version,
        );

        // Write to a temporary file first, then rename for atomicity.
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, content).context("Failed to write temporary pidfile")?;
        std::fs::rename(&tmp_path, path).context("Failed to rename pidfile into place")?;

        Ok(())
    }

    /// Read and parse a pidfile.
    pub fn read(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).context("Failed to read pidfile")?;
        Self::parse(&content)
    }

    /// Parse pidfile content from a string.
    fn parse(content: &str) -> Result<Self> {
        let mut pid: Option<u32> = None;
        let mut socket: Option<PathBuf> = None;
        let mut tcp_port: Option<u16> = None;
        let mut version: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "pid" => {
                        pid = Some(value.parse().context("Invalid pid value in pidfile")?);
                    }
                    "socket" => {
                        socket = Some(PathBuf::from(value));
                    }
                    "tcp_port" => {
                        tcp_port =
                            Some(value.parse().context("Invalid tcp_port value in pidfile")?);
                    }
                    "version" => {
                        version = Some(value.to_string());
                    }
                    _ => {
                        // Ignore unknown keys for forward compatibility.
                    }
                }
            }
        }

        Ok(Self {
            pid: pid.context("Missing pid in pidfile")?,
            socket: socket.context("Missing socket in pidfile")?,
            tcp_port: tcp_port.context("Missing tcp_port in pidfile")?,
            version: version.context("Missing version in pidfile")?,
        })
    }
}

/// Get the path to the pidfile for the current user.
///
/// Returns `/tmp/ty-find-{uid}.pid` on Unix systems.
#[allow(unsafe_code)]
#[allow(clippy::unnecessary_wraps)] // Returns Err on non-Unix platforms
pub fn get_pidfile_path() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        // SAFETY: `libc::getuid()` is a simple syscall that returns the real
        // user ID. It has no preconditions and cannot cause UB.
        let uid = unsafe { libc::getuid() };
        Ok(PathBuf::from(format!("/tmp/ty-find-{uid}.pid")))
    }

    #[cfg(not(unix))]
    {
        anyhow::bail!("Pidfile is only supported on Unix systems")
    }
}

/// Remove the pidfile if it exists. Errors are logged but not propagated.
pub fn remove_pidfile(path: &Path) {
    if path.exists() {
        if let Err(e) = std::fs::remove_file(path) {
            tracing::warn!("Failed to remove pidfile {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pidfile_roundtrip() {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let path = dir.path().join("test.pid");

        let data = PidfileData {
            pid: 12345,
            socket: PathBuf::from("/tmp/ty-find-1000.sock"),
            tcp_port: 52341,
            version: "0.2.2".to_string(),
        };

        data.write(&path).expect("write should succeed");
        let read_back = PidfileData::read(&path).expect("read should succeed");

        assert_eq!(data, read_back);
    }

    #[test]
    fn test_pidfile_parse() {
        let content = "pid=42\nsocket=/tmp/foo.sock\ntcp_port=8080\nversion=1.0.0\n";
        let data = PidfileData::parse(content).expect("parse should succeed");

        assert_eq!(data.pid, 42);
        assert_eq!(data.socket, PathBuf::from("/tmp/foo.sock"));
        assert_eq!(data.tcp_port, 8080);
        assert_eq!(data.version, "1.0.0");
    }

    #[test]
    fn test_pidfile_parse_ignores_unknown_keys() {
        let content = "pid=1\nsocket=/s.sock\ntcp_port=99\nversion=0.1.0\nfuture_key=hello\n";
        let data = PidfileData::parse(content).expect("should ignore unknown keys");
        assert_eq!(data.pid, 1);
    }

    #[test]
    fn test_pidfile_parse_missing_field() {
        let content = "pid=1\nsocket=/s.sock\n";
        let err = PidfileData::parse(content).unwrap_err();
        assert!(err.to_string().contains("Missing tcp_port"));
    }

    #[test]
    fn test_pidfile_parse_empty() {
        let err = PidfileData::parse("").unwrap_err();
        assert!(err.to_string().contains("Missing pid"));
    }

    #[test]
    fn test_pidfile_parse_whitespace_lines() {
        let content = "  pid=5  \n\n  socket=/tmp/x.sock  \n  tcp_port=100  \n  version=2.0  \n";
        let data = PidfileData::parse(content).expect("should handle whitespace");
        assert_eq!(data.pid, 5);
        assert_eq!(data.tcp_port, 100);
    }

    #[test]
    fn test_get_pidfile_path() {
        let path = get_pidfile_path().expect("should return a valid path");
        let path_str = path.to_string_lossy();
        assert!(path_str.starts_with("/tmp/ty-find-"));
        assert!(path_str.ends_with(".pid"));
    }

    #[test]
    fn test_remove_pidfile_nonexistent() {
        // Should not panic on nonexistent file.
        remove_pidfile(Path::new("/tmp/nonexistent-ty-find-test.pid"));
    }

    #[test]
    fn test_pidfile_atomic_write() {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let path = dir.path().join("atomic.pid");

        let data = PidfileData {
            pid: 999,
            socket: PathBuf::from("/tmp/test.sock"),
            tcp_port: 12345,
            version: "0.1.0".to_string(),
        };

        data.write(&path).expect("write should succeed");

        // Temp file should not remain.
        let tmp_path = path.with_extension("tmp");
        assert!(!tmp_path.exists(), "temp file should be cleaned up after rename");
        assert!(path.exists(), "pidfile should exist");
    }
}
