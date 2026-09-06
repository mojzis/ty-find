use std::process;

/// Ensure `ty` is available, either directly on PATH or via `uvx`.
/// Panics with install instructions if neither works.
#[allow(dead_code)] // the daemon and guide suites don't need ty
pub fn require_ty() {
    let direct = process::Command::new("ty")
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if direct {
        return;
    }

    let via_uvx = process::Command::new("uvx")
        .arg("ty")
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    assert!(
        via_uvx,
        "ty is not installed and uvx fallback failed. Install it with: uv add --dev ty"
    );
}

/// The minimum `ty` version that supports `callHierarchy/*`.
///
/// Established empirically in `docs/dev/call-hierarchy-spike.md`: `0.0.40`
/// does not advertise `callHierarchyProvider`, `0.0.41` does. This is above
/// tyf's `0.0.15` floor and above every version in the *blocking* CI matrix,
/// so the `calls` suite has to gate on it rather than assume it.
///
/// Mirrors `capabilities.call_hierarchy` in `ci/ty-versions.json`, the single
/// source of truth for ty version data. Kept as a literal because integration
/// tests do not read files at build time; change both together.
#[allow(dead_code)] // only the `calls` suite uses this
pub const CALL_HIERARCHY_MIN_TY: (u32, u32, u32) = (0, 0, 41);

/// Read the installed `ty` version, preferring `ty` on PATH then `uvx ty` —
/// the same resolution order the product uses.
#[allow(dead_code)] // only the `calls` suite uses this
fn installed_ty_version() -> Option<(u32, u32, u32)> {
    let try_cmd = |mut cmd: process::Command| -> Option<String> {
        let out = cmd.output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
    };

    let mut direct = process::Command::new("ty");
    direct.arg("--version");
    let raw = try_cmd(direct).or_else(|| {
        let mut uvx = process::Command::new("uvx");
        uvx.arg("ty").arg("--version");
        try_cmd(uvx)
    })?;

    parse_ty_version(&raw)
}

/// Parse `"ty 0.0.73\n"` into `(0, 0, 73)`.
fn parse_ty_version(raw: &str) -> Option<(u32, u32, u32)> {
    let trimmed = raw.trim();
    let version = trimmed.strip_prefix("ty ").unwrap_or(trimmed).trim();
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    // A trailing pre-release suffix (`0.0.41rc1`) is not something ty has
    // published, but take the leading digits rather than failing outright.
    let patch_raw = parts.next()?;
    let digits: String = patch_raw.chars().take_while(char::is_ascii_digit).collect();
    let patch = digits.parse().ok()?;
    Some((major, minor, patch))
}

/// Skip the calling test unless the installed `ty` supports call hierarchy.
///
/// Returns `false` (with an explanatory line on stdout) instead of panicking:
/// the blocking CI matrix deliberately runs `ty` versions predating the
/// feature, and a hard failure there would be a false regression signal.
#[must_use]
#[allow(dead_code)] // only the `calls` suite uses this
pub fn has_call_hierarchy() -> bool {
    match installed_ty_version() {
        Some(version) if version >= CALL_HIERARCHY_MIN_TY => true,
        Some(version) => {
            println!(
                "SKIP: installed ty {}.{}.{} predates call hierarchy (needs {}.{}.{}+) \
                 — see docs/dev/call-hierarchy-spike.md",
                version.0,
                version.1,
                version.2,
                CALL_HIERARCHY_MIN_TY.0,
                CALL_HIERARCHY_MIN_TY.1,
                CALL_HIERARCHY_MIN_TY.2,
            );
            false
        }
        None => {
            println!("SKIP: could not determine the installed ty version");
            false
        }
    }
}

/// The Unix socket the daemon listens on: `/tmp/ty-find-{uid}.sock`.
///
/// Mirrors `get_socket_path` in `src/daemon/client.rs`. The daemon has no
/// env override, so this real path is the only one a test can watch.
#[cfg(unix)]
#[allow(dead_code)] // only the daemon-sensitive suites use this
#[allow(unsafe_code)]
pub fn daemon_socket_path() -> std::path::PathBuf {
    // SAFETY: `libc::getuid()` is a simple syscall that returns the real
    // user ID. It has no preconditions and cannot cause UB.
    let uid = unsafe { libc::getuid() };
    std::path::PathBuf::from(format!("/tmp/ty-find-{uid}.sock"))
}

/// The daemon's pidfile: `/tmp/ty-find-{uid}.pid`.
///
/// Mirrors `pidfile_path` in `src/daemon/pidfile.rs`.
#[cfg(unix)]
#[allow(dead_code)] // only the daemon-sensitive suites use this
#[allow(unsafe_code)]
pub fn daemon_pidfile_path() -> std::path::PathBuf {
    // SAFETY: as above.
    let uid = unsafe { libc::getuid() };
    std::path::PathBuf::from(format!("/tmp/ty-find-{uid}.pid"))
}

/// PIDs of every running *daemon* started from the binary at `bin_path`.
///
/// The daemon's command line is `<bin> daemon start --foreground`, so
/// matching on that prefix excludes ordinary `tyf` invocations running at
/// the same moment. Uses `ps` rather than `pgrep -f`: concurrent `pgrep`
/// invocations carry the pattern in their own command lines and match each
/// other.
#[allow(dead_code)] // only the daemon-sensitive suites use this
pub fn daemon_pids(bin_path: &std::path::Path) -> std::collections::HashSet<String> {
    let prefix = format!("{} daemon", bin_path.to_string_lossy());
    let output = process::Command::new("ps").args(["-eo", "pid=,args="]).output();
    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let (pid, args) = line.trim().split_once(' ')?;
                args.trim_start().starts_with(&prefix).then(|| pid.to_string())
            })
            .collect(),
        Err(_) => std::collections::HashSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_ty_version;

    #[test]
    fn parses_ty_version_output() {
        assert_eq!(parse_ty_version("ty 0.0.73\n"), Some((0, 0, 73)));
    }

    #[test]
    fn parses_bare_version() {
        assert_eq!(parse_ty_version("0.0.41"), Some((0, 0, 41)));
    }

    #[test]
    fn rejects_unparseable_output() {
        assert_eq!(parse_ty_version("not a version"), None);
    }
}
