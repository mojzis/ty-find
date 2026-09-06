//! `ty-find`: the executable `uvx ty-find` looks for.
//!
//! maturin's `bin` bindings install every cargo binary and refuse
//! `[project.scripts]`, so the package name has to be a real binary. This
//! one has no logic of its own: it hands off to the `tyf` installed next to
//! it (falling back to `tyf` on PATH), keeping the wheel from carrying the
//! whole CLI twice. On Unix it `exec`s, so `tyf` inherits the process; on
//! other platforms it waits and forwards the exit code.

use std::path::PathBuf;
use std::process::Command;

const TYF: &str = if cfg!(windows) { "tyf.exe" } else { "tyf" };

/// The `tyf` next to this binary if there is one, else the name alone so
/// the OS resolves it on PATH.
fn sibling_tyf() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(TYF)))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(TYF))
}

#[allow(clippy::exit)]
fn main() {
    let tyf = sibling_tyf();
    let args = std::env::args_os().skip(1);
    let mut cmd = Command::new(&tyf);
    cmd.args(args);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        let err = cmd.arg0("tyf").exec();
        eprintln!("ty-find: failed to run {}: {err}", tyf.display());
        std::process::exit(1);
    }

    #[cfg(not(unix))]
    {
        match cmd.status() {
            Ok(status) => std::process::exit(status.code().unwrap_or(1)),
            Err(err) => {
                eprintln!("ty-find: failed to run {}: {err}", tyf.display());
                std::process::exit(1);
            }
        }
    }
}
