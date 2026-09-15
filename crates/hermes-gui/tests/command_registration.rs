//! 防复发：UI 里 `invoke("…")` 的命令，必须在 `main.rs` 的 `generate_handler!` 注册。
//!
//! 漏注册既不会编译报错、也不会让别的测试变红，只会在用户点下去时静默失败
//! （`list_outputs` / `open_output` 就是这么加进来的，见
//! `docs/records/20260913-quiet-failures-and-visible-outputs.md`）。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn gui_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 拼接 `ui/src` 下全部 TypeScript 源码。
fn ui_source() -> String {
    fn walk(dir: &Path, out: &mut String) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("ts" | "tsx")
            ) {
                out.push_str(&fs::read_to_string(&path).unwrap_or_default());
                out.push('\n');
            }
        }
    }

    let mut out = String::new();
    walk(&gui_dir().join("ui/src"), &mut out);
    assert!(!out.is_empty(), "ui/src 下没有读到任何 TypeScript 源码");
    out
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

fn skip_ws(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

/// 把注释与字符串字面量替换成空格（保持字节长度），避免
/// `"invoke(\"nope\")"` 这种字面量被当成真调用。
///
/// 只替换 ASCII 字节，不会切进多字节字符 —— UTF-8 续字节都 ≥ 0x80。
fn strip_comments_and_strings(src: &str) -> String {
    let mut b = src.as_bytes().to_vec();
    let n = b.len();
    let mut i = 0;
    while i < n {
        match b[i] {
            b'/' if i + 1 < n && b[i + 1] == b'/' => {
                while i < n && b[i] != b'\n' {
                    b[i] = b' ';
                    i += 1;
                }
            }
            b'/' if i + 1 < n && b[i + 1] == b'*' => {
                let start = i;
                i += 2;
                while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                let end = (i + 2).min(n);
                for byte in &mut b[start..end] {
                    *byte = b' ';
                }
                i = end;
            }
            quote @ (b'"' | b'\'' | b'`') => {
                let start = i;
                i += 1;
                while i < n && b[i] != quote {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                let end = (i + 1).min(n);
                for byte in &mut b[start..end] {
                    *byte = b' ';
                }
                i = end;
            }
            _ => i += 1,
        }
    }
    String::from_utf8(b).expect("只替换 ASCII 字节，UTF-8 仍然合法")
}

/// 抓出 `invoke(<泛型>)?("名字"` 里的名字。泛型里的 `<>` 按配对跳过。
///
/// 在**净化后**的源码里定位 `invoke` 这个词（这样字符串/注释里的
/// `invoke("nope")` 不会被算数），但结构字符与名字都回到**原文**里读 ——
/// 净化只做等长替换，两边偏移一致，而字符串字面量在净化版里已经连引号一起没了。
fn invoked_names(src: &str) -> BTreeSet<String> {
    let code = strip_comments_and_strings(src);
    let bytes = src.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;
    while let Some(pos) = code[i..].find("invoke") {
        let start = i + pos;
        i = start + "invoke".len();
        if start > 0 && is_ident(bytes[start - 1]) {
            continue;
        }
        let mut j = i;
        skip_ws(bytes, &mut j);
        if j < bytes.len() && bytes[j] == b'<' {
            let mut depth = 0;
            while j < bytes.len() {
                match bytes[j] {
                    b'<' => depth += 1,
                    b'>' => {
                        depth -= 1;
                        if depth == 0 {
                            j += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            skip_ws(bytes, &mut j);
        }
        if j >= bytes.len() || bytes[j] != b'(' {
            continue;
        }
        j += 1;
        skip_ws(bytes, &mut j);
        if j < bytes.len() && bytes[j] == b'"' {
            let name_start = j + 1;
            let mut name_end = name_start;
            while name_end < bytes.len() && bytes[name_end] != b'"' {
                name_end += 1;
            }
            out.insert(src[name_start..name_end].to_string());
            i = name_end;
        }
    }
    out
}

/// `generate_handler![commands::x::y, …]` 里的叶子名。
fn registered_names(main_src: &str) -> BTreeSet<String> {
    let start = main_src
        .find("generate_handler![")
        .expect("main.rs 里找不到 generate_handler!");
    let rest = &main_src[start..];
    let end = rest.find("])").expect("generate_handler! 没有结束");
    rest[..end]
        .split(',')
        .filter_map(|part| part.trim().rsplit("::").next())
        .filter(|name| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn every_invoked_command_is_registered() {
    let invoked = invoked_names(&ui_source());
    let registered = registered_names(&fs::read_to_string(gui_dir().join("src/main.rs")).unwrap());
    assert!(
        invoked.len() > 50,
        "扫描器失明了：只认出 {} 个 invoke 调用",
        invoked.len()
    );
    let missing: Vec<&str> = invoked
        .iter()
        .filter(|name| !registered.contains(*name))
        .map(String::as_str)
        .collect();
    assert!(
        missing.is_empty(),
        "这些命令 UI 会调用，但 main.rs 没注册：{missing:?}"
    );
}

#[test]
fn scanner_reads_generic_and_plain_invokes() {
    let found = invoked_names(
        r#"
        await invoke<SourceRow[]>("list_sources", { query });
        await invoke("open_output", { path: relPath });
        const notACall = "invoke(\"nope\")";
        "#,
    );
    assert!(found.contains("list_sources"));
    assert!(found.contains("open_output"));
    assert!(!found.contains("nope"));
}
