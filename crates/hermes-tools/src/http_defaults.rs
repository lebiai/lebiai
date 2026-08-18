//! Shared HTTP client and headers for web_fetch / web_search.

use once_cell::sync::Lazy;
use reqwest::redirect::Policy;
use reqwest::Client;

const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

fn browser_headers() -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert(
        reqwest::header::ACCEPT,
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
            .parse()
            .unwrap(),
    );
    h.insert(
        reqwest::header::ACCEPT_LANGUAGE,
        "zh-CN,zh;q=0.9,en;q=0.8".parse().unwrap(),
    );
    h
}

/// Default client (web_search etc.). Follows redirects without SSRF re-check;
/// callers that need SSRF must use [`FETCH_CLIENT`] + pre-validate the URL.
pub static HTTP_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .user_agent(BROWSER_UA)
        .default_headers(browser_headers())
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("failed to build shared HTTP client")
});

/// Client for `web_fetch`: re-validates every redirect hop against SSRF rules.
pub static FETCH_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .user_agent(BROWSER_UA)
        .default_headers(browser_headers())
        .timeout(std::time::Duration::from_secs(15))
        .redirect(Policy::custom(|attempt| {
            let url = attempt.url().to_string();
            if crate::url_safety::validate_public_http_url(&url).is_err() {
                attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("redirect to blocked URL: {url}"),
                ))
            } else if attempt.previous().len() >= 5 {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .expect("failed to build fetch HTTP client")
});
