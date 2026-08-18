//! Bearer-token auth for the HTTP/WS API.
//!
//! One shared secret gates every `/api/v1/*` request. REST clients send
//! `Authorization: Bearer <token>`. WebSocket clients should prefer a
//! **short-lived ticket** from `POST /api/v1/ws-ticket` then
//! `?ticket=<hex>` (single-use, 60s) so proxy access logs do not retain the
//! long-lived server token. Legacy `?token=<long-lived>` remains accepted for
//! older clients.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rand::RngCore;

/// CLI/env inputs controlling where the server token comes from. Resolution
/// order in [`resolve_token`]: explicit value → token file →
/// `HERMES_SERVER_TOKEN` → the persisted default file (generated on first run).
#[derive(Debug, Default, Clone)]
pub struct TokenOpts {
    /// `--token <value>`
    pub value: Option<String>,
    /// `--token-file <path>`
    pub file: Option<PathBuf>,
}

/// Default token file: `~/.lebi-ai/server.token`.
pub fn default_token_path() -> anyhow::Result<PathBuf> {
    Ok(hermes_core::data_path("server.token"))
}

/// Resolve the server token by precedence. If none of the explicit sources
/// yield one, the default file is loaded; if it does not yet exist, a fresh
/// 32-byte random token is generated and persisted (mode 0600 on unix).
/// Always returns a non-empty token → the server is **never** unauthenticated.
pub fn resolve_token(opts: &TokenOpts) -> anyhow::Result<String> {
    if let Some(t) = opts
        .value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(t.to_string());
    }
    if let Some(p) = &opts.file {
        let raw = std::fs::read_to_string(p)
            .map_err(|e| anyhow::anyhow!("reading --token-file {}: {e}", p.display()))?;
        let t = raw.trim();
        if t.is_empty() {
            anyhow::bail!("token file {} is empty", p.display());
        }
        return Ok(t.to_string());
    }
    if let Ok(t) = std::env::var("HERMES_SERVER_TOKEN") {
        let t = t.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    let path = default_token_path()?;
    if let Ok(raw) = std::fs::read_to_string(&path) {
        let t = raw.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    let generated = generate_token();
    save_token(&path, &generated)?;
    tracing::info!(path = %path.display(), "generated new server token");
    Ok(generated)
}

/// 32 random bytes as lowercase hex (64 chars) from the OS CSPRNG.
fn generate_token() -> String {
    let mut buf = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    hex_encode(&buf)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

fn save_token(path: &PathBuf, token: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

/// Shared secrets for the auth layer: long-lived bearer + ticket store.
#[derive(Clone)]
pub struct AuthState {
    pub token: Arc<String>,
    pub tickets: Arc<crate::tickets::TicketStore>,
}

/// axum middleware: accept Bearer, short-lived `?ticket=`, or legacy `?token=`.
pub async fn auth_middleware(
    State(auth): State<AuthState>,
    req: Request,
    next: Next,
) -> Response {
    if let Some(p) = bearer_from_headers(req.headers()) {
        if ct_eq(&p, auth.token.as_str()) {
            return next.run(req).await;
        }
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    if let Some(ticket) = query_param(req.uri(), "ticket") {
        if auth.tickets.consume(&ticket) {
            return next.run(req).await;
        }
        return (StatusCode::UNAUTHORIZED, "invalid or expired ticket").into_response();
    }
    if let Some(p) = query_param(req.uri(), "token") {
        if ct_eq(&p, auth.token.as_str()) {
            return next.run(req).await;
        }
    }
    (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
}

/// Pull `Bearer <token>` from the Authorization header.
fn bearer_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    let h = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let v = h.strip_prefix("Bearer ")?.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// Pull a named query parameter (no `url` crate dependency).
fn query_param(uri: &Uri, key: &str) -> Option<String> {
    let q = uri.query()?;
    for pair in q.split('&') {
        let mut it = pair.splitn(2, '=');
        if it.next()? == key {
            let v = it.next().unwrap_or("");
            let decoded = percent_decode(v);
            if decoded.is_empty() {
                return None;
            }
            return Some(decoded);
        }
    }
    None
}

/// Minimal percent-decoding for the token query param.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((a << 4) | b);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Constant-time string compare. A length mismatch short-circuits — token
/// length is not secret (it's always 64 hex chars when generated).
fn ct_eq(a: &str, b: &str) -> bool {
    let (ab, bb) = (a.as_bytes(), b.as_bytes());
    if ab.len() != bb.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for i in 0..ab.len() {
        diff |= ab[i] ^ bb[i];
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ct_eq_works() {
        assert!(ct_eq("abc", "abc"));
        assert!(!ct_eq("abc", "abd"));
        assert!(!ct_eq("abc", "abcd"));
        assert!(!ct_eq("abc", "ab"));
        assert!(!ct_eq("", "x"));
        assert!(ct_eq("", ""));
    }

    #[test]
    fn query_token_parsed() {
        let uri: Uri = "ws://h/api/v1/chat?token=abc123".parse().unwrap();
        assert_eq!(query_param(&uri, "token").as_deref(), Some("abc123"));
    }

    #[test]
    fn query_token_among_others() {
        let uri: Uri = "http://h/p?foo=1&token=secret&bar=2".parse().unwrap();
        assert_eq!(query_param(&uri, "token").as_deref(), Some("secret"));
    }

    #[test]
    fn query_token_percent_decoded() {
        let uri: Uri = "http://h/p?token=a%2Bb%3D".parse().unwrap();
        assert_eq!(query_param(&uri, "token").as_deref(), Some("a+b="));
    }

    #[test]
    fn no_query_yields_none() {
        let uri: Uri = "http://h/p".parse().unwrap();
        assert!(query_param(&uri, "token").is_none());
    }

    #[test]
    fn query_ticket_param() {
        let uri: Uri = "ws://h/api/v1/chat?ticket=deadbeef".parse().unwrap();
        assert_eq!(query_param(&uri, "ticket").as_deref(), Some("deadbeef"));
    }
}
