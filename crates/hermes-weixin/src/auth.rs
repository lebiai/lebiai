//! QR-code login flow and persistent token storage.
//!
//! Storage path: `~/.small-rust-hermes/wechat.toml`, mode `0600`. The file
//! contains the `bot_token` which is a long-lived bearer credential — treat
//! it like an API key.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;
use serde::{Deserialize, Serialize};

use crate::client::{Client, DEFAULT_BASE_URL};

/// Persisted creds (file under `~/.small-rust-hermes/wechat.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCreds {
    pub bot_token: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Server-assigned bot account id (informational; helps when
    /// multiple QR-logins exist on the same machine).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<String>,
    /// Server-assigned user (operator) id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

impl StoredCreds {
    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("resolving $HOME")?;
        Ok(home.join(".small-rust-hermes").join("wechat.toml"))
    }

    /// Load credentials from `path`. Returns `Ok(None)` if the file does
    /// not exist (caller should run [`LoginSession::start`] + [`LoginSession::await_confirmation`]).
    pub fn load(path: &Path) -> Result<Option<Self>> {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                let c: Self = toml::from_str(&s)
                    .with_context(|| format!("parsing {}", path.display()))?;
                Ok(Some(c))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Write credentials with mode 0600. Creates parent directory if
    /// missing.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let s = toml::to_string(self).context("serializing creds")?;
        std::fs::write(path, s).with_context(|| format!("writing {}", path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(path)?.permissions();
            perm.set_mode(0o600);
            std::fs::set_permissions(path, perm)?;
        }
        Ok(())
    }
}

/// Status of a poll iteration during QR confirmation.
#[derive(Debug, Clone)]
pub enum QrPollState {
    /// QR fetched but no scan yet.
    Waiting,
    /// User has scanned but not confirmed.
    Scanned,
    /// Done — credentials persisted by caller.
    Confirmed(StoredCreds),
    /// Previous QR expired; a fresh one has been fetched and is now active.
    /// The string is the freshly-rendered terminal QR for caller to re-print.
    Refreshed(String),
}

/// One QR-login session in progress.
pub struct LoginSession {
    /// Opaque session token used as the polling key. **Not** scannable.
    pub qrcode: String,
    /// Payload that actually goes into the scannable QR (a URL). Encode
    /// this — not [`Self::qrcode`] — into the QR you show the user.
    qr_payload: String,
    client: Client,
    base_url: String,
}

impl LoginSession {
    /// Fetch a QR token from the server.
    pub async fn start(base_url: &str) -> Result<Self> {
        let client = Client::new(base_url.to_string())?;
        let resp = client.get_bot_qrcode().await.context("fetching QR code")?;
        let qr_payload = resp
            .qrcode_img_content
            .ok_or_else(|| anyhow!("server did not return qrcode_img_content"))?;
        Ok(Self {
            qrcode: resp.qrcode,
            qr_payload,
            client,
            base_url: base_url.to_string(),
        })
    }

    /// Render the scannable payload as a terminal QR (unicode half-blocks).
    /// Print the result to stdout for the user to scan with WeChat.
    pub fn render_terminal(&self) -> Result<String> {
        let code = QrCode::new(self.qr_payload.as_bytes())
            .context("encoding qrcode payload")?;
        Ok(code
            .render::<Dense1x2>()
            .dark_color(Dense1x2::Light)
            .light_color(Dense1x2::Dark)
            .quiet_zone(true)
            .build())
    }

    /// Re-fetch a fresh QR (used internally after the current one expires).
    async fn refresh(&mut self) -> Result<()> {
        let resp = self
            .client
            .get_bot_qrcode()
            .await
            .context("refreshing QR code")?;
        self.qrcode = resp.qrcode;
        self.qr_payload = resp
            .qrcode_img_content
            .ok_or_else(|| anyhow!("server did not return qrcode_img_content on refresh"))?;
        Ok(())
    }

    /// Poll the server once. Caller should sleep between calls.
    pub async fn poll(&self) -> Result<QrPollState> {
        let resp = self
            .client
            .get_qrcode_status(&self.qrcode)
            .await
            .context("polling QR status")?;
        // Status strings match the official `@tencent-weixin/openclaw-weixin`
        // server contract: `wait`, `scaned` (sic), `confirmed`, `expired`.
        match resp.status.as_str() {
            "confirmed" => {
                let token = resp
                    .bot_token
                    .ok_or_else(|| anyhow!("confirmed but no bot_token in response"))?;
                let base = resp.baseurl.unwrap_or_else(|| self.base_url.clone());
                Ok(QrPollState::Confirmed(StoredCreds {
                    bot_token: token,
                    base_url: base,
                    bot_id: resp.ilink_bot_id,
                    user_id: resp.ilink_user_id,
                }))
            }
            "expired" => Err(ExpiredSignal.into()),
            "scaned" => Ok(QrPollState::Scanned),
            // "wait" (and any other transient state we don't recognize) → keep waiting
            _ => Ok(QrPollState::Waiting),
        }
    }

    /// Poll loop with automatic QR refresh on expiry. The `on_state`
    /// callback is invoked once per state change so callers can update UI
    /// (e.g. print `Refreshed(qr)` directly to stdout).
    ///
    /// The session is mutated in place when expired QRs are replaced.
    /// Gives up after `max_refreshes` expirations.
    pub async fn await_confirmation<F>(
        &mut self,
        max_refreshes: u32,
        mut on_state: F,
    ) -> Result<StoredCreds>
    where
        F: FnMut(&QrPollState),
    {
        let mut refreshes = 0;
        loop {
            match self.poll().await {
                Ok(state) => {
                    on_state(&state);
                    if let QrPollState::Confirmed(c) = state {
                        return Ok(c);
                    }
                }
                Err(e) if e.is::<ExpiredSignal>() => {
                    if refreshes >= max_refreshes {
                        bail!(
                            "QR code expired and the refresh limit ({max_refreshes}) was reached"
                        );
                    }
                    refreshes += 1;
                    self.refresh().await?;
                    let rendered = self.render_terminal()?;
                    on_state(&QrPollState::Refreshed(rendered));
                }
                Err(e) => return Err(e),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

/// Internal sentinel for "the server says this QR is expired"; not
/// surfaced to callers — `await_confirmation` translates it into a refresh.
#[derive(Debug)]
struct ExpiredSignal;

impl std::fmt::Display for ExpiredSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QR code expired")
    }
}

impl std::error::Error for ExpiredSignal {}
