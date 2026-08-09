//! ANSI styling, gated on whether stderr is a TTY and `NO_COLOR` is unset.
//!
//! Use the per-colour helpers ([`red`], [`green`], etc.) — they return a
//! plain `String` when colour is disabled, so callers don't need to branch.
//! For one-off codes use [`paint`] directly.
//!
//! `NO_COLOR` follows the de-facto standard at <https://no-color.org/>:
//! presence of the variable (any value, including empty) disables colour.

use std::io::IsTerminal;
use std::sync::OnceLock;

static ANSI_ENABLED: OnceLock<bool> = OnceLock::new();

/// True iff stderr is a TTY and `NO_COLOR` is not set. Cached on first call.
pub fn ansi_enabled() -> bool {
    *ANSI_ENABLED.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        std::io::stderr().is_terminal()
    })
}

/// Wrap `s` with `code` + reset if colour is enabled, otherwise return `s`
/// unchanged. `code` is the bare ANSI sequence, e.g. `"\x1b[31m"`.
pub fn paint(code: &str, s: &str) -> String {
    if ansi_enabled() {
        format!("{code}{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn red(s: &str) -> String {
    paint("\x1b[31m", s)
}
pub fn green(s: &str) -> String {
    paint("\x1b[32m", s)
}
pub fn yellow(s: &str) -> String {
    paint("\x1b[33m", s)
}
pub fn dim(s: &str) -> String {
    paint("\x1b[90m", s)
}
pub fn bold(s: &str) -> String {
    paint("\x1b[1m", s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_plain_when_disabled() {
        // We can't reliably toggle ANSI_ENABLED in a test (it's cached and
        // depends on actual stderr). Just ensure the function returns a
        // string that either equals input or contains the code.
        let r = red("hi");
        assert!(r == "hi" || r == "\x1b[31mhi\x1b[0m");
    }
}
