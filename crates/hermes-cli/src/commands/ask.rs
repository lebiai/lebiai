//! `hermes ask` — one-shot prompt: send a single user message, print reply.

use anyhow::{Context, Result};
use hermes_core::{CompletionRequest, Message};
use hermes_llm::Config;

use super::util::build_active_provider;

pub async fn run(prompt: String, system: Option<String>) -> Result<()> {
    let cfg = Config::load_default()
        .context("loading config from ~/.small-rust-hermes/config.toml")?;
    let provider_cfg = cfg.active_provider()?.clone();
    let provider = build_active_provider(&cfg)?;

    let req = CompletionRequest {
        model: provider_cfg.model.clone(),
        system,
        messages: vec![Message::user_text(prompt)],
        tools: vec![],
        max_tokens: provider_cfg.max_tokens,
        temperature: None,
        enable_caching: false,
    };

    let resp = provider
        .complete(req)
        .await
        .map_err(|e| anyhow::anyhow!("provider.complete: {e}"))?;

    println!("{}", resp.text());
    tracing::info!(
        input_tokens = resp.usage.input_tokens,
        output_tokens = resp.usage.output_tokens,
        stop_reason = ?resp.stop_reason,
        "completion done"
    );
    Ok(())
}
