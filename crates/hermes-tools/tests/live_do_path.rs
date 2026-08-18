//! Live checks for write → Office package → open. Ignored in default `cargo test`
//! because they launch the system opener (Word / browser).

use hermes_tools::{open, web_search, write};

#[tokio::test]
#[ignore = "launches Word/browser; run with --ignored"]
async fn write_office_to_desktop_and_open() {
    let ws = tempfile::tempdir().unwrap();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let docx_name = format!("乐彼AI测试-简报-{stamp}.docx");
    let xlsx_name = format!("乐彼AI测试-表格-{stamp}.xlsx");

    let docx_out = write::run(
        ws.path(),
        serde_json::json!({
            "path": format!("~/Desktop/{docx_name}"),
            "content": "乐彼AI 打开与导出测试\n这是一份真实的 Word 文件，不是纯文本改后缀。\n日期：2026-08-14"
        }),
    )
    .await
    .unwrap();
    assert!(!docx_out.is_error, "{}", docx_out.content);

    let xlsx_out = write::run(
        ws.path(),
        serde_json::json!({
            "path": format!("~/Desktop/{xlsx_name}"),
            "content": "项目,金额\n测试文档,1\n测试表格,2"
        }),
    )
    .await
    .unwrap();
    assert!(!xlsx_out.is_error, "{}", xlsx_out.content);

    let desktop = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Desktop");
    let docx = desktop.join(&docx_name);
    let xlsx = desktop.join(&xlsx_name);
    assert!(docx.exists(), "{}", docx.display());
    assert!(xlsx.exists(), "{}", xlsx.display());
    let db = std::fs::read(&docx).unwrap();
    let xb = std::fs::read(&xlsx).unwrap();
    assert_eq!(&db[0..2], b"PK");
    assert_eq!(&xb[0..2], b"PK");

    let opened = open::run(
        ws.path(),
        serde_json::json!({ "target": format!("~/Desktop/{docx_name}") }),
    )
    .await
    .unwrap();
    assert!(
        !opened.is_error,
        "opening Word on Desktop failed: {}",
        opened.content
    );

    let page = open::run(
        ws.path(),
        serde_json::json!({ "target": "https://example.com/" }),
    )
    .await
    .unwrap();
    assert!(!page.is_error, "opening webpage failed: {}", page.content);
}

#[tokio::test]
#[ignore = "hits the live web"]
async fn search_returns_http_pages_or_honest_failure() {
    let ws = tempfile::tempdir().unwrap();
    let out = web_search::run(
        ws.path(),
        serde_json::json!({ "query": "site:example.com example domain", "limit": 3 }),
        None,
    )
    .await
    .unwrap();
    if out.is_error {
        assert!(
            out.content.contains("No results") || out.content.contains("web_search error"),
            "failure must be honest, got: {}",
            out.content
        );
        return;
    }
    assert!(
        !out.content.contains(".css") || out.content.contains("http"),
        "{}",
        out.content
    );
    assert!(
        !out.content.contains("r.bing.com/rp/"),
        "junk CDN leaked: {}",
        out.content
    );
}
