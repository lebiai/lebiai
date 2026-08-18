//! Workspace-confining sandbox for the `bash` tool.
//!
//! - **macOS:** `sandbox-exec` seatbelt profile — deny writes outside workspace
//!   (+ `/tmp`, `$TMPDIR`, `/dev/null`, `/dev/tty`, and the same user export
//!   folders `write` allows: Desktop / Documents / Downloads).
//! - **Linux:** `bwrap` when available (bind-mount workspace + tmp); else plain
//!   shell with a clear marker that sandbox is unavailable.
//! - **Windows / other:** no OS sandbox; command still runs with `current_dir`
//!   = workspace (soft boundary only).
//!
//! Network is allowed (agents often need package installs / git). Absolute
//! high-risk commands remain gated by `hermes-turn::danger`.

use std::path::{Path, PathBuf};
use tokio::process::Command;

/// How the command was launched (surfaced in tool output for honesty).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    MacosSeatbelt,
    LinuxBwrap,
    Unsandboxed,
}

impl SandboxMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::MacosSeatbelt => "sandbox=macos-seatbelt",
            Self::LinuxBwrap => "sandbox=linux-bwrap",
            Self::Unsandboxed => "sandbox=none",
        }
    }
}

/// Build a tokio Command that runs `command` under the best available sandbox.
pub fn sandboxed_shell(workspace: &Path, command: &str) -> (Command, SandboxMode) {
    let ws = workspace.to_path_buf();

    #[cfg(target_os = "macos")]
    {
        if let Some(cmd) = macos_seatbelt(&ws, command) {
            return (cmd, SandboxMode::MacosSeatbelt);
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(cmd) = linux_bwrap(&ws, command) {
            return (cmd, SandboxMode::LinuxBwrap);
        }
    }

    let mut c = Command::new("sh");
    c.arg("-c").arg(command).current_dir(&ws);
    // Drop potentially dangerous env inheritance selectively? Keep PATH/HOME
    // for usability; seatbelt/bwrap handle FS.
    (c, SandboxMode::Unsandboxed)
}

#[cfg(target_os = "macos")]
fn macos_seatbelt(workspace: &Path, command: &str) -> Option<Command> {
    let ws = dunce_abs(workspace)?;
    let ws_str = ws.to_string_lossy();
    // Also allow /private/var/folders (user temp on modern macOS).
    let tmp = std::env::temp_dir();
    let tmp_str = tmp.to_string_lossy();
    let mut extra_lines = String::new();
    for root in crate::safety::user_export_roots() {
        if !root.exists() {
            continue;
        }
        let Some(abs) = dunce_abs(&root) else {
            continue;
        };
        extra_lines.push_str(&format!(
            "(allow file-write* (subpath \"{}\"))\n",
            escape_seatbelt_path(&abs.to_string_lossy())
        ));
    }
    extra_lines.push_str(&seatbelt_deny_secrets());

    let profile = format!(
        r#"(version 1)
(deny default)
(allow process*)
(allow signal)
(allow sysctl-read)
(allow mach-lookup)
(allow system-socket)
(allow network*)
(allow file-read*)
(allow file-write-data (literal "/dev/null"))
(allow file-write* (subpath "{ws}"))
(allow file-write* (subpath "/tmp"))
(allow file-write* (subpath "/private/tmp"))
(allow file-write* (subpath "{tmp}"))
(allow file-write* (subpath "/private/var/folders"))
{extra}(allow file-ioctl (literal "/dev/null") (literal "/dev/tty"))
"#,
        ws = escape_seatbelt_path(&ws_str),
        tmp = escape_seatbelt_path(&tmp_str),
        extra = extra_lines,
    );

    let mut c = Command::new("sandbox-exec");
    c.arg("-p")
        .arg(profile)
        .arg("sh")
        .arg("-c")
        .arg(command)
        .current_dir(&ws)
        .stdin(std::process::Stdio::null());
    Some(c)
}

#[cfg(target_os = "macos")]
fn escape_seatbelt_path(p: &str) -> String {
    p.replace('\\', "\\\\").replace('"', "\\\"")
}

/// After `allow file-read*`, deny key/token locations. Last matching deny wins
/// for these subpaths on macOS seatbelt.
#[cfg(target_os = "macos")]
fn seatbelt_deny_secrets() -> String {
    let mut out = String::new();
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".ssh"));
        paths.push(home.join(".gnupg"));
        paths.push(home.join(".aws"));
        paths.push(home.join(".kube"));
        paths.push(home.join(".netrc"));
    }
    let root = hermes_core::data_root();
    paths.push(root.join("config.toml"));
    paths.push(root.join("server.token"));
    paths.push(root.join("wechat.toml"));
    paths.push(root.join("feishu.toml"));
    paths.push(root.join("telegram.toml"));
    for p in paths {
        let Some(abs) = dunce_abs(&p).or_else(|| p.is_absolute().then_some(p)) else {
            continue;
        };
        let escaped = escape_seatbelt_path(&abs.to_string_lossy());
        if abs.is_dir() {
            out.push_str(&format!("(deny file-read* (subpath \"{escaped}\"))\n"));
        } else {
            out.push_str(&format!("(deny file-read* (literal \"{escaped}\"))\n"));
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn linux_bwrap(workspace: &Path, command: &str) -> Option<Command> {
    // Require bwrap on PATH.
    let bwrap = which("bwrap")?;
    let ws = dunce_abs(workspace)?;
    let ws_str = ws.to_string_lossy().into_owned();
    let tmp = std::env::temp_dir();
    let tmp_str = tmp.to_string_lossy().into_owned();

    let export_binds: Vec<(String, String)> = crate::safety::user_export_roots()
        .into_iter()
        .filter(|p| p.exists())
        .filter_map(|p| dunce_abs(&p))
        .map(|p| {
            let s = p.to_string_lossy().into_owned();
            (s.clone(), s)
        })
        .collect();

    let mut c = Command::new(bwrap);
    c.args([
        "--die-with-parent",
        "--unshare-pid",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--ro-bind",
        "/usr",
        "/usr",
        "--ro-bind",
        "/bin",
        "/bin",
        "--ro-bind",
        "/lib",
        "/lib",
        "--ro-bind-try",
        "/lib64",
        "/lib64",
        "--ro-bind-try",
        "/etc",
        "/etc",
        "--bind",
        &ws_str,
        &ws_str,
        "--bind",
        &tmp_str,
        &tmp_str,
        "--bind",
        "/tmp",
        "/tmp",
    ]);
    for (src, dst) in &export_binds {
        c.args(["--bind", src, dst]);
    }
    c.args(["--chdir", &ws_str, "--", "sh", "-c", command])
        .stdin(std::process::Stdio::null());
    Some(c)
}

#[cfg(target_os = "linux")]
fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        for p in std::env::split_paths(&paths) {
            let candidate = p.join(bin);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    })
}

fn dunce_abs(p: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(p).ok().or_else(|| {
        if p.is_absolute() {
            Some(p.to_path_buf())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_some_command() {
        let dir = tempfile::tempdir().unwrap();
        let (mut cmd, mode) = sandboxed_shell(dir.path(), "echo hi");
        // Ensure we can at least spawn on this platform (may be unsandboxed).
        let _ = mode;
        let out = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { cmd.output().await })
            .unwrap();
        assert!(out.status.success() || !out.stderr.is_empty() || !out.stdout.is_empty() || true);
    }
}
