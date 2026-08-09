//! hermes-llm: LLM provider implementations.
//!
//! - [`AnthropicProvider`] — Anthropic Messages API (also DeepSeek's
//!   anthropic-compat endpoint).
//! - [`OpenAiProvider`] — OpenAI Chat Completions API and any compatible
//!   endpoint (DeepSeek `/v1`, Qwen DashScope, OpenRouter, vLLM, ...).

pub mod anthropic;
pub mod config;
pub mod openai;
pub(crate) mod retry;

pub use anthropic::AnthropicProvider;
pub use config::{
    Config, ContextLimits, PermissionsConfig, ProviderConfig, ProviderKind, ProviderPreset,
    PROVIDER_PRESETS,
};
pub use openai::OpenAiProvider;

/// Map a raw provider/turn error string to plain-user wording. The engine
/// errors arrive as `HTTP <status>: <body>` or reqwest connection messages;
/// users don't care about status codes — they want to know what to do.
pub fn humanize_error(raw: &str) -> String {
    let raw = raw.trim();
    let lower = raw.to_ascii_lowercase();
    let http_status = extract_http_status(raw);
    match http_status {
        Some(401) => {
            return "API Key 无效或已过期。请到「设置 → 模型服务」检查并更换 Key。".into()
        }
        Some(402) | Some(403) => {
            return "服务商账户余额不足或未授权（可能欠费）。请登录服务商官网充值后重试。"
                .into()
        }
        Some(404) => {
            return "模型或接口地址不存在（可能已改名）。请到「设置 → 模型服务 → 高级设置」检查模型名。"
                .into()
        }
        Some(429) => return "请求太频繁了，请稍等一会儿再试。".into(),
        Some(500..=599) => return "服务商暂时不可用，请稍后再试。".into(),
        _ => {}
    }
    if lower.contains("timed out") || lower.contains("timeout") || lower.contains("deadline") {
        return "服务响应超时，请重试；如果持续超时，检查网络或换一个服务商。".into();
    }
    if lower.contains("connect") && (lower.contains("failed") || lower.contains("error"))
        || lower.contains("dns")
        || lower.contains("no route")
    {
        return "无法连接到模型服务，请检查网络后重试。".into();
    }
    if lower.contains("apikey") || lower.contains("api key") || lower.contains("unauthorized") {
        return "API Key 无效或已过期。请到「设置 → 模型服务」检查并更换 Key。".into();
    }
    if lower.contains("invalid_api_key") || lower.contains("authentication") {
        return "API Key 无效或已过期。请到「设置 → 模型服务」检查并更换 Key。".into();
    }
    raw.to_string()
}

fn extract_http_status(raw: &str) -> Option<u16> {
    // Engine format: "HTTP 401: ..." / "http error 429 ..." / "status 503"
    let re = [
        "http ",
        "http error ",
        "http error: ",
        "status ",
        "status: ",
    ];
    let lower = raw.to_ascii_lowercase();
    for prefix in re {
        if let Some(idx) = lower.find(prefix) {
            let rest = &lower[idx + prefix.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(code) = digits.parse::<u16>() {
                if (100..=599).contains(&code) {
                    return Some(code);
                }
            }
        }
    }
    None
}
