//! 定义文件（人物 / 项目组）的 frontmatter 切分。
//!
//! `hermes-store` 有 `frontmatter::parse_doc_str`，但它依赖本 crate，反向引用会成环——
//! 这里留一份精简实现，**人物与项目组共用这一处**，别各写各的。
//!
//! 按**行边界**切分：只有整行等于 `---` 才算闭合，值内部（如折叠标量里的 `---`）
//! 不会被当成边界。Ruling 1.1-b，见 `docs/records/20260914-personas.md` §2.0.1。
pub(crate) fn split(path: &str, raw: &str) -> (String, String) {
    let mut lines = raw.lines();
    assert!(
        lines.next().map(str::trim_end) == Some("---"),
        "{path}: 定义必须以整行 `---` 开头"
    );
    let mut yaml = String::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    assert!(closed, "{path}: frontmatter 缺闭合的整行 `---`");
    (yaml, lines.collect::<Vec<_>>().join("\n"))
}
