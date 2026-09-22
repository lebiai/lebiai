//! 防复发：UI 真实用到的 Tauri 能力，必须在 `capabilities/default.json` 里授权。
//!
//! 背景（见 `docs/records/20260913-gui-close-button.md`）：`App.tsx` 的关闭流程调用
//! `getCurrentWindow().destroy()`，而 `core:default` 展开后的 `core:window:default`
//! **不含** `allow-destroy`。权限漏配既不报编译错、也不会让任何既有测试变红，
//! 只会在用户点下去时静默失败 —— 所以在这里锁死。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// UI 源码里的调用指纹 → 必须被授予的权限。
///
/// 每条都**双向**校验：指纹找不到 = 映射表过期（要求同步），
/// 权限没授予 = 用户点下去会静默失败。这样映射表不会烂掉后假装通过。
const REQUIRED: &[(&str, &str)] = &[
    (".destroy(", "core:window:allow-destroy"),
    // 正文里的链接（来源小标签）走系统浏览器。WebView 自己不会开外链：漏了这条
    // 权限，用户点下去同样静默无反应——`opener:default` 展开含 `allow-open-url`。
    ("openUrl(", "opener:allow-open-url"),
];

fn gui_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 拼接 `ui/src` 下全部 TypeScript 源码，供指纹匹配。
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

fn read_json(path: &Path) -> serde_json::Value {
    let raw = fs::read_to_string(path).unwrap_or_else(|e| panic!("读不到 {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{} 不是合法 JSON: {e}", path.display()))
}

fn manifests() -> serde_json::Value {
    read_json(&gui_dir().join("gen/schemas/acl-manifests.json"))
}

/// 展开一条授权：`…:default` 是权限集（递归展开），其余视为具体命令权限。
/// 权限集里的条目可能是短名（如 `allow-destroy`），按所在插件补齐命名空间。
fn expand(manifests: &serde_json::Value, id: &str, out: &mut BTreeSet<String>) {
    if let Some((namespace, leaf)) = id.rsplit_once(':') {
        if leaf == "default" {
            let set = manifests
                .get(namespace)
                .and_then(|node| node.get("default_permission"))
                .and_then(|default| default.get("permissions"))
                .and_then(|list| list.as_array());
            if let Some(list) = set {
                for item in list.iter().filter_map(|value| value.as_str()) {
                    let child = if item.contains(':') {
                        item.to_string()
                    } else {
                        format!("{namespace}:{item}")
                    };
                    expand(manifests, &child, out);
                }
                return;
            }
        }
    }
    out.insert(id.to_string());
}

fn expand_all(manifests: &serde_json::Value, ids: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for id in ids {
        expand(manifests, id, &mut out);
    }
    out
}

/// `capabilities/default.json` 实际生效的权限集合（权限集已展开）。
fn granted_permissions() -> BTreeSet<String> {
    let caps = read_json(&gui_dir().join("capabilities/default.json"));
    let ids: Vec<String> = caps["permissions"]
        .as_array()
        .expect("capabilities 的 permissions 必须是数组")
        .iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();
    expand_all(&manifests(), &ids)
}

#[test]
fn ui_window_api_calls_are_granted() {
    let source = ui_source();
    let granted = granted_permissions();
    let mut problems = Vec::new();
    for (needle, permission) in REQUIRED {
        if !source.contains(needle) {
            problems.push(format!(
                "UI 源码里已找不到 `{needle}`；请同步本测试的映射表"
            ));
        } else if !granted.contains(*permission) {
            problems.push(format!(
                "UI 调用了 `{needle}`，但 capabilities 未授予 `{permission}`"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn default_permission_sets_expand() {
    let manifests = manifests();

    let window_default = expand_all(&manifests, &["core:window:default".to_string()]);
    // 展开确实生效 —— 否则上面的测试会因为集合为空而以错误的理由通过。
    assert!(window_default.contains("core:window:allow-is-visible"));
    assert!(window_default.contains("core:window:allow-internal-toggle-maximize"));
    // 根因事实：默认集里既没有 destroy 也没有 close，所以必须显式授权。
    assert!(!window_default.contains("core:window:allow-destroy"));
    assert!(!window_default.contains("core:window:allow-close"));

    let core_default = expand_all(&manifests, &["core:default".to_string()]);
    assert!(core_default.contains("core:window:allow-is-visible"));
    assert!(core_default.contains("core:event:allow-listen"));
    // 展开后的权限集名会被成员替换：`core:default` ⊇ `core:window:default` 的成员。
    assert!(window_default.is_subset(&core_default));
}
