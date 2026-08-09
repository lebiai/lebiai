//! Terminal styling with automatic color detection.
//!
//! All status/diagnostic output in the CLI goes to **stderr** (stdout is
//! reserved for the assistant's actual reply, so it stays clean for pipes).
//! Color is therefore gated on whether *stderr* is a TTY, plus the usual
//! environment overrides:
//!
//! - `NO_COLOR` (any value) — force color **off**. See <https://no-color.org>.
//! - `CLICOLOR_FORCE` (non-empty, not `0`) — force color **on**, even when
//!   stderr is redirected (useful for CI logs that render ANSI).
//! - otherwise: color on iff stderr is a terminal.
//!
//! The result is computed once and cached, so detection cost is paid at most
//! once per process.

use std::io::IsTerminal;
use std::sync::OnceLock;

/// Whether ANSI color codes should be emitted on the status stream (stderr).
pub fn color_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        if std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| !v.is_empty() && v != "0") {
            return true;
        }
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        std::io::stderr().is_terminal()
    })
}

/// Wrap `s` in the SGR sequence `code` (e.g. `"33"`, `"1;33"`), resetting
/// afterwards. Returns `s` unchanged when color is disabled.
pub fn paint(code: &str, s: &str) -> String {
    if color_enabled() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

/// Dim / grey (SGR 90) — secondary, low-emphasis text.
pub fn dim(s: &str) -> String {
    paint("90", s)
}

/// Faint (SGR 2) — hints and parentheticals.
pub fn faint(s: &str) -> String {
    paint("2", s)
}

/// Red (SGR 31) — errors.
pub fn red(s: &str) -> String {
    paint("31", s)
}

/// Green (SGR 32) — success.
pub fn green(s: &str) -> String {
    paint("32", s)
}

/// Yellow (SGR 33) — tool activity / warnings.
pub fn yellow(s: &str) -> String {
    paint("33", s)
}

/// Bold (SGR 1).
pub fn bold(s: &str) -> String {
    paint("1", s)
}

/// Bold yellow (SGR 1;33) — confirmation prompts.
pub fn bold_yellow(s: &str) -> String {
    paint("1;33", s)
}

/// Green `✓` success marker.
pub fn ok_mark() -> String {
    green("✓")
}

/// Red `✗` failure marker.
pub fn err_mark() -> String {
    red("✗")
}
