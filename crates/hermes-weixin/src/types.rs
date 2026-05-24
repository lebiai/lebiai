//! Wire types for the WeChat iLink Bot HTTP/JSON protocol.
//!
//! Field names match the upstream API; only fields the crate uses are
//! deserialized. Unknown fields are ignored.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Channel version we advertise to the server. Matches the value used by
/// the official `@tencent-weixin/openclaw-weixin` plugin.
pub const CHANNEL_VERSION: &str = "1.0.2";

/// Discriminator for `item_list[].type` in WeChat messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    Text = 1,
    Image = 2,
    Voice = 3,
    File = 4,
    Video = 5,
}

/// Outer envelope used by `/ilink/bot/*` POST endpoints — `ret == 0` means success.
#[derive(Debug, Deserialize)]
pub struct ApiEnvelope<T> {
    #[serde(default)]
    pub ret: i32,
    #[serde(default)]
    pub err_msg: Option<String>,
    #[serde(flatten)]
    pub data: T,
}

// --- QR login -----------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct GetQrCodeResp {
    pub qrcode: String,
    /// Server-side payload to encode into the actual scannable QR. Despite
    /// the field name this is **not** a base64 image — it's an opaque
    /// string URL we feed into a QR encoder ourselves and render in the
    /// terminal.
    #[serde(default)]
    pub qrcode_img_content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QrCodeStatusResp {
    /// "wait" | "scaned" | "confirmed" | "expired" (server-defined; note
    /// the unusual spellings).
    pub status: String,
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub baseurl: Option<String>,
    #[serde(default)]
    pub ilink_bot_id: Option<String>,
    #[serde(default)]
    pub ilink_user_id: Option<String>,
}

// --- Long-poll: getupdates ---------------------------------------------

/// Inbound long-poll request. `base_info` is injected centrally by the
/// client so callers only supply the cursor.
#[derive(Debug, Serialize)]
pub struct GetUpdatesReq {
    pub get_updates_buf: String,
}

#[derive(Debug, Deserialize)]
pub struct GetUpdatesResp {
    #[serde(default)]
    pub msgs: Vec<WeixinMessage>,
    #[serde(default)]
    pub get_updates_buf: String,
    #[serde(default)]
    pub longpolling_timeout_ms: u64,
}

// --- Message schema -----------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeixinMessage {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub from_user_id: String,
    #[serde(default)]
    pub to_user_id: String,
    /// Outbound dedupe key. Format used by the official client: `wcb-{uuid v4}`.
    /// Empty on inbound; required on outbound (the server uses it as an
    /// idempotency token).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_id: String,
    #[serde(default)]
    pub message_type: i32,
    #[serde(default)]
    pub message_state: i32,
    /// Opaque thread-binding token; MUST be echoed back on reply or the
    /// message will not bind to the inbound conversation.
    #[serde(default)]
    pub context_token: String,
    #[serde(default)]
    pub item_list: Vec<MessageItem>,
}

impl WeixinMessage {
    /// First text item's text, if the message is plain-text. Returns `None`
    /// for non-text messages (image, voice, file, video) or empty payloads.
    pub fn first_text(&self) -> Option<&str> {
        self.item_list.iter().find_map(|i| match &i.payload {
            ItemPayload::Text { text_item } => Some(text_item.text.as_str()),
            _ => None,
        })
    }

    /// Build an outbound text reply for an inbound message. Enforces the
    /// outbound-only schema:
    ///   - `from_user_id` cleared (server infers from token; including it
    ///     causes silent-success non-delivery)
    ///   - `client_id = "wcb-{uuid}"` (idempotency token; missing causes
    ///     silent drops on the server side)
    ///   - `message_type = 2` (BOT 发出)
    ///   - `message_state = 2` (FINISH — complete message)
    ///   - `context_token` echoed verbatim from the inbound message (required)
    pub fn reply_text(inbound: &WeixinMessage, text: impl Into<String>) -> Self {
        Self {
            from_user_id: String::new(),
            to_user_id: inbound.from_user_id.clone(),
            client_id: format!("wcb-{}", Uuid::new_v4()),
            message_type: 2,
            message_state: 2,
            context_token: inbound.context_token.clone(),
            item_list: vec![MessageItem::text(text)],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageItem {
    #[serde(rename = "type")]
    pub item_type: i32,
    #[serde(flatten)]
    pub payload: ItemPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ItemPayload {
    Text {
        text_item: TextItem,
    },
    /// Catch-all so unknown / non-text items don't fail deserialization.
    Other(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    pub text: String,
}

impl MessageItem {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            item_type: ItemType::Text as i32,
            payload: ItemPayload::Text {
                text_item: TextItem { text: s.into() },
            },
        }
    }
}

// --- sendmessage --------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SendMessageReq {
    pub msg: WeixinMessage,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageResp {
    /// Server-assigned id (if returned). Not all responses include this.
    #[serde(default)]
    pub msg_id: Option<String>,
}
