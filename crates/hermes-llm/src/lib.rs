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

/// Map a raw provider/turn error string to plain-user wording.
///
/// `lang` is a BCP-47-ish tag (`zh-CN`, `zh`, `en-US`, `en`). Defaults to
/// Chinese when empty/unknown (product default language).
pub fn humanize_error(raw: &str) -> String {
    humanize_error_lang(raw, "zh-CN")
}

/// Language-aware variant of [`humanize_error`].
pub fn humanize_error_lang(raw: &str, lang: &str) -> String {
    let zh = is_zh(lang);
    let raw = raw.trim();
    let lower = raw.to_ascii_lowercase();
    let http_status = extract_http_status(raw);
    match http_status {
        Some(401) => {
            return if zh {
                "API Key 无效或已过期。请到「设置 → 模型服务」检查并更换 Key。"
            } else {
                "API key is invalid or expired. Open Settings → Model and replace the key."
            }
            .into()
        }
        Some(402) | Some(403) => {
            return if zh {
                "服务商账户余额不足或未授权（可能欠费）。请登录服务商官网充值后重试。"
            } else {
                "Provider account is unauthorized or out of credit. Top up on the provider site and retry."
            }
            .into()
        }
        Some(404) => {
            return if zh {
                "模型或接口地址不存在（可能已改名）。请到「设置 → 模型服务 → 高级设置」检查模型名。"
            } else {
                "Model or endpoint not found. Check the model name under Settings → Model → Advanced."
            }
            .into()
        }
        Some(429) => {
            return if zh {
                "请求太频繁了，请稍等一会儿再试。"
            } else {
                "Too many requests. Wait a moment and try again."
            }
            .into()
        }
        Some(500..=599) => {
            return if zh {
                "服务商暂时不可用，请稍后再试。"
            } else {
                "The model provider is temporarily unavailable. Try again later."
            }
            .into()
        }
        _ => {}
    }
    if lower.contains("timed out") || lower.contains("timeout") || lower.contains("deadline") {
        return if zh {
            "服务响应超时，请重试；如果持续超时，检查网络或换一个服务商。"
        } else {
            "The provider timed out. Retry; if it keeps happening, check your network or switch providers."
        }
        .into();
    }
    if lower.contains("connect") && (lower.contains("failed") || lower.contains("error"))
        || lower.contains("dns")
        || lower.contains("no route")
    {
        return if zh {
            "无法连接到模型服务，请检查网络后重试。"
        } else {
            "Could not reach the model provider. Check your network and retry."
        }
        .into();
    }
    if lower.contains("apikey")
        || lower.contains("api key")
        || lower.contains("unauthorized")
        || lower.contains("invalid_api_key")
        || lower.contains("authentication")
        || lower.contains("no api key")
    {
        return if zh {
            "API Key 无效或已过期。请到「设置 → 模型服务」检查并更换 Key。"
        } else {
            "API key is invalid or expired. Open Settings → Model and replace the key."
        }
        .into();
    }
    // Strip engineer prefixes when we pass the raw string through.
    let cleaned = raw
        .strip_prefix("config: ")
        .or_else(|| raw.strip_prefix("session: "))
        .or_else(|| raw.strip_prefix("Config: "))
        .or_else(|| raw.strip_prefix("Session: "))
        .unwrap_or(raw);
    cleaned.to_string()
}

fn is_zh(lang: &str) -> bool {
    let l = lang.trim().to_ascii_lowercase();
    l.is_empty() || l.starts_with("zh")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_401_zh_en() {
        let zh = humanize_error_lang("HTTP 401: unauthorized", "zh-CN");
        assert!(zh.contains("API Key"));
        let en = humanize_error_lang("HTTP 401: unauthorized", "en-US");
        assert!(en.to_ascii_lowercase().contains("api key"));
        assert!(!en.contains("无效"));
    }
}
