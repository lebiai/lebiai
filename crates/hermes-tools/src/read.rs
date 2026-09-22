//! `read` — read a file with line numbers.

use std::path::Path;

use hermes_core::{Result, ToolCallOutcome, ToolSpec};
use serde::Deserialize;

use crate::safety;

/// 一次 `read` 的**单次返回上限**（字符）。与引擎侧
/// [`hermes_core::compaction::MAX_TOOL_RESULT_CHARS`] 是同一根绳子 —— 超过它，
/// 请求体折叠就会把这条结果砍掉，模型只能再读一次（2026-09-21 实测）。
/// 两侧由 `crate::TOOL_RESULT_CEILINGS` 上那条断言拴住。
pub const MAX_READ_CHARS: usize = hermes_core::compaction::MAX_TOOL_RESULT_CHARS;

#[derive(Deserialize)]
struct Args {
    path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "read".into(),
        description: "Read a file and return its contents with line numbers. \
            Use `offset` and `limit` to read specific sections of large files. \
            Always read a file before editing it to understand its structure and \
            find the exact text to replace. When a file is longer than one \
            call can return, the tail says exactly which `offset` continues it."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "File path relative to workspace"},
                "offset": {"type": "integer", "description": "Start line (0-based, default 0)"},
                "limit": {"type": "integer", "description": "Max lines to return (default 2000)"}
            },
            "required": ["path"]
        }),
        requires_confirmation: false,
    }
}

pub async fn run(workspace: &Path, args: serde_json::Value) -> Result<ToolCallOutcome> {
    let a: Args = serde_json::from_value(args)
        .map_err(|e| hermes_core::Error::ToolHost(format!("read: bad args: {e}")))?;
    let path = safety::resolve(workspace, &a.path)?;
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| hermes_core::Error::ToolHost(format!("read {}: {e}", path.display())))?;

    let offset = a.offset.unwrap_or(0);
    let limit = a.limit.unwrap_or(2000);
    let lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();

    // 一次调用吐多少，**由这里说了算**，而且说得出「接着读的把手」。
    //
    // 引擎侧有同一根绳子（`hermes_core::compaction::MAX_TOOL_RESULT_CHARS`）：
    // 超过它的工具结果会被请求体折叠砍掉，模型只能重取一次。**所以不许无声地多吐**，
    // 也不许「吐一半不说」——超了就在**行边界**切开，并写明 `offset=`。
    let mut out = String::new();
    let mut used = 0usize;
    let mut taken = 0usize;
    let mut clipped_line = false;
    for (i, line) in lines.iter().enumerate() {
        let rendered = format!("{:>5}\t{}\n", offset + i + 1, line);
        let width = rendered.chars().count();
        if taken == 0 && width > MAX_READ_CHARS {
            // 头一行本身就超预算（压缩过的 JSON / 一行的 bundle）：也要给出东西，
            // 否则模型连「从哪儿接着读」都不知道。截到上限，并把行号说清楚。
            out.extend(rendered.chars().take(MAX_READ_CHARS));
            taken = 1;
            clipped_line = true;
            break;
        }
        if used + width > MAX_READ_CHARS {
            break;
        }
        out.push_str(&rendered);
        used += width;
        taken += 1;
    }

    let total = content.lines().count();
    let next_offset = offset + taken;
    let remaining = total.saturating_sub(next_offset);
    if clipped_line {
        out.push_str(&format!(
            "... (第 {} 行本身就超过 {MAX_READ_CHARS} 字，已截断；后面还有 {remaining} 行，\
             用 offset={next_offset} 继续)\n",
            next_offset
        ));
    } else if remaining > 0 {
        out.push_str(&format!(
            "... (还有 {remaining} 行；用 offset={next_offset} 继续读)\n"
        ));
    }
    Ok(ToolCallOutcome {
        content: out,
        is_error: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(lines: &[String]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f.md"), lines.join("\n")).unwrap();
        dir
    }

    async fn read_out(
        dir: &tempfile::TempDir,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> String {
        let mut args = serde_json::json!({"path": "f.md"});
        if let Some(o) = offset {
            args["offset"] = serde_json::json!(o);
        }
        if let Some(l) = limit {
            args["limit"] = serde_json::json!(l);
        }
        run(dir.path(), args).await.unwrap().content
    }

    /// 短文件：原样给全，尾巴上不许出现「接着读」。
    #[tokio::test]
    async fn a_short_file_comes_back_whole_with_no_continuation() {
        let dir = tmp(&["第一行".into(), "第二行".into()]);
        let out = read_out(&dir, None, None).await;
        assert!(out.contains("第一行") && out.contains("第二行"), "{out}");
        assert!(!out.contains("继续读"), "没被切就不该出现把手：{out}");
    }

    /// 超上限：**在行边界**切开（行号连续、没有半行），并把 `offset=` 给对。
    #[tokio::test]
    async fn an_over_budget_file_is_cut_on_a_line_boundary_with_a_working_offset() {
        // 每行 100 个汉字 ⇒ 一行 ≈ 106 字符（含行号前缀与换行）。
        let line = "甲".repeat(100);
        let dir = tmp(&(0..1000).map(|_| line.clone()).collect::<Vec<_>>());

        let out = read_out(&dir, None, None).await;
        assert!(
            out.chars().count() <= MAX_READ_CHARS + 200,
            "吐出的正文不许超过上限（尾巴那句指路不算）：{} 字",
            out.chars().count()
        );
        let last = out
            .lines()
            .rev()
            .find(|l| !l.starts_with("... ("))
            .expect("至少要有一行正文");
        assert!(
            last.ends_with(&line),
            "最后一行必须是完整的：…{}",
            &last[last.len() - 8..]
        );
        let m = out.lines().last().expect("要有一句指路");
        assert!(m.starts_with("... ("), "指路句要说清怎么接着读：{m}");

        // 把手真的能用：按它给的 offset 续读，接上的正是被切掉的那一行。
        let next: usize = m
            .split("offset=")
            .nth(1)
            .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|d| d.parse().ok())
            .expect("指路句里必须有 offset");
        let first_line_no = out
            .lines()
            .find(|l| !l.starts_with("... ("))
            .and_then(|l| l.split('\t').next())
            .and_then(|n| n.trim().parse::<usize>().ok())
            .expect("行号前缀");
        let emitted = out.lines().filter(|l| !l.starts_with("... (")).count();
        assert_eq!(
            next,
            first_line_no - 1 + emitted,
            "offset 必须正好接在最后一行之后"
        );

        let rest = read_out(&dir, Some(next), Some(1)).await;
        let rest_first = rest.lines().next().unwrap();
        assert_eq!(
            rest_first
                .split('\t')
                .next()
                .unwrap()
                .trim()
                .parse::<usize>()
                .unwrap(),
            next + 1,
            "续读的第一行必须紧接着上一段"
        );
        assert!(rest_first.ends_with(&line), "续读回来的也必须是完整行");
    }

    /// `limit` 先切（没顶到字符预算）：照样给把手，不是老那句「N more lines」。
    #[tokio::test]
    async fn a_line_limit_cut_still_hands_over_an_offset() {
        let dir = tmp(&["一".into(), "二".into(), "三".into()]);
        let out = read_out(&dir, None, Some(2)).await;
        assert!(out.contains("还有 1 行"), "{out}");
        assert!(out.contains("用 offset=2 继续读"), "{out}");
    }
}
