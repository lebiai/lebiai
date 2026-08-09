//! `hermes doctor` — environment & configuration health check.
//!
//! Prints a checklist so a user (or a bug report) can see at a glance what's
//! configured and what's missing, without leaking secrets. Never mutates
//! anything; safe to run anytime.

use anyhow::Result;
use hermes_llm::Config;

use super::style;

pub async fn run() -> Result<()> {
    eprintln!("{}", style::bold("hermes doctor"));
    eprintln!();

    let mut problems = 0usize;

    // --- config file ---
    let cfg_path = Config::default_path()?;
    if cfg_path.exists() {
        ok(&format!("config file: {}", cfg_path.display()));
    } else {
        problems += 1;
        bad(&format!("config file missing: {}", cfg_path.display()));
        hint("run `hermes init` to create it");
    }

    // --- parse + provider ---
    match Config::load_default() {
        Ok(cfg) => {
            ok("config parses");
            match cfg.active_provider() {
                Ok(p) => {
                    ok(&format!(
                        "active provider: {} ({}, model {})",
                        cfg.default_provider, p.base_url, p.model
                    ));
                    if p.api_key.trim().is_empty() {
                        problems += 1;
                        bad("API key is empty");
                        hint("run `hermes init`, or edit the [providers] table");
                    } else {
                        ok(&format!("API key present ({})", masked(&p.api_key)));
                    }
                    if p.max_tokens == 0 {
                        problems += 1;
                        bad("max_tokens is 0");
                    }
                }
                Err(e) => {
                    problems += 1;
                    bad(&format!("provider misconfigured: {e}"));
                    hint("check `default_provider` matches a populated [providers.*] table");
                }
            }

            // --- workspace ---
            let ws = &cfg.workspace.root;
            if ws.as_os_str().is_empty() {
                warn("workspace root is empty (will default to current dir)");
            } else if ws.exists() {
                ok(&format!("workspace: {}", ws.display()));
            } else {
                warn(&format!(
                    "workspace does not exist yet: {} (created on first use)",
                    ws.display()
                ));
            }
        }
        Err(e) => {
            problems += 1;
            bad(&format!("config failed to load: {e}"));
        }
    }

    // --- MCP servers (advisory; absence is fine) ---
    match hermes_mcp::McpConfig::load_default() {
        Ok(mcp) => {
            let n = mcp.servers.len();
            if n == 0 {
                ok("MCP servers: none configured (built-in tools still available)");
            } else {
                ok(&format!("MCP servers configured: {n}"));
            }
        }
        Err(e) => warn(&format!(
            "mcp.json not loaded: {e} (built-in tools still available)"
        )),
    }

    eprintln!();
    if problems == 0 {
        eprintln!(
            "{}",
            style::green("All good — you're ready to `hermes chat`.")
        );
    } else {
        eprintln!(
            "{}",
            style::yellow(&format!(
                "{problems} issue(s) found — see the ✗ lines above."
            ))
        );
    }
    Ok(())
}

/// Mask a secret, showing only a short prefix/suffix.
fn masked(key: &str) -> String {
    let n = key.chars().count();
    if n <= 8 {
        "********".to_string()
    } else {
        let head: String = key.chars().take(4).collect();
        let tail: String = key
            .chars()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{head}…{tail}, {n} chars")
    }
}

fn ok(msg: &str) {
    eprintln!("  {} {msg}", style::ok_mark());
}
fn bad(msg: &str) {
    eprintln!("  {} {msg}", style::err_mark());
}
fn warn(msg: &str) {
    eprintln!("  {} {msg}", style::yellow("•"));
}
fn hint(msg: &str) {
    eprintln!("      {}", style::dim(&format!("↳ {msg}")));
}
