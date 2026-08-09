//! WeChat (iLink Bot) connection for the GUI surface.
//!
//! Thin Tauri seam only: QR login (start / poll) and start / stop of the
//! shared long-poll serve loop. All engine logic lives in `hermes-weixin`
//! (protocol + `service::serve`) and `hermes-channel` (per-user session
//! driver) — the GUI never forks a parallel loop.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use hermes_channel::handle_text_message;
use hermes_weixin::auth::{ExpiredSignal, LoginSession, QrPollState, StoredCreds};
use hermes_weixin::client::Client as WxClient;
use hermes_weixin::types::WeixinMessage;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::error::GuiError;
use crate::state::{AppState, WechatStatus};

// ===== login =================================================================

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WechatLoginView {
    /// QR module matrix (`true` = dark) for the frontend canvas.
    pub matrix: Vec<Vec<bool>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WechatPollView {
    /// "waiting" | "scanned" | "refreshed" | "confirmed"
    pub status: String,
    /// Fresh QR matrix after an expiry refresh (else `None`).
    pub matrix: Option<Vec<Vec<bool>>>,
    /// bot_id after confirmation (frontend then calls `wechat_start`).
    pub bot_id: Option<String>,
}

/// Start a QR login session; returns the scannable matrix once.
#[tauri::command]
pub async fn wechat_login_start(state: State<'_, AppState>) -> Result<WechatLoginView, GuiError> {
    let session = LoginSession::start(hermes_weixin::DEFAULT_BASE_URL).await?;
    let matrix = session.matrix()?;
    *state.wechat.login.lock().await = Some(session);
    Ok(WechatLoginView { matrix })
}

/// Poll the login session once. Auto-refreshes an expired QR (returns a new
/// matrix) and persists credentials on confirmation.
#[tauri::command]
pub async fn wechat_login_poll(state: State<'_, AppState>) -> Result<WechatPollView, GuiError> {
    let mut guard = state.wechat.login.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| GuiError::Internal("没有进行中的扫码登录，请重新开始。".to_string()))?;
    match session.poll().await {
        Ok(QrPollState::Waiting) => Ok(WechatPollView {
            status: "waiting".into(),
            matrix: None,
            bot_id: None,
        }),
        Ok(QrPollState::Scanned) => Ok(WechatPollView {
            status: "scanned".into(),
            matrix: None,
            bot_id: None,
        }),
        // `Refreshed` is only produced by `await_confirmation`'s internal
        // loop; `poll()` returns `ExpiredSignal` instead, handled below.
        Ok(QrPollState::Refreshed(_)) => Ok(WechatPollView {
            status: "waiting".into(),
            matrix: None,
            bot_id: None,
        }),
        Ok(QrPollState::Confirmed(creds)) => {
            let path = StoredCreds::default_path()?;
            creds.save(&path)?;
            let bot_id = creds.bot_id.clone();
            *guard = None;
            Ok(WechatPollView {
                status: "confirmed".into(),
                matrix: None,
                bot_id,
            })
        }
        Err(e) if e.is::<ExpiredSignal>() => {
            session.refresh().await?;
            let matrix = session.matrix()?;
            Ok(WechatPollView {
                status: "refreshed".into(),
                matrix: Some(matrix),
                bot_id: None,
            })
        }
        Err(e) => Err(e.into()),
    }
}

// ===== serve loop ============================================================

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WechatStatusView {
    /// "stopped" | "listening" | "token_expired" | "error"
    pub state: String,
    pub bot_id: Option<String>,
    pub last_error: Option<String>,
    /// Whether a verified credential file exists on disk.
    pub logged_in: bool,
    /// Whether the serve loop is currently running.
    pub listening: bool,
}

impl From<&WechatStatus> for WechatStatusView {
    fn from(s: &WechatStatus) -> Self {
        Self {
            state: s.state.clone(),
            bot_id: s.bot_id.clone(),
            last_error: s.last_error.clone(),
            logged_in: true,
            listening: false,
        }
    }
}

async fn status_view(state: &AppState) -> Result<WechatStatusView, GuiError> {
    let logged_in = StoredCreds::load(&StoredCreds::default_path()?)?.is_some();
    let listening = state.wechat.serve_task.lock().await.is_some();
    let mut view = WechatStatusView::from(&*state.wechat.status.lock().await);
    view.logged_in = logged_in;
    view.listening = listening;
    Ok(view)
}

#[tauri::command]
pub async fn wechat_status(state: State<'_, AppState>) -> Result<WechatStatusView, GuiError> {
    status_view(&state).await
}

/// Start the shared long-poll serve loop as a background task. Idempotent:
/// returns the current status when already listening.
#[tauri::command]
pub async fn wechat_start(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WechatStatusView, GuiError> {
    if state.wechat.serve_task.lock().await.is_some() {
        return status_view(&state).await;
    }
    let creds_path = StoredCreds::default_path()?;
    let creds = StoredCreds::load(&creds_path)?
        .ok_or_else(|| GuiError::Internal("请先扫码登录微信。".to_string()))?;
    let wx = WxClient::with_token(creds.base_url.clone(), creds.bot_token.clone())?;
    let ctx = state.build_serve_ctx().await?;

    state.wechat.shutdown.store(false, Ordering::SeqCst);
    let users = Arc::clone(&state.wechat.serve_users);
    let shutdown = Arc::clone(&state.wechat.shutdown);
    let status = Arc::clone(&state.wechat.status);
    let serve_task = Arc::clone(&state.wechat.serve_task);
    let app_emit = app.clone();
    let bot_id = creds.bot_id.clone();
    let wx_loop = wx.clone();
    let task = tokio::spawn(async move {
        *status.lock().await = WechatStatus {
            state: "listening".to_string(),
            bot_id: bot_id.clone(),
            last_error: None,
        };
        let _ = app_emit.emit("wechat-status", status.lock().await.clone());

        let on_message = move |inbound: WeixinMessage, text: String| {
            let ctx = ctx.clone();
            let wx = wx_loop.clone();
            let users = users.clone();
            async move {
                let from = inbound.from_user_id.clone();
                let mut guard = users.lock().await;
                handle_text_message(ctx.as_ref(), &wx, &mut guard, &from, text, inbound).await
            }
        };
        let result = hermes_weixin::service::serve(&wx, shutdown, on_message).await;
        let (state_str, last_error) = match result {
            Ok(()) => ("stopped".to_string(), None),
            Err(e) => {
                let msg = format!("{e:#}");
                if msg.contains("token 已失效") {
                    ("token_expired".to_string(), Some(msg))
                } else {
                    ("error".to_string(), Some(msg))
                }
            }
        };
        *status.lock().await = WechatStatus {
            state: state_str,
            bot_id,
            last_error,
        };
        let _ = app_emit.emit("wechat-status", status.lock().await.clone());
        *serve_task.lock().await = None;
    });
    *state.wechat.serve_task.lock().await = Some(task);
    status_view(&state).await
}

/// Stop the serve loop. Returns immediately with a `stopping` status; the
/// background task transitions to `stopped` (or token_expired/error) once
/// the poll loop exits, then emits the final status.
#[tauri::command]
pub async fn wechat_stop(app: AppHandle, state: State<'_, AppState>) -> Result<(), GuiError> {
    state.wechat.shutdown.store(true, Ordering::SeqCst);
    if state.wechat.serve_task.lock().await.is_some() {
        let bot_id = state.wechat.status.lock().await.bot_id.clone();
        *state.wechat.status.lock().await = WechatStatus {
            state: "stopping".to_string(),
            bot_id,
            last_error: None,
        };
        let _ = app.emit("wechat-status", state.wechat.status.lock().await.clone());
    }
    Ok(())
}

/// Stop the serve loop and remove the stored credential (full disconnect).
#[tauri::command]
pub async fn wechat_logout(app: AppHandle, state: State<'_, AppState>) -> Result<(), GuiError> {
    wechat_stop(app, state).await?;
    let path = StoredCreds::default_path()?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| GuiError::Internal(format!("{e}")))?;
    }
    Ok(())
}
