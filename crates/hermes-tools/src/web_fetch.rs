//! `web_fetch` — fetch a URL, clean it to markdown, and (optionally) extract a
//! prompt-focused answer with a cheap LLM.
//!
//! Pipeline:
//! 1. Fetch the page (cached cleaned markdown reused within the TTL).
//! 2. Lift any `<script type="application/json">` payload (Next.js
//!    `__NEXT_DATA__` & co.) out of the page as fallback body text — for those
//!    sites the article lives inside the JSON, not in the DOM.
//! 3. Strip boilerplate blocks (`<script>`/`<style>`/`<head>`/`<nav>`/… and
//!    HTML comments) so JS/CSS noise never reaches the model.
//! 4. Absolutize the surviving `href`/`src` values against the page URL, so the
//!    markdown carries links the model can cite instead of site-relative paths.
//! 5. Convert the remaining HTML to markdown (structure preserved).
//! 6. If a `prompt` is supplied and an extraction provider is wired, ask a
//!    cheap model to answer the prompt against the page and return only that
//!    answer. A long page is handed over as its most date-dense window rather
//!    than whole; an unusable answer (empty because reasoning ate the output
//!    budget, or a bare "no items" over a page that is full of that date) is
//!    retried once with a doubled budget *and a different window*; still
//!    unusable → say "got nothing", never dump the page. Otherwise return the
//!    cleaned markdown, truncated to `max_chars`.
//!
//! Step 6 mirrors Claude Code's WebFetch: the tool does the extraction so the
//! main loop never has to read raw HTML.

use std::collections::HashSet;
use std::path::Path;

use hermes_core::{CompletionRequest, Message, Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

use crate::http_defaults::FETCH_CLIENT;
use crate::web::{ttl_or_default, WebToolsContext, EXTRACT_RETRY_FACTOR, EXTRACT_RETRY_MAX_TOKENS};
use crate::web_cache;

/// Upper bound on cleaned-markdown chars fed to the extraction model, to keep
/// the extraction request's token count bounded regardless of `max_chars`.
const EXTRACT_INPUT_CAP: usize = 48_000;

/// Target size (Chinese characters) of the window handed to the extraction
/// model when the page is longer than this. 2026-09-21 实测：第一财经首页
/// （一万汉字以上、当天日期出现 653 次）整页送进去时，抽取第一次 `stop=MaxTokens`
/// 回空串、重试直接瞎答「本页无当天条目」；同一个模型读一小段是稳的。
/// 所以长页不再整页送，只送「当天日期最密」的那一段。
const EXTRACT_WINDOW_HAN: usize = 6_000;

/// A short answer that claims there is nothing while the page it was given is
/// full of the very date we asked about is a **miss**, not an answer. Below this
/// length we treat it as one and try another window.
const EXTRACT_MISS_ANSWER_CHARS: usize = 40;

/// How many occurrences of the focus token make "no items" implausible.
const EXTRACT_MISS_FOCUS_HITS: usize = 5;

/// 抽取用的系统提示词。
///
/// 末两行是 2026-09-21 加的：中国证券报首页当天 23 条稿子，标题旁只有钟点
/// （`*18:15*`），日期**只**写在链接地址里（`.../2026/09/21/detail_....html`）。
/// 不写明这一点，模型会照字面判「没有日期」，回一句「本页无当天条目」交卷 ——
/// 页面内容明明够，是整个站被一句话判死。
const EXTRACT_SYSTEM: &str =
    "You extract information from a web page to answer a specific question. \
Use ONLY the page content provided — do not add outside knowledge. \
Answer directly and concisely, quoting the relevant facts, figures, or quotes. \
If the page does not contain the answer, say so explicitly rather than guessing. \
Markdown links keep their targets: a date is often written ONLY in the link URL \
(`.../2026/09/21/...`, `.../20260921...`) while the visible text next to it shows just \
a clock time (`18:15`). Read the link URLs for dates before concluding a page has none.";

#[derive(Deserialize)]
struct Args {
    url: String,
    #[serde(default = "default_max_chars")]
    max_chars: usize,
    /// Optional question. When provided (and an extraction model is wired), the
    /// tool returns a focused answer instead of the raw page markdown.
    #[serde(default)]
    prompt: Option<String>,
}

pub const fn default_max_chars() -> usize {
    20_000
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "web_fetch".into(),
        description: "Fetch a web page and return its main content as clean markdown. \
            Pass a `prompt` to get a focused answer extracted from the page instead of the \
            full text — strongly preferred when you are looking for something specific, as \
            it is far more token-efficient. \
            Efficient research flow: web_search first, read the snippets, then fetch AT MOST \
            1–2 of the most promising URLs (with a `prompt`) — avoid fetching many pages. \
            Only fetch URLs from web_search results or that the user provided; do not guess URLs."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "URL to fetch (must come from search results or user input)"},
                "max_chars": {"type": "integer", "description": "Max characters of markdown to return when no prompt is given (default 20000)"},
                "prompt": {"type": "string", "description": "Optional question to answer from the page. When set, returns a concise extracted answer instead of the full page."}
            },
            "required": ["url"]
        }),
        requires_confirmation: false,
    }
}

pub async fn run(
    _workspace: &Path,
    args: serde_json::Value,
    ctx: Option<&WebToolsContext>,
) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("web_fetch: bad args: {e}")))?;

    if let Err(reason) = crate::url_safety::validate_public_http_url(&a.url) {
        return Ok(ToolCallOutcome {
            content: format!("web_fetch blocked: {reason}"),
            is_error: true,
        });
    }

    let ttl = ttl_or_default(ctx);
    let cache_key = web_cache::fetch_key(&a.url);

    // Reuse cleaned markdown from a prior fetch of the same URL when fresh.
    let (markdown, from_cache) = match web_cache::get(&cache_key, ttl) {
        Some(md) => (md, true),
        None => {
            let resp = FETCH_CLIENT
                .get(&a.url)
                .send()
                .await
                .map_err(|e| hermes_core::Error::ToolHost(format!("web_fetch: {e}")))?;

            let status = resp.status();
            if !status.is_success() {
                let hint = match status.as_u16() {
                    403 => "This site blocks automated access. Try a different source from your search results, or use the web_search snippet directly.",
                    404 => "Page not found. The URL may be wrong — use web_search to find the correct page instead of guessing URLs.",
                    429 => "Rate limited. Wait before retrying, or use the web_search snippet instead.",
                    _ => "Try a different URL from your search results.",
                };
                return Ok(ToolCallOutcome {
                    content: format!("HTTP {status} for {}\n{hint}", a.url),
                    is_error: true,
                });
            }

            let header_charset = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .and_then(charset_from_content_type);
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| hermes_core::Error::ToolHost(format!("web_fetch body: {e}")))?;
            let body = decode_html_bytes(&bytes, header_charset.as_deref());

            let md = clean_html_to_markdown(&body, reqwest::Url::parse(&a.url).ok().as_ref());
            web_cache::put(cache_key, md.clone());
            (md, false)
        }
    };

    if markdown.chars().count() < 80 {
        return Ok(ToolCallOutcome {
            content: format!(
                "(Page returned very little text — likely a JS-rendered site. \
                 Use the web_search snippet instead of fetching this URL.)\n{markdown}"
            ),
            is_error: false,
        });
    }

    // Prompt-focused extraction path (Claude Code WebFetch style).
    if let (Some(prompt), Some(ctx)) = (a.prompt.as_deref().filter(|p| !p.trim().is_empty()), ctx) {
        match extract_answer(ctx, &a.url, &markdown, prompt).await {
            Ok(Extraction::Answer(answer)) => {
                let tag = if from_cache { " (page cached)" } else { "" };
                return Ok(ToolCallOutcome {
                    content: format!("{answer}\n\n[extracted from {}{tag}]", a.url),
                    is_error: false,
                });
            }
            Ok(Extraction::Empty { budget }) => {
                // 抽取问过两次、都回空串：**不许**退回整页原文。2026-09-20 实测，
                // 退回原文等于一次往上下文里塞两万字，海燕那一轮正是这样把 16384
                // 的输出预算撞爆、整轮零产出的。所以这里给一句「拿不到」，让采的人
                // 记一笔往下走。
                let note = if budget > ctx.extract_max_tokens {
                    format!(
                        "抽取预算已从 {} 加到 {} tokens 重问一次，仍空",
                        ctx.extract_max_tokens, budget
                    )
                } else {
                    format!("抽取预算 {} tokens 已顶上限，未再重问", budget)
                };
                return Ok(ToolCallOutcome {
                    content: format!(
                        "{}：抽取没给出正文（该页正文 {} 字，{note}）。\
                         这一站本轮按「拿不到」记一笔，不要再抓同一个 URL，\
                         也不要摘掉 prompt 去抓整页。",
                        a.url,
                        markdown.chars().count(),
                    ),
                    is_error: false,
                });
            }
            Err(e) => {
                tracing::warn!(error=%e, url=%a.url, "web_fetch extraction failed");
                return Ok(ToolCallOutcome {
                    content: format!(
                        "web_fetch 抽取请求失败（{}）：{}。这一站本轮按「拿不到」记一笔。",
                        a.url, e
                    ),
                    is_error: true,
                });
            }
        }
    }

    // Default path: return cleaned markdown, truncated to max_chars.
    let prefix = if from_cache { "(cached)\n" } else { "" };
    let content = if markdown.chars().count() > a.max_chars {
        let t: String = markdown.chars().take(a.max_chars).collect();
        format!("{prefix}{t}\n... (truncated at {} chars)", a.max_chars)
    } else {
        format!("{prefix}{markdown}")
    };

    Ok(ToolCallOutcome {
        content,
        is_error: false,
    })
}

/// 抽取调用的结局。
enum Extraction {
    /// 拿到了正文。
    Answer(String),
    /// 两次都没拿到正文（预算已顶上限时只问了一次）。`budget` 是最后一次用的预算。
    Empty { budget: u32 },
}

/// 问抽取模型一个问题；回空串就加预算**再问一次**。
///
/// 推理模型会把看不见的 `reasoning_content` 先算进 `max_tokens`：预算被推理吃光
/// 时正文回空串、`stop_reason=MaxTokens`（见
/// `crates/hermes-llm/src/openai.rs::a_reasoning_only_response_is_not_sendable_content`）。
/// 2026-09-21 一轮实跑，上证报 / 证券时报 / e公司 / 券商中国 / 第一财经 / 财新
/// 六个站当轮全部栽在这一条上 —— 页面当天明明有稿。跟主循环同一条规矩（
/// `docs/records/20260920-reasoning-visible-and-budget.md`）：这一轮本来就要作废，
/// 加预算重来一次只有赚。空串之外的情形（请求报错）不重试，那不是预算问题。
async fn extract_answer(
    ctx: &WebToolsContext,
    url: &str,
    page: &str,
    prompt: &str,
) -> Result<Extraction> {
    // 窗口可能跨日（默认窗：今天 00:00 → 我按下采集那一刻；说「近 3 天」时跨 3 天）：
    // 焦点是**一组**日期，不是一个。提示词里没写日期就退回整页。
    let focuses = focus_tokens(prompt);
    // 长页只送「窗口日期最密」的一段；短页照旧整页送。
    let windows: Vec<String> = if !focuses.is_empty() && han_count(page) > EXTRACT_WINDOW_HAN {
        ranked_windows(page, &focuses, EXTRACT_WINDOW_HAN)
    } else {
        vec![page.chars().take(EXTRACT_INPUT_CAP).collect()]
    };
    let Some(first_window) = windows.first() else {
        return Ok(Extraction::Empty {
            budget: ctx.extract_max_tokens,
        });
    };

    let first = ctx.extract_max_tokens;
    let resp = ask_extract(ctx, url, first_window, prompt, first).await?;
    let answer = resp.text();
    if usable_answer(&answer, first_window, &focuses) {
        return Ok(Extraction::Answer(answer));
    }
    let Some(retry_budget) = double_budget(first) else {
        return Ok(Extraction::Empty { budget: first });
    };
    // 重试**换一段**（有第二段就用第二段）：同一段重复问没有信息增益。
    let second_window = windows.get(1).unwrap_or(first_window);
    tracing::info!(
        url,
        from = first,
        to = retry_budget,
        stop = ?resp.stop_reason,
        answer_chars = answer.chars().count(),
        windows = windows.len(),
        switched_window = windows.len() > 1,
        "web_fetch extraction came back unusable — retrying once with a bigger budget"
    );
    let resp = ask_extract(ctx, url, second_window, prompt, retry_budget).await?;
    let answer = resp.text();
    if usable_answer(&answer, second_window, &focuses) {
        return Ok(Extraction::Answer(answer));
    }
    Ok(Extraction::Empty {
        budget: retry_budget,
    })
}

/// 这一条答案能不能用：空串不行；「页面上窗口里的日期到处都是、答案却一口咬定没有」也不行。
fn usable_answer(answer: &str, page: &str, focuses: &[String]) -> bool {
    if answer.trim().is_empty() {
        return false;
    }
    !answer_looks_like_a_miss(answer, page, focuses)
}

/// 短答案 + 页面上**窗口里任一天**到处都是 → 判为漏答（模型看漏了，不是页面真没有）。
fn answer_looks_like_a_miss(answer: &str, page: &str, focuses: &[String]) -> bool {
    answer.chars().count() < EXTRACT_MISS_ANSWER_CHARS
        && focus_set_count(page, focuses) >= EXTRACT_MISS_FOCUS_HITS
}

/// 同一个日期在页面里有好几种写法：`2026-09-21` / `2026/09/21` / `2026.09.21` /
/// `2026年9月21日` / `20260921`。识日期时这几种都要算同一天。
///
/// 2026-09-21 实测：中国证券报首页当天有 23 条稿子，日期**只**出现在链接路径
/// `.../2026/09/21/detail_....html` 里。只按提示词里的连字符写法数 → 一处都数不到，
/// 于是切窗切到页头导航、抽取回空，整站交 0 条，回执还写着「本页无当天条目」。
fn date_variants(focus: &str) -> Vec<String> {
    let digits: String = focus.chars().filter(char::is_ascii_digit).collect();
    if digits.len() != 8 {
        return vec![focus.to_string()];
    }
    let (y, m, d) = (&digits[0..4], &digits[4..6], &digits[6..8]);
    let trim = |s: &str| {
        let t = s.trim_start_matches('0');
        if t.is_empty() {
            "0".to_string()
        } else {
            t.to_string()
        }
    };
    let (m2, d2) = (trim(m), trim(d));
    vec![
        format!("{y}-{m}-{d}"),
        format!("{y}/{m}/{d}"),
        format!("{y}.{m}.{d}"),
        format!("{y}年{m}月{d}日"),
        format!("{y}-{m2}-{d2}"),
        format!("{y}/{m2}/{d2}"),
        format!("{y}年{m2}月{d2}日"),
        digits,
    ]
}

/// `focus` 这一天在页面里出现几次（各写法相加；写法之间不重叠）。
fn focus_count(page: &str, focus: &str) -> usize {
    if focus.is_empty() {
        return 0;
    }
    date_variants(focus)
        .iter()
        .map(|variant| page.match_indices(variant.as_str()).count())
        .sum()
}

/// 窗口里**每一天**在页面里出现几次的总和（一天都没写就是 0）。
fn focus_set_count(page: &str, focuses: &[String]) -> usize {
    focuses.iter().map(|f| focus_count(page, f)).sum()
}

/// 从提示词里取出**窗口内**可数的日期标记（`2026-09-21` / `2026/09/21` / `2026.09.21`），
/// 用来在长页里挑「窗口日期最密」的那一段、也用来判漏答。
///
/// 窗口可能跨日（默认窗：今天 00:00 → 我按下采集那一刻；说「近 3 天」时跨 3 天），
/// 所以是**一组**日期，不是一个 —— 只认第一个会把后一天那批稿子判成「不在窗口内」。
/// 提示词里一个日期都没写（用户说「近 3 天」）就返回空（退回整页）。
/// 最多认 4 个：提示词里偶有举例日期，别让它喧宾夺主。
fn focus_tokens(prompt: &str) -> Vec<String> {
    let chars: Vec<char> = prompt.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            let mut j = i;
            while j < chars.len()
                && (chars[j].is_ascii_digit() || matches!(chars[j], '-' | '/' | '.') || {
                    chars[j] == '年' || chars[j] == '月' || chars[j] == '日'
                })
            {
                j += 1;
            }
            let token: String = chars[start..j].iter().collect();
            let digits = token.chars().filter(|c| c.is_ascii_digit()).count();
            if digits == 8 && token.chars().any(|c| c == '-' || c == '/') && !out.contains(&token) {
                out.push(token);
                if out.len() == 4 {
                    return out;
                }
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    out
}

/// 把长页切成互不重叠的窗口（每段约 `window_han` 个汉字），按段内**窗口日期**
/// 出现次数从多到少排序。第一段就是「窗口日期最密」的那一段。
fn ranked_windows(page: &str, focuses: &[String], window_han: usize) -> Vec<String> {
    let mut windows: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut han = 0usize;
    for line in page.split_inclusive('\n') {
        let line_han = han_count(line);
        if han > 0 && han + line_han > window_han {
            windows.push(std::mem::take(&mut current));
            han = 0;
        }
        current.push_str(line);
        han += line_han;
    }
    if !current.is_empty() {
        windows.push(current);
    }
    windows.sort_by_key(|w| std::cmp::Reverse(focus_set_count(w, focuses)));
    windows
}

/// 把一次抽取请求送出去。`max_tokens` 是这里唯一的旋钮 —— 重试就靠它。
async fn ask_extract(
    ctx: &WebToolsContext,
    url: &str,
    page: &str,
    prompt: &str,
    max_tokens: u32,
) -> Result<hermes_core::CompletionResponse> {
    let user_msg = format!(
        "Web page content (markdown) from {url}:\n\n{page}\n\n---\nQuestion: {prompt}\n\nAnswer using only the content above."
    );
    let req = CompletionRequest {
        model: ctx.extract_model.clone(),
        system: Some(EXTRACT_SYSTEM.to_string()),
        messages: vec![Message::user_text(user_msg)],
        tools: Vec::new(),
        max_tokens,
        temperature: Some(0.1),
        enable_caching: false,
    };
    ctx.extract_provider.complete(req).await
}

/// 抽取回空串后要给的下一个预算：× [`EXTRACT_RETRY_FACTOR`]，封顶
/// [`EXTRACT_RETRY_MAX_TOKENS`]。已经翻不动了（配置本身就顶到上限）就回 `None`，
/// 调用方据此只问一次、不空转。
fn double_budget(budget: u32) -> Option<u32> {
    let next = budget
        .saturating_mul(EXTRACT_RETRY_FACTOR)
        .min(EXTRACT_RETRY_MAX_TOKENS);
    if next > budget {
        Some(next)
    } else {
        None
    }
}

/// Minimum total Chinese characters an embedded JSON payload must carry before
/// it is worth appending — below this the page is an ordinary one whose real
/// body already came through the DOM, and we must not fatten it.
const EMBEDDED_JSON_MIN_HAN: usize = 300;

/// Upper bound on how much lifted JSON text is appended to the markdown.
const EMBEDDED_JSON_MAX_CHARS: usize = 12_000;

/// Charset named by a `Content-Type` header (`text/html; charset=gbk`), lowercased.
fn charset_from_content_type(value: &str) -> Option<String> {
    let lower = ascii_lower(value);
    let at = lower.find("charset=")? + "charset=".len();
    let rest = &value[at..];
    let label: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .collect();
    let label = label.trim().to_ascii_lowercase();
    if label.is_empty() {
        None
    } else {
        Some(label)
    }
}

/// Decode an HTML body into text: the header charset wins, then a `<meta charset>`
/// sniffed from the head, then UTF-8 (lossy).
///
/// 2026-09-21：GBK 站点（同花顺 `news.10jqka.com.cn` 走 header，汽车之家那类只写
/// `<meta charset=gb2312>`）以前一律按 UTF-8 解 → 满页乱码 → 抽取必空。这一处按
/// 页面自己声明的编码解，是所有站点共用的能力，不是某个站的特例。
fn decode_html_bytes(bytes: &[u8], header_charset: Option<&str>) -> String {
    let sniffed = if header_charset.is_none() {
        sniff_meta_charset(&String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]))
    } else {
        None
    };
    let label = header_charset
        .map(str::to_string)
        .or(sniffed)
        .unwrap_or_else(|| "utf-8".to_string());
    let encoding = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = encoding.decode(bytes);
    text.into_owned()
}

/// `<meta charset="gb2312">` / `<meta http-equiv="Content-Type" content="…charset=gbk">`
/// from the head of the document, lowercased.
fn sniff_meta_charset(head: &str) -> Option<String> {
    let lower = ascii_lower(head);
    if let Some(at) = lower.find("charset=") {
        let rest = &head[at + "charset=".len()..];
        let label: String = rest
            .trim_start_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace())
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            .collect();
        let label = label.trim().to_ascii_lowercase();
        if !label.is_empty() {
            return Some(label);
        }
    }
    None
}

/// Boilerplate-strip, absolutize links against `base`, convert to markdown, and
/// append any JSON-embedded body text.
fn clean_html_to_markdown(html: &str, base: Option<&reqwest::Url>) -> String {
    // Lift the JSON body *before* the `<script>` blocks are dropped — on
    // Next.js sites that payload is the only place the article exists.
    let embedded = embedded_json_text(html);

    let mut h = remove_comments(html);
    for tag in [
        "script", "style", "head", "noscript", "svg", "template", "iframe", "form", "nav",
        "header", "footer", "aside",
    ] {
        h = remove_tag_blocks(&h, tag);
    }
    if let Some(base) = base {
        h = absolutize_urls(&h, base);
    }
    let mut md = match htmd::convert(&h) {
        Ok(md) => md,
        Err(_) => strip_tags_fallback(&h),
    };
    if !embedded.is_empty() {
        md.push_str("\n\n<!-- page-embedded-json -->\n");
        md.push_str(&embedded);
    }
    collapse_blank_lines(&md)
}

/// Chinese character count, used to judge whether a string is real prose.
fn han_count(s: &str) -> usize {
    s.chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .count()
}

/// Rewrite relative `href` / `src` values into absolute URLs against `base`.
///
/// `htmd` emits whatever the HTML said, so a site-relative `/article/1.html`
/// reaches the model as-is and gets dropped (it cannot be cited). Absolute URLs
/// and non-navigational schemes pass through untouched; a value that fails to
/// resolve also passes through unchanged rather than panicking.
fn absolutize_urls(html: &str, base: &reqwest::Url) -> String {
    let lower = ascii_lower(html);
    let mut out = String::with_capacity(html.len());
    let mut i = 0usize;
    while i < html.len() {
        let Some((start, name_len)) = next_url_attr(&lower, i) else {
            out.push_str(&html[i..]);
            break;
        };
        out.push_str(&html[i..start + name_len]);
        let bytes = html.as_bytes();
        let mut j = start + name_len;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'=' {
            i = start + name_len;
            continue;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() {
            out.push_str(&html[start + name_len..]);
            break;
        }
        let (val_start, val_end, next) = if bytes[j] == b'"' || bytes[j] == b'\'' {
            let vs = j + 1;
            match html[vs..].find(bytes[j] as char) {
                Some(r) => (vs, vs + r, vs + r + 1),
                None => (vs, html.len(), html.len()),
            }
        } else {
            match html[j..].find(|c: char| c.is_ascii_whitespace() || c == '>') {
                Some(r) => (j, j + r, j + r),
                None => (j, html.len(), html.len()),
            }
        };
        out.push_str(&html[start + name_len..val_start]);
        out.push_str(&rewrite_relative_url(&html[val_start..val_end], base));
        out.push_str(&html[val_end..next]);
        i = next;
    }
    out
}

/// Resolve one attribute value against the page URL, leaving anything that is
/// already absolute (or not a navigation target) untouched.
fn rewrite_relative_url(value: &str, base: &reqwest::Url) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return value.to_string();
    }
    let lower = ascii_lower(trimmed);
    for scheme in [
        "http://",
        "https://",
        "mailto:",
        "tel:",
        "data:",
        "javascript:",
    ] {
        if lower.starts_with(scheme) {
            return value.to_string();
        }
    }
    match base.join(trimmed) {
        Ok(url) => url.to_string(),
        Err(_) => value.to_string(),
    }
}

/// Byte offset and length of the next `href` / `src` attribute name at or after
/// `from`. The name must start an attribute (preceded by whitespace, `<`, a
/// quote or `/`) and must not be a prefix of a longer name — `srcset` and
/// `data-src` are not link targets.
fn next_url_attr(lower: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = lower.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        let name_len = if bytes[i..].starts_with(b"href") {
            4
        } else if bytes[i..].starts_with(b"src") {
            3
        } else {
            i += 1;
            continue;
        };
        let prev_ok = i == 0
            || matches!(
                bytes[i - 1],
                b' ' | b'\t' | b'\n' | b'\r' | b'<' | b'"' | b'\'' | b'/'
            );
        let next_ok = match bytes.get(i + name_len) {
            Some(c) => c.is_ascii_whitespace() || matches!(c, b'=' | b'/' | b'>'),
            None => true,
        };
        if prev_ok && next_ok {
            return Some((i, name_len));
        }
        i += 1;
    }
    None
}

/// Text carried inside `<script type="application/json">` blocks (Next.js
/// `__NEXT_DATA__` and friends). Returns `""` unless the payload carries enough
/// Chinese to be a real body — ordinary pages must be unaffected.
fn embedded_json_text(html: &str) -> String {
    let mut seen: HashSet<String> = HashSet::new();
    let mut pieces: Vec<String> = Vec::new();
    for body in json_script_bodies(html) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(body.trim()) else {
            continue;
        };
        let mut strings = Vec::new();
        collect_json_strings(&value, &mut strings);
        for s in strings {
            let text = strip_tags_fallback(&s);
            if text.chars().count() < 8 || han_count(&text) < 4 {
                continue;
            }
            if seen.insert(text.clone()) {
                pieces.push(text);
            }
        }
    }
    let joined = pieces.join("\n");
    if han_count(&joined) < EMBEDDED_JSON_MIN_HAN {
        return String::new();
    }
    joined.chars().take(EMBEDDED_JSON_MAX_CHARS).collect()
}

/// Every string value in a JSON tree, in document order.
fn collect_json_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                collect_json_strings(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                collect_json_strings(item, out);
            }
        }
        _ => {}
    }
}

/// Bodies of `<script>` blocks whose opening tag declares JSON (`type="…/json"`
/// or `id="__NEXT_DATA__"`).
fn json_script_bodies(html: &str) -> Vec<&str> {
    let lower = ascii_lower(html);
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(rel) = lower[i..].find("<script") {
        let start = i + rel;
        let after = lower[start + 7..].chars().next();
        if !matches!(after, Some(' ' | '>' | '\t' | '\n' | '\r' | '/')) {
            i = start + 7;
            continue;
        }
        let Some(gt) = html[start..].find('>') else {
            break;
        };
        let open_end = start + gt + 1;
        let open_tag = &lower[start..open_end];
        let Some(crel) = lower[open_end..].find("</script>") else {
            break;
        };
        if open_tag.contains("application/json") || open_tag.contains("__next_data__") {
            out.push(&html[open_end..open_end + crel]);
        }
        i = open_end + crel + "</script>".len();
    }
    out
}

/// Lowercase ASCII letters only, preserving byte length so offsets computed on
/// the result map back onto the original string.
fn ascii_lower(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                c
            }
        })
        .collect()
}

/// Remove every `<tag …>…</tag>` block (case-insensitive). For void/unclosed
/// tags the rest of the document from the opening tag is dropped.
fn remove_tag_blocks(html: &str, tag: &str) -> String {
    let lower = ascii_lower(html);
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut i = 0usize;
    while i < html.len() {
        let Some(rel) = lower[i..].find(&open) else {
            out.push_str(&html[i..]);
            break;
        };
        let start = i + rel;
        // Confirm the match is the whole tag name, not a prefix (e.g. <nav> vs
        // <navigation>): the next char must be a tag-name boundary.
        let after = lower[start + open.len()..].chars().next();
        let is_boundary = match after {
            Some(c) => matches!(c, ' ' | '>' | '/' | '\t' | '\n' | '\r'),
            None => true,
        };
        if !is_boundary {
            out.push_str(&html[i..start + 1]);
            i = start + 1;
            continue;
        }
        out.push_str(&html[i..start]);
        match lower[start..].find(&close) {
            Some(crel) => i = start + crel + close.len(),
            None => break, // no closing tag — drop the remainder
        }
    }
    out
}

fn remove_comments(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Collapse runs of 3+ blank lines down to a single blank line and trim.
fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank_run = 0usize;
    for line in s.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out.trim().to_string()
}

/// Fallback used only if htmd fails to parse: strip remaining tags to plain
/// text. Operates on already-boilerplate-stripped HTML, so it's clean text.
fn strip_tags_fallback(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut last_was_space = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            _ if in_tag => {}
            _ if c.is_whitespace() => {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            _ => {
                out.push(c);
                last_was_space = false;
            }
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 抽取路径的替身：按脚本逐次给答案（`None` = 像推理模型把预算吃光那样回
    /// 空串），或直接把请求打回来。顺手记下每次请求的 `max_tokens`，好断言
    /// 「重试真的发生了、预算真的变大了」。
    struct FakeExtract {
        /// 第 n 次调用取第 n 项，用光后一直用最后一项。
        script: Vec<Option<&'static str>>,
        fail: bool,
        budget_log: std::sync::Mutex<Vec<u32>>,
    }

    impl FakeExtract {
        fn answering(script: Vec<Option<&'static str>>) -> Self {
            Self {
                script,
                fail: false,
                budget_log: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn broken() -> Self {
            Self {
                script: Vec::new(),
                fail: true,
                budget_log: std::sync::Mutex::new(Vec::new()),
            }
        }

        /// 每次抽取请求收到的 `max_tokens`，按调用顺序。
        fn budgets(&self) -> Vec<u32> {
            self.budget_log.lock().expect("budget log").clone()
        }
    }

    #[async_trait::async_trait]
    impl hermes_core::LlmProvider for FakeExtract {
        async fn complete(
            &self,
            req: hermes_core::CompletionRequest,
        ) -> hermes_core::Result<hermes_core::CompletionResponse> {
            let attempt = {
                let mut log = self.budget_log.lock().expect("budget log");
                log.push(req.max_tokens);
                log.len() - 1
            };
            if self.fail {
                return Err(hermes_core::Error::ToolHost("provider down".into()));
            }
            let idx = attempt.min(self.script.len().saturating_sub(1));
            let answer = self.script.get(idx).copied().flatten();
            let content = answer
                .map(|t| {
                    vec![hermes_core::ContentBlock::Text {
                        text: t.to_string(),
                    }]
                })
                .unwrap_or_default();
            Ok(hermes_core::CompletionResponse {
                content,
                // 空串 = 推理把输出预算吃光；真实 provider 这时报的就是 MaxTokens
                // （见 `hermes-llm/src/openai.rs` 的 reasoning-only 用例）。
                stop_reason: if answer.is_some() {
                    hermes_core::StopReason::EndTurn
                } else {
                    hermes_core::StopReason::MaxTokens
                },
                usage: hermes_core::Usage::default(),
                truncated_tool_ids: Vec::new(),
            })
        }

        fn capabilities(&self) -> hermes_core::Capabilities {
            hermes_core::Capabilities::default()
        }

        fn name(&self) -> &str {
            "fake-extract"
        }
    }

    /// 建上下文，并把替身的把手交回来 —— 要断言调用次数与预算就得留着它。
    fn ctx_with(fake: FakeExtract) -> (WebToolsContext, std::sync::Arc<FakeExtract>) {
        let fake = std::sync::Arc::new(fake);
        let ctx = WebToolsContext {
            extract_provider: fake.clone(),
            extract_model: String::new(),
            extract_max_tokens: crate::web::DEFAULT_EXTRACT_MAX_TOKENS,
            search_backend: crate::web::SearchBackend::Scraper,
            tavily_api_key: String::new(),
            brave_api_key: String::new(),
            searxng_url: String::new(),
            cache_ttl_secs: 900,
        };
        (ctx, fake)
    }

    /// 预置缓存，免得测试真去打网络。
    fn seed(url: &str, markdown: &str) {
        web_cache::put(web_cache::fetch_key(url), markdown.to_string());
    }

    fn long_page() -> String {
        // 必须超过 80 字，否则会先撞上「页面几乎没字」那一支。
        "首页新闻：央行公布 LPR 维持不变。".repeat(40)
    }

    /// 抽取回空串时**不许**退回整页原文 —— 那条路一次往上下文里塞两万字，
    /// 2026-09-20 实测把海燕整轮的输出预算撞爆、零产出。
    #[tokio::test]
    async fn an_empty_extraction_never_falls_back_to_the_whole_page() {
        let url = "https://example.com/empty-extract-test";
        let page = long_page();
        seed(url, &page);
        let (ctx, _fake) = ctx_with(FakeExtract::answering(vec![None]));
        let out = run(
            std::path::Path::new("."),
            serde_json::json!({"url": url, "prompt": "今天的新闻"}),
            Some(&ctx),
        )
        .await
        .expect("run");
        assert!(!out.is_error, "拿不到不该报红：{}", out.content);
        assert!(
            out.content.contains("拿不到"),
            "必须明说拿不到：{}",
            out.content
        );
        assert!(
            out.content.chars().count() < 500,
            "不许把整页原文倒回来（{} 字）",
            out.content.chars().count()
        );
        assert!(
            !out.content.contains("首页新闻：央行公布 LPR"),
            "整页原文泄进来了：{}",
            out.content
        );
    }

    #[tokio::test]
    async fn a_working_extraction_is_returned_verbatim() {
        let url = "https://example.com/good-extract-test";
        seed(url, &long_page());
        let (ctx, _fake) = ctx_with(FakeExtract::answering(vec![Some(
            "【官方】新华财经｜2026-09-20 09:27｜LPR 不变｜https://x/1",
        )]));
        let out = run(
            std::path::Path::new("."),
            serde_json::json!({"url": url, "prompt": "今天的新闻"}),
            Some(&ctx),
        )
        .await
        .expect("run");
        assert!(out.content.contains("LPR 不变"), "{}", out.content);
        assert!(out.content.contains("[extracted from"), "{}", out.content);
    }

    #[tokio::test]
    async fn a_broken_extraction_says_so_instead_of_dumping_the_page() {
        let url = "https://example.com/broken-extract-test";
        seed(url, &long_page());
        let (ctx, _fake) = ctx_with(FakeExtract::broken());
        let out = run(
            std::path::Path::new("."),
            serde_json::json!({"url": url, "prompt": "今天的新闻"}),
            Some(&ctx),
        )
        .await
        .expect("run");
        assert!(out.is_error, "provider 挂了要明说：{}", out.content);
        assert!(out.content.contains("provider down"), "{}", out.content);
        assert!(
            !out.content.contains("首页新闻：央行公布 LPR"),
            "{}",
            out.content
        );
    }

    /// 抽取回空串多半是推理把输出预算吃光了。这一轮本来就要作废，**加预算重问
    /// 一次只有赚** —— 2026-09-21 实测上证报 / 证券时报 / e公司 / 券商中国 /
    /// 第一财经 / 财新六个站当轮全栽在这上面。重试必须在同一次调用里做完，
    /// 不能指望上级再喊一次。
    #[tokio::test]
    async fn an_empty_extraction_is_retried_once_with_a_bigger_budget() {
        let url = "https://example.com/retry-extract-test";
        seed(url, &long_page());
        let (ctx, fake) = ctx_with(FakeExtract::answering(vec![
            None,
            Some("【主流】证券时报｜2026-09-20 23:13｜上纬新材发售消费级人形机器人｜https://x/1"),
        ]));
        let out = run(
            std::path::Path::new("."),
            serde_json::json!({"url": url, "prompt": "今天的新闻"}),
            Some(&ctx),
        )
        .await
        .expect("run");
        assert!(!out.is_error, "{}", out.content);
        assert!(
            out.content.contains("上纬新材"),
            "重试拿到的正文没返回：{}",
            out.content
        );
        let budgets = fake.budgets();
        assert_eq!(budgets.len(), 2, "必须恰好重试一次：{budgets:?}");
        assert_eq!(
            budgets[0],
            crate::web::DEFAULT_EXTRACT_MAX_TOKENS,
            "首问预算该是配置值：{budgets:?}"
        );
        assert!(budgets[1] > budgets[0], "重试没有加预算：{budgets:?}");
    }

    /// 两次都空才认账 —— 而且是「拿不到」，不是把整页倒回来，也不是一直问下去。
    #[tokio::test]
    async fn a_twice_empty_extraction_gives_up_after_one_retry() {
        let url = "https://example.com/twice-empty-extract-test";
        seed(url, &long_page());
        let (ctx, fake) = ctx_with(FakeExtract::answering(vec![None]));
        let out = run(
            std::path::Path::new("."),
            serde_json::json!({"url": url, "prompt": "今天的新闻"}),
            Some(&ctx),
        )
        .await
        .expect("run");
        assert!(!out.is_error, "拿不到不该报红：{}", out.content);
        assert!(out.content.contains("拿不到"), "{}", out.content);
        assert!(
            out.content.contains("重问一次"),
            "得说清已经加预算重试过：{}",
            out.content
        );
        assert!(out.content.chars().count() < 500, "{}", out.content);
        assert!(
            !out.content.contains("首页新闻：央行公布 LPR"),
            "整页原文泄进来了：{}",
            out.content
        );
        assert_eq!(
            fake.budgets().len(),
            2,
            "只许重试一次：{:?}",
            fake.budgets()
        );
    }

    /// 第一次就拿到正文时不许再问一遍 —— 重试是兜底，不是例行。
    #[tokio::test]
    async fn a_working_extraction_is_not_retried() {
        let url = "https://example.com/no-retry-test";
        seed(url, &long_page());
        let (ctx, fake) = ctx_with(FakeExtract::answering(vec![Some("一次就成")]));
        let out = run(
            std::path::Path::new("."),
            serde_json::json!({"url": url, "prompt": "今天的新闻"}),
            Some(&ctx),
        )
        .await
        .expect("run");
        assert!(out.content.contains("一次就成"), "{}", out.content);
        assert_eq!(fake.budgets().len(), 1, "不该重试：{:?}", fake.budgets());
    }

    /// 预算已经顶到上限时只问一次，不做无谓的第二问；没顶到就按倍率翻。
    #[test]
    fn a_budget_at_the_ceiling_is_not_doubled() {
        assert_eq!(double_budget(crate::web::EXTRACT_RETRY_MAX_TOKENS), None);
        assert!(double_budget(crate::web::EXTRACT_RETRY_MAX_TOKENS - 1).is_some());
        assert_eq!(
            double_budget(crate::web::DEFAULT_EXTRACT_MAX_TOKENS),
            Some(crate::web::DEFAULT_EXTRACT_MAX_TOKENS * crate::web::EXTRACT_RETRY_FACTOR)
        );
    }

    #[test]
    fn strips_script_and_style_content() {
        let html = r#"<html><head><title>T</title><style>body{color:red;font-size:14px}</style></head>
            <body><h1>Real Heading</h1><script>var x=1;function f(){return 42;}window.__NEXT_DATA__={a:1}</script>
            <p>Actual article text here.</p></body></html>"#;
        let md = clean_html_to_markdown(html, None);
        assert!(md.contains("Real Heading"), "heading missing: {md}");
        assert!(md.contains("Actual article text"), "body missing: {md}");
        // None of the JS/CSS noise should survive.
        assert!(!md.contains("color:red"), "css leaked: {md}");
        assert!(!md.contains("font-size"), "css leaked: {md}");
        assert!(!md.contains("__NEXT_DATA__"), "js json leaked: {md}");
        assert!(!md.contains("function f"), "js leaked: {md}");
        assert!(!md.contains("var x"), "js leaked: {md}");
    }

    #[test]
    fn removes_comments_and_nav_footer() {
        let html = "<body><nav><a href=#>menu junk</a></nav><!-- tracking pixel -->\
            <main><p>Content paragraph.</p></main><footer>copyright junk</footer></body>";
        let md = clean_html_to_markdown(html, None);
        assert!(md.contains("Content paragraph"), "{md}");
        assert!(!md.contains("menu junk"), "nav leaked: {md}");
        assert!(!md.contains("copyright junk"), "footer leaked: {md}");
        assert!(!md.contains("tracking pixel"), "comment leaked: {md}");
    }

    #[test]
    fn tag_name_boundary_not_prefix() {
        // <navigation> must NOT be removed by the `nav` rule.
        let out = remove_tag_blocks("<navigation>keep me</navigation>", "nav");
        assert!(out.contains("keep me"), "over-matched prefix: {out}");
    }

    #[test]
    fn collapse_blank_lines_works() {
        assert_eq!(collapse_blank_lines("a\n\n\n\nb"), "a\n\nb");
    }

    fn base(url: &str) -> reqwest::Url {
        reqwest::Url::parse(url).expect("test base url")
    }

    /// 站根相对链接必须按被抓页面的 URL 补成绝对地址 —— 否则下游只能拿到
    /// `/article/detail/4192261.html`，模型写不出来源就只能填栏目页或干脆留空。
    #[test]
    fn relative_links_are_absolutized_against_the_page_url() {
        let html = r#"<body><a href="/a/b.html">站根相对</a><a href="c/d.html">同级相对</a>            <a href="//other.com/e.html">协议相对</a><img src="/img/x.png"></body>"#;
        let md = clean_html_to_markdown(html, Some(&base("https://x.com/n/1.html")));
        assert!(md.contains("https://x.com/a/b.html"), "{md}");
        assert!(md.contains("https://x.com/n/c/d.html"), "{md}");
        assert!(md.contains("https://other.com/e.html"), "{md}");
        assert!(md.contains("https://x.com/img/x.png"), "{md}");
    }

    /// 非导航协议与页内锚点原样保留，绝不能被 resolve 成 `https://…`。
    #[test]
    fn non_http_schemes_and_fragments_are_left_alone() {
        let html = r##"<body><a href="javascript:void(0)">js</a><a href="mailto:a@b.com">mail</a>            <a href="tel:10086">tel</a><a href="#top">锚点</a><a href="HTTPS://X.com/Y">绝对</a>            <a href="data:text/plain,hi">data</a></body>"##;
        let md = clean_html_to_markdown(html, Some(&base("https://x.com/n/1.html")));
        // htmd 会把 URL 里的括号转义成 `\(`，所以只断言协议头原样。
        assert!(md.contains("javascript:void"), "{md}");
        assert!(md.contains("mailto:a@b.com"), "{md}");
        assert!(md.contains("tel:10086"), "{md}");
        assert!(md.contains("#top"), "{md}");
        assert!(md.contains("HTTPS://X.com/Y"), "{md}");
        assert!(md.contains("data:text/plain,hi"), "{md}");
        assert!(!md.contains("https://x.com/n/javascript"), "{md}");
        assert!(!md.contains("https://x.com/n/1.html#"), "{md}");
    }

    /// `srcset` / `data-src` 不是链接目标：名字边界没看住就会把图片描述改烂。
    #[test]
    fn url_attribute_matching_respects_name_boundaries() {
        let html = r#"<body><img srcset="/a/1.png 1x, /a/2.png 2x" src="/real/0.png"><div data-src="/b/3.png"></div></body>"#;
        let out = absolutize_urls(html, &base("https://x.com/n/1.html"));
        assert!(
            out.contains(r#"srcset="/a/1.png 1x, /a/2.png 2x""#),
            "srcset 被改写：{out}"
        );
        assert!(
            out.contains(r#"data-src="/b/3.png""#),
            "data-src 被改写：{out}"
        );
        assert!(out.contains(r#"src="https://x.com/real/0.png""#), "{out}");
    }

    /// Next.js 站的正文全在 `__NEXT_DATA__` 的 JSON 里，去标签只能拿到空壳。
    /// 这里要求把 JSON 里的中文句子作为兜底正文捞回来。
    #[test]
    fn embedded_next_data_json_is_surfaced_as_fallback_text() {
        let list: Vec<serde_json::Value> = (0..40)
            .map(|i| {
                serde_json::json!({
                    "title": format!("产业动态标题{i}：新能源产业链迎来重大变化"),
                    "brief": format!("这是第{i}条摘要，介绍了产业政策、市场走势与资本运作的关键进展。"),
                })
            })
            .collect();
        let payload = serde_json::json!({"props": {"pageProps": {"list": list}}});
        let html = format!(
            "<html><body><div id=\"root\"></div>\
             <script id=\"__NEXT_DATA__\" type=\"application/json\">{payload}</script>\
             </body></html>"
        );
        let md = clean_html_to_markdown(&html, None);
        assert!(
            md.contains("新能源产业链迎来重大变化"),
            "JSON 正文没捞回来：{md}"
        );
        assert!(md.contains("<!-- page-embedded-json -->"), "{md}");
        assert!(
            !md.contains("__NEXT_DATA__"),
            "原始 JSON 骨架漏出来了：{md}"
        );
    }

    /// 兜底是兜底：JSON 里的中文凑不够量时，普通页面必须一字不加。
    #[test]
    fn small_embedded_json_is_not_appended() {
        let html = "<html><body><p>普通页面正文在这里。</p>\
            <script type=\"application/json\">{\"props\":{\"pageProps\":{\"title\":\"中文标题\",\"brief\":\"中文摘要\"}}}</script>\
            </body></html>";
        let md = clean_html_to_markdown(html, None);
        assert!(md.contains("普通页面正文在这里"), "{md}");
        assert!(
            !md.contains("<!-- page-embedded-json -->"),
            "薄 JSON 被拼上了：{md}"
        );
        assert!(!md.contains("中文标题"), "{md}");
    }

    // ── 批 5：字符集（GBK 站点）──────────────────────────────────────────────

    /// GBK 页按 `<meta charset>` 解，不能整页乱码。
    #[test]
    fn gbk_pages_are_decoded_from_the_meta_charset() {
        let html = "<html><head><meta charset=\"gb2312\"></head>\
            <body><p>中共中央政治局召开会议讨论十五五规划</p></body></html>";
        let (bytes, _, _) = encoding_rs::GBK.encode(html);
        let text = decode_html_bytes(&bytes, None);
        assert!(
            text.contains("中共中央政治局召开会议讨论十五五规划"),
            "{text}"
        );
        assert!(!text.contains('\u{fffd}'), "解出替换字符＝还是乱码：{text}");
    }

    /// Header 里的 charset 优先于 meta。
    #[test]
    fn header_charset_wins_over_the_meta_tag() {
        let html = "<html><head><meta charset=\"utf-8\"></head><body>中文内容测试</body></html>";
        let (bytes, _, _) = encoding_rs::GBK.encode(html);
        let text = decode_html_bytes(&bytes, Some("gbk"));
        assert!(text.contains("中文内容测试"), "{text}");
    }

    /// 没写编码的普通 UTF-8 页（绝大多数）行为不变。
    #[test]
    fn utf8_pages_are_untouched() {
        let html = "<html><body><p>普通页面正文在这里。</p></body></html>";
        assert_eq!(decode_html_bytes(html.as_bytes(), None), html);
    }

    /// `Content-Type: text/html;charset=gbk` 能读出来。
    #[test]
    fn content_type_charset_is_parsed() {
        assert_eq!(
            charset_from_content_type("text/html;charset=gbk").as_deref(),
            Some("gbk")
        );
        assert_eq!(
            charset_from_content_type("text/html; charset=GB2312").as_deref(),
            Some("gb2312")
        );
        assert_eq!(charset_from_content_type("text/html"), None);
    }

    // ── 批 5 / C 批：长页抽取只送「窗口日期最密」的一段 ───────────────────────

    fn days(d: &[&str]) -> Vec<String> {
        d.iter().map(|s| s.to_string()).collect()
    }

    /// 提示词里的日期能被认出来（三种写法都要认），且**窗口里每一天**都认。
    #[test]
    fn focus_tokens_read_the_dates_out_of_the_prompt() {
        assert_eq!(
            focus_tokens("只列发布时间在 2026-09-21 当天的新闻"),
            days(&["2026-09-21"])
        );
        // 跨日窗口（说「近 3 天」时跨 3 天）——窗口里每一天都要认，
        // 只认第一个会把后一天那批稿子判成「不在窗口内」。
        assert_eq!(
            focus_tokens("时间窗口 2026-09-20 00:00 — 2026-09-22 15:40（含起、不含止）"),
            days(&["2026-09-20", "2026-09-22"])
        );
        assert_eq!(focus_tokens("窗口 2026/09/21 全天"), days(&["2026/09/21"]));
        assert!(
            focus_tokens("列出当天条目").is_empty(),
            "没写日期就退回整页"
        );
    }

    /// 跨天窗口：切窗按整段窗口挑最密的段，漏答判定也按整段窗口数。
    #[test]
    fn a_window_spanning_two_days_focuses_on_the_densest_day() {
        let mut page = String::new();
        for i in 0..400 {
            page.push_str(&format!("无关的旧闻第 {i} 条 filler filler filler\n"));
        }
        for i in 0..20 {
            page.push_str(&format!("2026-09-22 08:{:02} 窗口末日第 {i} 条\n", i));
        }
        for i in 0..400 {
            page.push_str(&format!("页面底部导航第 {i} 条 filler filler filler\n"));
        }
        let focus = days(&["2026-09-21", "2026-09-22"]);
        let windows = ranked_windows(&page, &focus, 2_000);
        assert!(
            windows[0].contains("窗口末日第 0 条"),
            "窗口里最新那天最密的段要排第一——只认窗口头一天会挑错段"
        );
        // 只数窗口头一天（09-21）会一处都数不到，等于漏答判别失效。
        assert_eq!(focus_set_count(&page, &days(&["2026-09-21"])), 0);
        assert!(focus_set_count(&page, &focus) >= 5);
    }

    /// 长页切窗：互不重叠、日期最密的那段排第一。
    #[test]
    fn long_pages_are_cut_into_ranked_windows() {
        let mut page = String::new();
        for i in 0..400 {
            page.push_str(&format!("无关的旧闻第 {i} 条 filler filler filler\n"));
        }
        for i in 0..20 {
            page.push_str(&format!("2026-09-21 14:{:02} 当天第 {i} 条新闻标题\n", i));
        }
        for i in 0..400 {
            page.push_str(&format!("页面底部导航第 {i} 条 filler filler filler\n"));
        }
        let windows = ranked_windows(&page, &days(&["2026-09-21"]), 2_000);
        assert!(windows.len() > 2, "长页应该切成多段：{}", windows.len());
        assert!(
            windows[0].contains("当天第 0 条新闻标题"),
            "最密的一段没排第一"
        );
        let total: usize = windows.iter().map(|w| w.len()).sum();
        assert!(
            total <= page.len(),
            "窗口之间不许重叠：{total} > {}",
            page.len()
        );
        let all: String = windows.concat();
        assert_eq!(
            all.matches("当天第 19 条新闻标题").count(),
            1,
            "当天那段必须完整落在一个窗口里"
        );
    }

    /// 页面里明明有 5 处以上这个日期、答案却短到只有一句「没有」→ 判漏答。
    #[test]
    fn a_short_no_items_answer_over_a_dated_page_is_a_miss() {
        let page = "2026-09-21 新闻甲\n2026-09-21 新闻乙\n2026-09-21 新闻丙\n\
                    2026-09-21 新闻丁\n2026-09-21 新闻戊\n";
        assert!(answer_looks_like_a_miss(
            "本页无当天条目。",
            page,
            &days(&["2026-09-21"])
        ));
        // 页面里本来就没有这个日期 → 那是诚实的「没有」，不算漏答。
        assert!(!answer_looks_like_a_miss(
            "本页无当天条目。",
            "2026-09-20 旧闻",
            &days(&["2026-09-21"])
        ));
        // 长答案（真的列了条目）不算漏答。
        let long = "1. 甲｜2026-09-21 09:10｜主体：A；关键数据：1 亿元｜https://x/1".repeat(4);
        assert!(!answer_looks_like_a_miss(
            &long,
            page,
            &days(&["2026-09-21"])
        ));
    }

    /// 抽日期不能只看正文：日期常只写在链接地址里（中国证券报就是这样）。
    #[test]
    fn the_extraction_prompt_says_dates_hide_in_link_urls() {
        assert!(
            EXTRACT_SYSTEM.contains("link URL") && EXTRACT_SYSTEM.contains("2026/09/21"),
            "系统提示词必须点明日期可能只写在链接里：{EXTRACT_SYSTEM}"
        );
    }

    /// 同一天的不同写法都要数到。
    #[test]
    fn the_same_day_counts_in_every_separator_form() {
        let page = concat!(
            "2026-09-21 14:02 连字符写法\n",
            "2026/09/21 虚斜杠写法\n",
            "2026.09.21 虚点写法\n",
            "2026年9月21日 中文写法\n",
            "2026年09月21日 中文补零写法\n",
        );
        assert_eq!(focus_count(page, "2026-09-21"), 5, "五种写法各算一次");
        assert_eq!(date_variants("2026-09-21").len(), 8);
        assert_eq!(date_variants("当天"), vec!["当天".to_string()]);
        // 页面结构变了、写法换了，漏答判别照样生效。
        assert!(answer_looks_like_a_miss(
            "本页无当天条目。",
            page,
            &days(&["2026-09-21"])
        ));
        // 别的日子不受影响。
        assert_eq!(focus_count(page, "2026-09-20"), 0);
    }

    /// 中国证券报首页当天 23 条的日期只写在链接路径里 —— 必须数得到。
    #[test]
    fn a_date_hidden_in_the_url_path_still_counts() {
        let page = concat!(
            "https://www.cs.com.cn/ssgs/01/2026/09/21/detail_2026092110040702.html\n",
            "https://www.cs.com.cn/xwzx/01/2026/09/21/detail_2026092110040608.html\n",
            "https://www.cs.com.cn/ssgs/01/2026/09/21/detail_2026092110040590.html\n",
        );
        assert!(focus_count(page, "2026-09-21") >= 3, "路径里的日期要数到");
        assert!(answer_looks_like_a_miss(
            "本页无当天条目。",
            page,
            &days(&["2026-09-21"])
        ));
    }

    /// 空串永远不可用；有日期焦点时漏答也不可用；其余照收。
    #[test]
    fn usable_answer_rejects_empty_and_misses() {
        let page = "2026-09-21 新闻甲\n2026-09-21 新闻乙\n2026-09-21 新闻丙\n\
                    2026-09-21 新闻丁\n2026-09-21 新闻戊\n";
        assert!(!usable_answer("   ", page, &days(&["2026-09-21"])));
        assert!(!usable_answer(
            "本页无当天条目。",
            page,
            &days(&["2026-09-21"])
        ));
        assert!(usable_answer(
            "1. 甲｜2026-09-21 09:10｜主体：A；关键数据：1 亿元｜https://x/1",
            page,
            &days(&["2026-09-21"])
        ));
    }
}
