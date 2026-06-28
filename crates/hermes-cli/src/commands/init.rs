//! `hermes init` — interactive first-run configuration.
//!
//! Walks the user through picking a provider, base URL, API key (hidden
//! input via `rpassword`), model, and token budget, then writes a complete
//! `~/.small-rust-hermes/config.toml` (mode 600). Everything else in the
//! config keeps its serde defaults, so the file is immediately usable by
//! `chat` / `ask` / `run`.

use std::io::Write;

use anyhow::{Context, Result};
use hermes_llm::{Config, ProviderConfig};

use super::style;

/// One selectable provider preset.
struct Preset {
    key: &'static str,
    label: &'static str,
    base_url: &'static str,
    model: &'static str,
}

const PRESETS: &[Preset] = &[
    Preset {
        key: "anthropic",
        label: "Anthropic (Claude)",
        base_url: "https://api.anthropic.com",
        model: "claude-sonnet-4-20250514",
    },
    Preset {
        key: "openai",
        label: "OpenAI (GPT)",
        base_url: "https://api.openai.com",
        model: "gpt-4o-mini",
    },
];

pub async fn run() -> Result<()> {
    let path = Config::default_path()?;

    eprintln!("{}", style::bold("hermes init — configure your model provider"));
    eprintln!();

    if path.exists() {
        eprint!(
            "{} already exists. Overwrite? [y/N] ",
            style::dim(&path.display().to_string())
        );
        std::io::stderr().flush().ok();
        let mut ans = String::new();
        std::io::stdin().read_line(&mut ans).ok();
        if !matches!(ans.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            eprintln!("(cancelled — existing config untouched)");
            return Ok(());
        }
    }

    // --- provider ---
    eprintln!("Choose a provider:");
    for (i, p) in PRESETS.iter().enumerate() {
        eprintln!("  {}) {}", i + 1, p.label);
    }
    let choice = prompt_line(&format!("Provider [1-{}] (default 1): ", PRESETS.len()))?;
    let idx = choice
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=PRESETS.len()).contains(n))
        .map(|n| n - 1)
        .unwrap_or(0);
    let preset = &PRESETS[idx];

    // --- base url ---
    let base_url = prompt_default(
        &format!("Base URL (default {}): ", preset.base_url),
        preset.base_url,
    )?;

    // --- api key (hidden) ---
    let api_key = rpassword::prompt_password(format!(
        "{} API key (input hidden): ",
        preset.label
    ))
    .context("reading API key")?
    .trim()
    .to_string();
    if api_key.is_empty() {
        eprintln!(
            "{}",
            style::yellow("  ⚠ empty API key — you can fill it in later, but calls will fail until you do.")
        );
    }

    // --- model ---
    let model = prompt_default(
        &format!("Model (default {}): ", preset.model),
        preset.model,
    )?;

    // --- max tokens ---
    let max_tokens_raw = prompt_default("Max output tokens (default 16384): ", "16384")?;
    let max_tokens = max_tokens_raw.trim().parse::<u32>().unwrap_or(16_384);

    // --- assemble config from defaults, then set the active provider ---
    let mut cfg: Config =
        toml::from_str(Config::default_config_toml()).context("seeding config from defaults")?;
    cfg.default_provider = preset.key.to_string();

    let provider = ProviderConfig {
        base_url,
        api_key,
        model,
        max_tokens,
    };
    match preset.key {
        "anthropic" => cfg.providers.anthropic = Some(provider),
        "openai" => cfg.providers.openai = Some(provider),
        _ => unreachable!("preset keys are fixed"),
    }

    cfg.save_to(&path)
        .with_context(|| format!("writing config to {}", path.display()))?;

    eprintln!();
    eprintln!("{} config written to {}", style::ok_mark(), path.display());
    eprintln!("  next: {}", style::bold("hermes chat"));
    eprintln!("  check: {}", style::bold("hermes doctor"));
    Ok(())
}

/// Prompt and return the raw trimmed line.
fn prompt_line(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    std::io::stderr().flush().ok();
    let mut s = String::new();
    std::io::stdin().read_line(&mut s).context("reading input")?;
    Ok(s.trim().to_string())
}

/// Prompt, returning `default` when the user just presses Enter.
fn prompt_default(prompt: &str, default: &str) -> Result<String> {
    let line = prompt_line(prompt)?;
    if line.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(line)
    }
}
