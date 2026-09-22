//! Software license (signed token) + trial window for desktop delivery.
//!
//! Format: `LEBI1.<base64url(payload_json)>.<base64url(ed25519_sig)>`
//! Payload fields: `product`, `exp` (unix secs), optional `iat`, `lic_id`, `plan`, `personas`.
//!
//! `personas` = the persona roster this license unlocks (`docs/spec/personas.md`).
//! Absent field = legacy token = built-ins only; unknown ids are reported, never silently
//! dropped. No `deny_unknown_fields` here — a client must always be able to read a newer
//! token than itself (spec §4.6).
//!
//! Spec: `docs/spec/license-ux.md`. Private key never ships in the client.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::paths::trial_anchor_path;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use ed25519_dalek::{
    Signature, SigningKey, Verifier, VerifyingKey, PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Product id embedded in every token.
pub const PRODUCT_ID: &str = "lebi-ai";
/// Trial length from first launch.
pub const TRIAL_DAYS: i64 = 3;
/// Days remaining at or below this → expiring urgency + daily nudge.
pub const EXPIRING_DAYS: i64 = 3;
/// Accept small clock skew when comparing `exp`.
pub const CLOCK_SKEW_SECS: i64 = 300;
/// Purchase contact (product copy).
pub const WECHAT_CONTACT: &str = "iodine001";
/// Token prefix.
pub const TOKEN_PREFIX: &str = "LEBI1";

/// 出厂验签公钥（Ed25519，32 字节）。
///
/// **私钥不在这份源码里、也不在任何提交里**：它只存在于签发机的
/// `~/.lebi-ai-issuer/seed.hex`（0600）。`docs/records/20260918-reaudit.md` P0-1
/// 记录过一次事故——私钥曾经与这份公钥一起躺在仓库里，等于谁都能自签授权码。
/// 换锁 = 生成新密钥对 + 只把公钥写到这里 + 让老码失效重发。
const PUBLIC_KEY_BYTES: [u8; PUBLIC_KEY_LENGTH] = [
    0x2e, 0xd6, 0xd2, 0xd6, 0x6a, 0x38, 0x9f, 0x2f, 0x2f, 0x65, 0x94, 0x77, 0x3b, 0xea, 0xc8, 0x31,
    0xfb, 0x09, 0x74, 0xe8, 0x46, 0x49, 0xe6, 0x03, 0xbe, 0x68, 0xb3, 0xc4, 0x84, 0xd7, 0x7b, 0x94,
];

#[derive(Debug, Error)]
pub enum LicenseError {
    #[error("invalid license format")]
    InvalidFormat,
    #[error("invalid license signature")]
    BadSignature,
    #[error("license is not for this product")]
    WrongProduct,
    #[error("license has expired")]
    Expired,
    #[error("this license ends earlier than your current one; not applied")]
    OlderThanCurrent,
    #[error("this is already your current license")]
    SameAsCurrent,
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("parse: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicensePhase {
    /// In trial, main features allowed.
    Trial,
    /// Valid signed license.
    Licensed,
    /// Trial over / license expired / invalid with no trial left.
    Locked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseUrgency {
    Ample,
    Expiring,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    pub phase: LicensePhase,
    pub urgency: LicenseUrgency,
    /// Main product surfaces (dialogue, etc.).
    pub can_use_main: bool,
    pub show_full_lock: bool,
    /// True when expiring and not yet nudged today (caller may show modal).
    pub should_nudge: bool,
    /// Effective end of access (trial end or license exp).
    pub expires_at: Option<String>,
    pub remaining_secs: i64,
    /// 0.0–1.0 for battery UI (vs nominal window).
    pub remaining_ratio: f64,
    /// True when access comes from trial (no active license).
    pub on_trial: bool,
    pub wechat: String,
    pub lic_id: Option<String>,
    pub plan: Option<String>,
    /// Personas this machine may show, on top of the built-ins (`docs/spec/personas.md`).
    /// Trial → empty. Licensed **and** Locked → the last valid token's roster: expiry locks
    /// the main surfaces, it never takes a persona away.
    pub personas: Vec<String>,
    /// Ids the token named that this build does not know. Sorted + deduped, never silently
    /// swallowed — the UI can say "this license names a persona your app doesn't have yet".
    pub unknown_personas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LicensePayload {
    product: String,
    /// Unix timestamp (seconds).
    exp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    iat: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lic_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plan: Option<String>,
    /// Persona ids unlocked by this license. Omitted = legacy token = built-ins only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    personas: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LicenseFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trial_started_at: Option<String>,
    /// Local calendar date `YYYY-MM-DD` of last renew nudge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_nudge_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_seen_unix: Option<i64>,
}

/// Decoded + verified token (not necessarily unexpired).
#[derive(Debug, Clone)]
pub struct VerifiedLicense {
    pub token: String,
    pub exp_unix: i64,
    pub iat_unix: Option<i64>,
    pub lic_id: Option<String>,
    pub plan: Option<String>,
    /// Exactly what the token says (unfiltered); [`build_status`] narrows it to this build's
    /// roster. Empty for legacy tokens.
    pub personas: Vec<String>,
}

/// 出厂验签公钥。生产路径只认这一把；测试注入自己的公钥（见 `*_with_key`）。
pub fn shipped_verifying_key() -> VerifyingKey {
    VerifyingKey::from_bytes(&PUBLIC_KEY_BYTES).expect("hardcoded public key")
}

fn b64_encode(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, LicenseError> {
    URL_SAFE_NO_PAD
        .decode(s.trim().as_bytes())
        .map_err(|_| LicenseError::InvalidFormat)
}

/// 用**调用方自备**的私钥签一张码（测试与签发工具用）。
///
/// 库里没有「拿着某个内置 seed 去签名」的入口——那正是事故的形状。
pub fn sign_token_with_key(
    sk: &SigningKey,
    exp_unix: i64,
    iat_unix: Option<i64>,
    lic_id: Option<String>,
    plan: Option<String>,
    personas: Option<Vec<String>>,
) -> Result<String, LicenseError> {
    use ed25519_dalek::Signer;
    let payload = LicensePayload {
        product: PRODUCT_ID.to_string(),
        exp: exp_unix,
        iat: iat_unix,
        lic_id,
        plan,
        personas,
    };
    let json = serde_json::to_vec(&payload).map_err(|e| LicenseError::Parse(e.to_string()))?;
    let sig = sk.sign(&json);
    Ok(format!(
        "{TOKEN_PREFIX}.{}.{}",
        b64_encode(&json),
        b64_encode(sig.to_bytes().as_ref())
    ))
}

/// Verify signature and product; does **not** check expiry.
pub fn verify_token(token: &str) -> Result<VerifiedLicense, LicenseError> {
    verify_token_with_key(token, &shipped_verifying_key())
}

/// Same as [`verify_token`], but against an injected public key (tests / fleet rotation).
pub fn verify_token_with_key(
    token: &str,
    key: &VerifyingKey,
) -> Result<VerifiedLicense, LicenseError> {
    let token = token.trim();
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 || parts[0] != TOKEN_PREFIX {
        return Err(LicenseError::InvalidFormat);
    }
    let json = b64_decode(parts[1])?;
    let sig_bytes = b64_decode(parts[2])?;
    if sig_bytes.len() != SIGNATURE_LENGTH {
        return Err(LicenseError::InvalidFormat);
    }
    let mut sig_arr = [0u8; SIGNATURE_LENGTH];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = Signature::from_bytes(&sig_arr);
    key.verify(&json, &sig)
        .map_err(|_| LicenseError::BadSignature)?;
    let payload: LicensePayload =
        serde_json::from_slice(&json).map_err(|e| LicenseError::Parse(e.to_string()))?;
    if payload.product != PRODUCT_ID {
        return Err(LicenseError::WrongProduct);
    }
    Ok(VerifiedLicense {
        token: token.to_string(),
        exp_unix: payload.exp,
        iat_unix: payload.iat,
        lic_id: payload.lic_id,
        plan: payload.plan,
        personas: payload.personas.unwrap_or_default(),
    })
}

fn now_unix() -> i64 {
    Utc::now().timestamp()
}

fn is_unexpired(exp_unix: i64, now: i64) -> bool {
    now < exp_unix + CLOCK_SKEW_SECS
}

fn local_date_string() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

// ── 试用起点：锚在数据根外 + 时钟只许向前 ────────────────────────────────
//
// 两件事都为一个目的：**试用不能靠删文件或改钟重新开始**（P0-5）。
// 1. 锚文件住在数据根**之外**（`paths::trial_anchor_path`），记下最早一次试用起点；
//    数据根里的 license.json 说什么都不能比它更晚。
// 2. 判定用的「现在」取「墙上时间」与「上次见到的时间」中较晚的那个 —— 把钟
//    往回拨不会多出任何时间。

/// 判定用的「现在」：时钟回拨不买时间。
fn effective_now(file: &LicenseFile, now: i64) -> i64 {
    file.last_seen_unix.map(|t| t.max(now)).unwrap_or(now)
}

/// 读锚（RFC3339 一行纯文本；读不动就当没有 —— 不因为锚坏了把人锁在门外）。
fn read_trial_anchor(anchor: &Path) -> Option<String> {
    let raw = fs::read_to_string(anchor).ok()?;
    let stamp = raw.trim();
    if stamp.is_empty() {
        return None;
    }
    parse_rfc3339(stamp)?;
    Some(stamp.to_string())
}

fn write_trial_anchor(anchor: &Path, stamp: &str) -> Result<(), LicenseError> {
    if let Some(parent) = anchor.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(anchor, format!("{stamp}\n"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(anchor, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// 把 `file` 的试用起点夹到锚上。返回是否需要落盘。
///
/// - 锚不在 → 首次运行：把当前起点（或现在）记进锚。`write=false` 时只读不建
///   （热路径上不许写盘）。
/// - 锚在 → 取更早的那个：数据根被删/换过之后，新提出的起点会被夹回去。
fn anchor_trial_start(file: &mut LicenseFile, anchor: Option<&Path>, write: bool) -> bool {
    let Some(anchor) = anchor else {
        return false;
    };
    match read_trial_anchor(anchor) {
        Some(stamped) => {
            let anchored = parse_rfc3339(&stamped);
            let claim = file.trial_started_at.as_deref().and_then(parse_rfc3339);
            let keep_anchor = match (anchored, claim) {
                (Some(a), Some(c)) => c > a,
                (Some(_), None) => true,
                _ => false,
            };
            if keep_anchor {
                file.trial_started_at = Some(stamped);
                return true;
            }
            false
        }
        None => {
            if !write {
                return false;
            }
            let stamp = file
                .trial_started_at
                .clone()
                .unwrap_or_else(|| Utc::now().to_rfc3339());
            let _ = write_trial_anchor(anchor, &stamp);
            false
        }
    }
}

/// 本机授权状态文件（产品数据根下的 `license.json`）。
///
/// GUI / CLI 要落码或读状态时用它；路径本身可注入的部分走各 `*_at` 变体，
/// 只有「默认装在哪」这一处在测试里不方便注入。
pub fn license_file_path() -> PathBuf {
    crate::paths::data_path("license.json")
}

fn load_file(path: &Path) -> Result<LicenseFile, LicenseError> {
    if !path.exists() {
        return Ok(LicenseFile::default());
    }
    let raw = fs::read_to_string(path)?;
    serde_json::from_str(&raw).map_err(|e| LicenseError::Parse(e.to_string()))
}

fn save_file(path: &Path, file: &LicenseFile) -> Result<(), LicenseError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(file).map_err(|e| LicenseError::Parse(e.to_string()))?;
    fs::write(path, raw)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn ensure_trial_started(file: &mut LicenseFile) -> bool {
    if file.trial_started_at.is_some() {
        return false;
    }
    file.trial_started_at = Some(Utc::now().to_rfc3339());
    true
}

fn trial_end_unix(file: &LicenseFile) -> Option<i64> {
    let started = file.trial_started_at.as_ref()?;
    let dt = parse_rfc3339(started)?;
    Some((dt + Duration::days(TRIAL_DAYS)).timestamp())
}

fn active_verified_with_key(
    file: &LicenseFile,
    now: i64,
    key: &VerifyingKey,
) -> Option<VerifiedLicense> {
    let token = file.token.as_ref()?;
    let v = verify_token_with_key(token, key).ok()?;
    if is_unexpired(v.exp_unix, now) {
        Some(v)
    } else {
        None
    }
}

fn fmt_unix(t: i64) -> String {
    Utc.timestamp_opt(t, 0)
        .single()
        .map(|d| d.to_rfc3339())
        .unwrap_or_default()
}

/// Token roster → this build's roster: keep what we know (deduped, token order),
/// report what we don't (sorted, deduped). Never silently drops an id.
fn split_personas(raw: &[String]) -> (Vec<String>, Vec<String>) {
    let mut known: Vec<String> = Vec::new();
    let mut unknown: BTreeSet<String> = BTreeSet::new();
    for id in raw {
        if crate::persona::get(id).is_none() {
            unknown.insert(id.clone());
        } else if !known.contains(id) {
            known.push(id.clone());
        }
    }
    (known, unknown.into_iter().collect())
}

fn build_status_with_key(file: &LicenseFile, now: i64, key: &VerifyingKey) -> LicenseStatus {
    let today = local_date_string();
    let nudged_today = file.last_nudge_date.as_deref() == Some(today.as_str());

    if let Some(v) = active_verified_with_key(file, now, key) {
        let remaining = (v.exp_unix - now).max(0);
        let window = match v.iat_unix {
            Some(iat) if v.exp_unix > iat => (v.exp_unix - iat) as f64,
            _ => (EXPIRING_DAYS * 86400 * 4) as f64, // fallback ~12d scale
        };
        let ratio = if window > 0.0 {
            (remaining as f64 / window).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let expiring = remaining <= EXPIRING_DAYS * 86400;
        let urgency = if expiring {
            LicenseUrgency::Expiring
        } else {
            LicenseUrgency::Ample
        };
        let (personas, unknown_personas) = split_personas(&v.personas);
        return LicenseStatus {
            phase: LicensePhase::Licensed,
            urgency,
            can_use_main: true,
            show_full_lock: false,
            should_nudge: expiring && !nudged_today,
            expires_at: Some(fmt_unix(v.exp_unix)),
            remaining_secs: remaining,
            remaining_ratio: ratio,
            on_trial: false,
            wechat: WECHAT_CONTACT.to_string(),
            lic_id: v.lic_id,
            plan: v.plan,
            personas,
            unknown_personas,
        };
    }

    // No valid license — trial?
    let trial_end = trial_end_unix(file);
    if let Some(end) = trial_end {
        if now < end + CLOCK_SKEW_SECS {
            let remaining = (end - now).max(0);
            let window = (TRIAL_DAYS * 86400) as f64;
            let ratio = (remaining as f64 / window).clamp(0.0, 1.0);
            let expiring = remaining <= EXPIRING_DAYS * 86400;
            // Trial is exactly 3 days: last 3 days means essentially whole trial is "expiring"
            // Spec: trial last 3 days use same nudge — for 3-day trial, always expiring urgency
            // after day 0. Actually "最后 3 天" for a 3-day trial = entire trial period.
            // That would nudge every day of trial which is OK per user confirmation.
            let urgency = if expiring {
                LicenseUrgency::Expiring
            } else {
                LicenseUrgency::Ample
            };
            return LicenseStatus {
                phase: LicensePhase::Trial,
                urgency,
                can_use_main: true,
                show_full_lock: false,
                should_nudge: expiring && !nudged_today,
                expires_at: Some(fmt_unix(end)),
                remaining_secs: remaining,
                remaining_ratio: ratio,
                on_trial: true,
                wechat: WECHAT_CONTACT.to_string(),
                lic_id: None,
                plan: None,
                // Trial shows the built-ins only — a trial is not a roster.
                personas: Vec::new(),
                unknown_personas: Vec::new(),
            };
        }
    }

    // Locked — the last valid token still says who works here. Expiry locks the main
    // surfaces; it does not take personas away (spec: 过期人物不消失).
    let last = file
        .token
        .as_ref()
        .and_then(|t| verify_token_with_key(t, key).ok());
    let (personas, unknown_personas) = last
        .as_ref()
        .map(|v| split_personas(&v.personas))
        .unwrap_or_default();
    let expires_at = last
        .as_ref()
        .map(|v| fmt_unix(v.exp_unix))
        .or_else(|| trial_end.map(fmt_unix));

    LicenseStatus {
        phase: LicensePhase::Locked,
        urgency: LicenseUrgency::Expired,
        can_use_main: false,
        show_full_lock: true,
        should_nudge: false,
        expires_at,
        remaining_secs: 0,
        remaining_ratio: 0.0,
        on_trial: false,
        wechat: WECHAT_CONTACT.to_string(),
        lic_id: None,
        plan: None,
        personas,
        unknown_personas,
    }
}

/// Load (or init trial), update last_seen, return status.
///
/// 走的是**真实路径**：试用起点会被数据根外的锚夹一次（[`anchor_trial_start`]）。
/// 测试用 [`load_status_at_with_key`]（不带锚），免得碰真实机器上的锚文件。
pub fn load_status() -> Result<LicenseStatus, LicenseError> {
    load_status_full(
        &license_file_path(),
        Some(&trial_anchor_path()),
        &shipped_verifying_key(),
    )
}

pub fn load_status_at(path: &Path) -> Result<LicenseStatus, LicenseError> {
    load_status_at_with_key(path, &shipped_verifying_key())
}

pub fn load_status_at_with_key(
    path: &Path,
    key: &VerifyingKey,
) -> Result<LicenseStatus, LicenseError> {
    load_status_full(path, None, key)
}

fn load_status_full(
    path: &Path,
    anchor: Option<&Path>,
    key: &VerifyingKey,
) -> Result<LicenseStatus, LicenseError> {
    let mut file = load_file(path)?;
    let mut dirty = ensure_trial_started(&mut file);
    let now = now_unix();
    // Soft clock tracking
    if file
        .last_seen_unix
        .map(|t| now + 86400 < t)
        .unwrap_or(false)
    {
        tracing::warn!("system clock appears to have moved backwards significantly");
    }
    dirty |= anchor_trial_start(&mut file, anchor, true);
    let now = effective_now(&file, now);
    if file.last_seen_unix != Some(now) {
        file.last_seen_unix = Some(now);
        dirty = true;
    }
    if dirty {
        save_file(path, &file)?;
    }
    Ok(build_status_with_key(&file, now, key))
}

/// Apply a new token. Same path for activate and renew.
pub fn apply_token(raw: &str) -> Result<LicenseStatus, LicenseError> {
    apply_token_full(
        &license_file_path(),
        Some(&trial_anchor_path()),
        raw,
        &shipped_verifying_key(),
    )
}

pub fn apply_token_at_with_key(
    path: &Path,
    raw: &str,
    key: &VerifyingKey,
) -> Result<LicenseStatus, LicenseError> {
    apply_token_full(path, None, raw, key)
}

fn apply_token_full(
    path: &Path,
    anchor: Option<&Path>,
    raw: &str,
    key: &VerifyingKey,
) -> Result<LicenseStatus, LicenseError> {
    let raw = raw.trim();
    let verified = verify_token_with_key(raw, key)?;
    let now = now_unix();
    if !is_unexpired(verified.exp_unix, now) {
        return Err(LicenseError::Expired);
    }

    let mut file = load_file(path)?;
    ensure_trial_started(&mut file);
    anchor_trial_start(&mut file, anchor, true);

    if let Some(cur) = file.token.as_ref() {
        if cur.trim() == raw {
            return Err(LicenseError::SameAsCurrent);
        }
        if let Ok(cur_v) = verify_token_with_key(cur, key) {
            if is_unexpired(cur_v.exp_unix, now) && verified.exp_unix < cur_v.exp_unix {
                return Err(LicenseError::OlderThanCurrent);
            }
        }
    }

    file.token = Some(verified.token);
    let now = effective_now(&file, now);
    file.last_seen_unix = Some(now);
    // Clear nudge so a new cycle can remind later if needed
    file.last_nudge_date = None;
    save_file(path, &file)?;
    Ok(build_status_with_key(&file, now, key))
}

/// Mark daily renew nudge as shown for local today.
pub fn mark_nudge_seen() -> Result<LicenseStatus, LicenseError> {
    mark_nudge_seen_at(&license_file_path())
}

pub fn mark_nudge_seen_at(path: &Path) -> Result<LicenseStatus, LicenseError> {
    let mut file = load_file(path)?;
    ensure_trial_started(&mut file);
    file.last_nudge_date = Some(local_date_string());
    let now = effective_now(&file, now_unix());
    file.last_seen_unix = Some(now);
    save_file(path, &file)?;
    Ok(build_status_with_key(&file, now, &shipped_verifying_key()))
}

/// Whether main features may run (dialogue).
pub fn can_use_main() -> bool {
    load_status().map(|s| s.can_use_main).unwrap_or(false)
}

/// Error token every gate raises when the main surfaces are locked. One string,
/// so the GUI banner, the provider decorator and the IM relays cannot drift.
pub const LOCKED_MESSAGE: &str = "license_locked";

/// [`can_use_main`] without touching the disk: never creates the trial file and
/// never refreshes `last_seen`.
///
/// The gate sits on the request hot path (every model call), so it must not
/// write. A missing file means "trial not started yet" — the same yes that
/// [`load_status`] would give while starting it.
pub fn can_use_main_readonly() -> bool {
    can_use_main_readonly_full(
        &license_file_path(),
        Some(&trial_anchor_path()),
        &shipped_verifying_key(),
    )
}

pub fn can_use_main_readonly_at(path: &Path, key: &VerifyingKey) -> bool {
    can_use_main_readonly_full(path, None, key)
}

/// 真实路径版：读锚（只读，不建）+ 时钟回拨夹取。热路径上不许写盘。
fn can_use_main_readonly_full(path: &Path, anchor: Option<&Path>, key: &VerifyingKey) -> bool {
    let Ok(mut file) = load_file(path) else {
        // Unreadable / corrupt license file: refuse rather than guess.
        return false;
    };
    if file.trial_started_at.is_none() {
        return true;
    }
    anchor_trial_start(&mut file, anchor, false);
    let now = effective_now(&file, now_unix());
    build_status_with_key(&file, now, key).can_use_main
}

// ── Developer helpers (debug builds only) ────────────────────────────────

/// True only in a **debug** build. `LEBI_DEV_TOOLS` used to turn these on inside
/// a release binary too — one environment variable, then
/// `dev_restore_license_backup` hands out a fresh trial
/// (`docs/records/20260918-license-hardening.md` P0-5). No environment variable
/// reopens this door.
pub fn dev_tools_enabled() -> bool {
    dev_tools_gate(cfg!(debug_assertions))
}

/// The decision, split out so the release half is testable in a debug test run.
fn dev_tools_gate(debug_build: bool) -> bool {
    debug_build
}

fn dev_backup_path() -> PathBuf {
    crate::paths::data_path("license.dev-backup.json")
}

/// True when a pre-simulate backup exists (can restore).
pub fn dev_has_license_backup() -> bool {
    dev_backup_path().exists()
}

/// Snapshot current license file, then force **expired lock** state (no token, trial long over).
/// Use from Settings → 开发者 to walk the full lock-screen flow.
pub fn dev_simulate_expired() -> Result<LicenseStatus, LicenseError> {
    if !dev_tools_enabled() {
        return Err(LicenseError::Parse("dev tools disabled".into()));
    }
    let path = license_file_path();
    let bak = dev_backup_path();
    if path.exists() {
        fs::copy(&path, &bak)?;
    } else if bak.exists() {
        let _ = fs::remove_file(&bak);
    }
    // Trial started well before TRIAL_DAYS ago → locked.
    let started = (Utc::now() - Duration::days(TRIAL_DAYS + 5)).to_rfc3339();
    let file = LicenseFile {
        token: None,
        trial_started_at: Some(started),
        last_nudge_date: None,
        last_seen_unix: Some(now_unix()),
    };
    save_file(&path, &file)?;
    Ok(build_status_with_key(
        &file,
        now_unix(),
        &shipped_verifying_key(),
    ))
}

/// Restore `license.json` from the backup taken by [`dev_simulate_expired`].
/// If no backup, delete license file so the next load starts a fresh trial.
pub fn dev_restore_license_backup() -> Result<LicenseStatus, LicenseError> {
    if !dev_tools_enabled() {
        return Err(LicenseError::Parse("dev tools disabled".into()));
    }
    let path = license_file_path();
    let bak = dev_backup_path();
    if bak.exists() {
        fs::copy(&bak, &path)?;
        let _ = fs::remove_file(&bak);
    } else if path.exists() {
        fs::remove_file(&path)?;
    }
    load_status_at(&path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use tempfile::tempdir;

    /// 每个测试自己生成一对密钥，再把公钥注入被测函数。
    /// 仓库里因此永远不需要存任何私钥 —— 真实事故见
    /// `docs/records/20260918-reaudit.md` P0-1（私钥曾与出厂公钥一起躺在源码里）。
    fn test_key() -> (SigningKey, VerifyingKey) {
        let sk = SigningKey::generate(&mut OsRng);
        let vk = sk.verifying_key();
        (sk, vk)
    }

    fn future_exp(days: i64) -> i64 {
        Utc::now().timestamp() + days * 86400
    }

    fn roster(ids: &[&str]) -> Option<Vec<String>> {
        Some(ids.iter().map(|s| s.to_string()).collect())
    }

    fn signed(sk: &SigningKey, days: i64, ids: Option<&[&str]>) -> String {
        let personas = ids.map(|list| list.iter().map(|s| s.to_string()).collect());
        sign_token_with_key(sk, future_exp(days), None, None, None, personas).unwrap()
    }

    #[test]
    fn sign_verify_roundtrip() {
        let (sk, vk) = test_key();
        let exp = future_exp(30);
        let tok = sign_token_with_key(
            &sk,
            exp,
            Some(Utc::now().timestamp()),
            Some("lic-1".into()),
            Some("month".into()),
            None,
        )
        .unwrap();
        let v = verify_token_with_key(&tok, &vk).unwrap();
        assert_eq!(v.exp_unix, exp);
        assert_eq!(v.lic_id.as_deref(), Some("lic-1"));
        assert!(v.personas.is_empty(), "老码没有名单字段");
    }

    #[test]
    fn a_token_carries_its_persona_roster() {
        let (sk, vk) = test_key();
        let dir = tempdir().unwrap();
        let st = apply_token_at_with_key(
            &dir.path().join("license.json"),
            &signed(&sk, 10, Some(&["sao-di-seng", "yu-tian"])),
            &vk,
        )
        .unwrap();
        assert_eq!(st.phase, LicensePhase::Licensed);
        assert_eq!(st.personas, vec!["sao-di-seng", "yu-tian"]);
        assert!(st.unknown_personas.is_empty());
    }

    #[test]
    fn a_legacy_token_leaves_the_roster_empty_and_is_byte_compatible() {
        let (sk, vk) = test_key();
        let dir = tempdir().unwrap();
        let tok = signed(&sk, 10, None);
        // No field on the wire: a token minted for an older client is unchanged.
        let json = b64_decode(tok.split('.').nth(1).unwrap()).unwrap();
        assert!(!String::from_utf8_lossy(&json).contains("personas"));
        let st = apply_token_at_with_key(&dir.path().join("license.json"), &tok, &vk).unwrap();
        assert_eq!(st.phase, LicensePhase::Licensed);
        assert!(st.personas.is_empty(), "没写名单 = 只有自带，不是出错");
        assert!(st.unknown_personas.is_empty());
    }

    #[test]
    fn an_unknown_persona_is_reported_not_swallowed() {
        let (sk, vk) = test_key();
        let dir = tempdir().unwrap();
        let tok = signed(
            &sk,
            10,
            Some(&[
                "sao-di-seng",
                "ghost-b",
                "sao-di-seng",
                "ghost-a",
                "ghost-b",
            ]),
        );
        let st = apply_token_at_with_key(&dir.path().join("license.json"), &tok, &vk).unwrap();
        assert_eq!(st.personas, vec!["sao-di-seng"], "认识的留下，去重");
        assert_eq!(
            st.unknown_personas,
            vec!["ghost-a", "ghost-b"],
            "不认识的另立一列，排序去重"
        );
    }

    #[test]
    fn a_trial_shows_no_licensed_personas() {
        let (_, vk) = test_key();
        let dir = tempdir().unwrap();
        let st = load_status_at_with_key(&dir.path().join("license.json"), &vk).unwrap();
        assert_eq!(st.phase, LicensePhase::Trial);
        assert!(st.personas.is_empty());
        assert!(st.unknown_personas.is_empty());
    }

    #[test]
    fn personas_survive_expiry() {
        let (sk, vk) = test_key();
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        let expired = sign_token_with_key(
            &sk,
            Utc::now().timestamp() - 86400,
            None,
            None,
            None,
            roster(&["sao-di-seng", "ghost-x"]),
        )
        .unwrap();
        let file = LicenseFile {
            token: Some(expired),
            trial_started_at: Some((Utc::now() - Duration::days(TRIAL_DAYS + 5)).to_rfc3339()),
            ..Default::default()
        };
        save_file(&path, &file).unwrap();
        let st = load_status_at_with_key(&path, &vk).unwrap();
        assert_eq!(st.phase, LicensePhase::Locked, "主能力已锁");
        assert!(st.show_full_lock);
        assert_eq!(st.personas, vec!["sao-di-seng"], "过期不收回角色");
        assert_eq!(st.unknown_personas, vec!["ghost-x"]);
    }

    #[test]
    fn can_use_main_readonly_never_writes_and_matches_the_trial_window() {
        let (_, vk) = test_key();
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");

        // 全新装机：文件还不存在 —— 读只读检查必须放行（等同 load_status 开试用）。
        assert!(can_use_main_readonly_at(&path, &vk));
        assert!(
            !path.exists(),
            "只读检查写了盘 = 热路径上每次都动 license.json"
        );

        // 试用期内：放行，且依旧不写。
        let started = (Utc::now() - Duration::days(1)).to_rfc3339();
        save_file(
            &path,
            &LicenseFile {
                trial_started_at: Some(started),
                ..Default::default()
            },
        )
        .unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        assert!(can_use_main_readonly_at(&path, &vk));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            before,
            "只读检查刷新了 last_seen"
        );

        // 试用过期：拦住。
        let expired = (Utc::now() - Duration::days(TRIAL_DAYS + 2)).to_rfc3339();
        save_file(
            &path,
            &LicenseFile {
                trial_started_at: Some(expired),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!can_use_main_readonly_at(&path, &vk));
    }

    #[test]
    fn trial_then_lock() {
        let (_, vk) = test_key();
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        // Force trial started 4 days ago
        let started = (Utc::now() - Duration::days(4)).to_rfc3339();
        let file = LicenseFile {
            trial_started_at: Some(started),
            ..Default::default()
        };
        save_file(&path, &file).unwrap();
        let st = load_status_at_with_key(&path, &vk).unwrap();
        assert_eq!(st.phase, LicensePhase::Locked);
        assert!(st.show_full_lock);
        assert!(!st.can_use_main);
    }

    #[test]
    fn apply_and_status_licensed() {
        let (sk, vk) = test_key();
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        let tok = signed(&sk, 10, None);
        let st = apply_token_at_with_key(&path, &tok, &vk).unwrap();
        assert_eq!(st.phase, LicensePhase::Licensed);
        assert!(st.can_use_main);
        assert!(!st.show_full_lock);
    }

    #[test]
    fn reject_older_token() {
        let (sk, vk) = test_key();
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        let long = signed(&sk, 30, None);
        apply_token_at_with_key(&path, &long, &vk).unwrap();
        let short = signed(&sk, 5, None);
        let err = apply_token_at_with_key(&path, &short, &vk).unwrap_err();
        assert!(matches!(err, LicenseError::OlderThanCurrent));
    }

    #[test]
    fn same_token_error() {
        let (sk, vk) = test_key();
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        let tok = signed(&sk, 10, None);
        apply_token_at_with_key(&path, &tok, &vk).unwrap();
        let err = apply_token_at_with_key(&path, &tok, &vk).unwrap_err();
        assert!(matches!(err, LicenseError::SameAsCurrent));
    }

    #[test]
    fn bad_signature_rejected() {
        let (other, _) = test_key();
        let (_, vk) = test_key();
        let tok = signed(&other, 10, None);
        assert!(matches!(
            verify_token_with_key(&tok, &vk),
            Err(LicenseError::BadSignature | LicenseError::InvalidFormat)
        ));
    }

    #[test]
    fn a_token_for_another_product_is_rejected() {
        // 换锁之后仍然只认本产品的码：签名对、product 不对 → 拒。
        let (sk, vk) = test_key();
        let tok = signed(&sk, 10, None);
        assert!(verify_token_with_key(&tok, &vk).is_ok());
        let (_, other_vk) = test_key();
        assert!(verify_token_with_key(&tok, &other_vk).is_err());
    }

    #[test]
    fn _osrng_compiles() {
        let _ = SigningKey::generate(&mut OsRng);
    }

    // ── P0-5：试用不能靠删文件 / 换数据根 / 改钟重新开始 ──────────────────

    /// release 构建里没有后门：不再看环境变量。
    #[test]
    fn release_builds_have_no_dev_tools() {
        assert!(!dev_tools_gate(false), "release 构建必须关掉开发者工具");
        assert!(dev_tools_gate(true), "debug 构建里它们才是可用的");
        assert_eq!(
            dev_tools_enabled(),
            cfg!(debug_assertions),
            "dev_tools_enabled 只能由构建类型决定，不许受环境变量影响"
        );
    }

    /// 把钟往回拨，不会多出任何时间。
    #[test]
    fn a_rewound_clock_does_not_buy_trial_time() {
        let (_, vk) = test_key();
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        // 试用 2 天前开的（第 3 天还没到）…但上一次运行「见过」5 天后的时间：
        // 说明有人把钟拨回去了 —— 判定必须按那 5 天后的时间算。
        let started = (Utc::now() - Duration::days(2)).to_rfc3339();
        let future = (Utc::now() + Duration::days(5)).timestamp();
        save_file(
            &path,
            &LicenseFile {
                trial_started_at: Some(started),
                last_seen_unix: Some(future),
                ..Default::default()
            },
        )
        .unwrap();

        let st = load_status_at_with_key(&path, &vk).unwrap();
        assert_eq!(st.phase, LicensePhase::Locked, "回拨时钟买到了试用时间");
        assert!(!st.can_use_main);
        assert!(!can_use_main_readonly_at(&path, &vk));
    }

    /// 删掉 license.json（或换个数据根）不再是一张新的三天试用 —— 锚说了算。
    #[test]
    fn deleting_the_license_file_does_not_mint_a_new_trial() {
        let (_, vk) = test_key();
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        let anchor = dir.path().join("trial-anchor");
        // 锚：这台机器的试用 10 天前就开始了。
        std::fs::write(
            &anchor,
            format!("{}\n", (Utc::now() - Duration::days(10)).to_rfc3339()),
        )
        .unwrap();

        // 数据根空着（文件被删了 / 换了新目录）。
        assert!(!path.exists());
        let st = load_status_full(&path, Some(&anchor), &vk).unwrap();

        assert_eq!(st.phase, LicensePhase::Locked, "删文件又拿到一张新试用");
        assert!(!can_use_main_readonly_full(&path, Some(&anchor), &vk));
        // 锚已经写进文件里了 —— 不是每次读盘都从头算。
        let saved = load_file(&path).unwrap();
        assert!(saved.trial_started_at.is_some());
    }

    /// 锚只往前钉：数据根里写一个更晚的起点也会被夹回去。
    #[test]
    fn the_anchor_clamps_a_later_trial_start() {
        let (_, vk) = test_key();
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        let anchor = dir.path().join("trial-anchor");
        let anchored = (Utc::now() - Duration::days(9)).to_rfc3339();
        std::fs::write(&anchor, format!("{anchored}\n")).unwrap();
        save_file(
            &path,
            &LicenseFile {
                trial_started_at: Some(Utc::now().to_rfc3339()),
                ..Default::default()
            },
        )
        .unwrap();

        let st = load_status_full(&path, Some(&anchor), &vk).unwrap();

        assert_eq!(st.phase, LicensePhase::Locked, "自己写的时间盖过了锚");
        assert_eq!(
            load_file(&path).unwrap().trial_started_at.as_deref(),
            Some(anchored.as_str())
        );
    }

    /// 热路径只读：检查一遍不许建锚、不许动 license.json。
    #[test]
    fn the_readonly_check_never_creates_anything() {
        let (_, vk) = test_key();
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        let anchor = dir.path().join("trial-anchor");

        assert!(can_use_main_readonly_full(&path, Some(&anchor), &vk));
        assert!(!anchor.exists(), "只读检查建了锚文件");
        assert!(!path.exists(), "只读检查写了 license.json");
    }

    /// 签发机自检：设了 `LEBI_ISSUER_SEED_HEX` 或 `LEBI_ISSUER_SEED_FILE` 时，
    /// 用那把私钥签一张码，再用**出厂公钥**验回来。通过 = 这台签发机的钥匙就是
    /// 这台构建认的锁（发出去的码不会「签了却激活不了」）。
    ///
    /// 客户机 / CI 上没有签发机私钥，跳过——那是预期，不是失败。
    #[test]
    fn issuer_seed_matches_the_shipped_public_key_when_provided() {
        use ed25519_dalek::SigningKey;

        let hex = std::env::var("LEBI_ISSUER_SEED_HEX").ok().or_else(|| {
            let path = std::env::var("LEBI_ISSUER_SEED_FILE")
                .unwrap_or_else(|_| "~/.lebi-ai-issuer/seed.hex".into());
            let path = path.replace('~', &std::env::var("HOME").unwrap_or_default());
            std::fs::read_to_string(path).ok()
        });
        let Some(hex) = hex else {
            eprintln!("[issuer-smoke] 没找到签发私钥，跳过（客户机 / CI 正常）");
            return;
        };
        eprintln!("[issuer-smoke] 用签发私钥做签→验闭环");
        let mut seed = [0u8; 32];
        let hex = hex.trim();
        assert_eq!(hex.len(), 64, "签发私钥应为 64 位十六进制");
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            seed[i] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16)
                .expect("签发私钥不是合法十六进制");
        }
        let sk = SigningKey::from_bytes(&seed);
        assert_eq!(
            sk.verifying_key().to_bytes(),
            PUBLIC_KEY_BYTES,
            "签发机私钥与出厂公钥不配对 —— 用它签的码客户端一律拒"
        );

        let now = now_unix();
        let tok = sign_token_with_key(&sk, now + 86400, Some(now), None, None, None).unwrap();
        let v = verify_token(&tok).expect("签发机签出的码，出厂公钥必须验得过去");
        assert_eq!(v.exp_unix, now + 86400);
    }

    /// 只扫可能藏密钥的文本文件；跳过构建产物与依赖。
    ///
    /// 一次读完整个目录再把结果排队 —— 早期的写法「找到就 return」会丢掉
    /// 同一目录里还没看过的项，整棵树大半被跳过（守卫因此假绿过一次）。
    struct LicenseSourceScan {
        dirs: Vec<std::path::PathBuf>,
        files: Vec<std::path::PathBuf>,
    }

    impl LicenseSourceScan {
        fn new(root: &std::path::Path) -> Self {
            Self {
                dirs: vec![root.to_path_buf()],
                files: Vec::new(),
            }
        }
    }

    impl Iterator for LicenseSourceScan {
        type Item = std::path::PathBuf;

        fn next(&mut self) -> Option<Self::Item> {
            const SKIP: &[&str] = &["target", ".git", "node_modules", "dist", ".trash"];
            const EXTS: &[&str] = &["rs", "py", "html", "sh", "ps1", "toml", "json", "md"];
            loop {
                if let Some(file) = self.files.pop() {
                    return Some(file);
                }
                let dir = self.dirs.pop()?;
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if path.is_dir() {
                        if !SKIP.contains(&name.as_str()) {
                            self.dirs.push(path);
                        }
                    } else if path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| EXTS.contains(&e))
                    {
                        self.files.push(path);
                    }
                }
            }
        }
    }

    /// 列表里的一段：一个字节值 / 列表分隔符 / 别的东西。
    enum ByteToken {
        Value(u8),
        Separator,
        Other,
    }

    /// 从 `text` 头部切一段。返回值与剩余部分。
    fn split_byte_token(text: &str) -> (ByteToken, &str) {
        const SEPARATORS: &[char] = &[',', ';', '[', ']', '(', ')', '{', '}', '|'];
        let mut chars = text.char_indices();
        let (_, first) = chars.next().expect("非空");
        if first.is_whitespace() || SEPARATORS.contains(&first) {
            return (ByteToken::Separator, &text[first.len_utf8()..]);
        }
        let digits = text
            .find(|c: char| c.is_whitespace() || SEPARATORS.contains(&c))
            .unwrap_or(text.len());
        let token = &text[..digits];
        let rest = &text[digits..];
        let parsed = match token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
        {
            Some(hex) if !hex.is_empty() => u8::from_str_radix(hex, 16).ok(),
            Some(_) => None,
            None => token.parse::<u8>().ok(),
        };
        match parsed {
            Some(b) => (ByteToken::Value(b), rest),
            None => {
                // 不是字节值也不是分隔符：整段跳过，避免把 `1234` 拆成 `12`/`34`。
                let end = text
                    .find(|c: char| c.is_whitespace() || SEPARATORS.contains(&c))
                    .unwrap_or(text.len());
                (ByteToken::Other, &text[end.max(1)..])
            }
        }
    }

    /// 从一段文本里抠出「可能是 32 字节私钥」的候选：
    /// ① 任意 ≥32 个连续字节值的列表（十六进制 `0x..` 或十进制写法）——
    ///    不依赖 `[u8; 32]` 这个标记：`vec![]` / Python 列表 / 裸数组都抓得到；
    /// ② 恰好 64 位的十六进制串（seed 文件 / 环境变量 / 网页表单里的常见写法）。
    fn seed_candidates(text: &str) -> Vec<[u8; 32]> {
        let mut out = Vec::new();

        // ① 连续字节值列表。不再依赖 `[u8; 32]` 标记：只要逗号 / 空白 / 括号
        //    连起来的一串数字都是 0..=255，就按字节数组看待。长列表取多个起点，
        //    防止前面有填充或我数错。误报无害——只有真能推出出厂公钥才报错。
        let mut run: Vec<u8> = Vec::new();
        let flush = |run: &mut Vec<u8>, out: &mut Vec<[u8; 32]>| {
            if run.len() >= 32 {
                for start in 0..(run.len() - 32).min(8) + 1 {
                    out.push(run[start..start + 32].try_into().expect("32 bytes"));
                }
            }
            run.clear();
        };
        let mut rest = text;
        while !rest.is_empty() {
            let (token, next) = split_byte_token(rest);
            rest = next;
            match token {
                ByteToken::Value(b) => run.push(b),
                ByteToken::Separator => {}
                ByteToken::Other => flush(&mut run, &mut out),
            }
        }
        flush(&mut run, &mut out);

        // ② 恰好 64 位的十六进制串（seed 文件 / 环境变量 / 网页表单里的常见写法）。
        for token in text.split(|c: char| !c.is_ascii_hexdigit()) {
            if token.len() != 64 {
                continue;
            }
            let mut bytes = [0u8; 32];
            let mut ok = true;
            for (i, chunk) in token.as_bytes().chunks(2).enumerate() {
                let part = std::str::from_utf8(chunk).unwrap_or("zz");
                match u8::from_str_radix(part, 16) {
                    Ok(b) => bytes[i] = b,
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                out.push(bytes);
            }
        }

        out
    }

    /// 扫描器本身要先被证明有效：否则「没抓到」会被误读成「没有泄漏」。
    #[test]
    fn the_leak_scanner_finds_a_planted_seed() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let seed = SigningKey::generate(&mut OsRng);
        let bytes = seed.to_bytes();

        // 十六进制数组写法（Rust）
        let mut rust = String::from("const LEAK: [u8; 32] = [\n");
        for b in &bytes {
            rust.push_str(&format!("    0x{b:02x},\n"));
        }
        rust.push_str("];\n");
        // 十进制数组写法
        let mut dec = String::from("const LEAK2: [u8; 32] = [");
        for b in &bytes {
            dec.push_str(&format!("{b}, "));
        }
        dec.push_str("];\n");
        // 64 位十六进制串写法
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

        // 无标记列表写法：`vec![…]` / Python 列表 —— 老扫描器（盯 `[u8; 32]`）抓不到
        let mut plain = String::from("SEED = [");
        for b in &bytes {
            plain.push_str(&format!("{b}, "));
        }
        plain.push_str("]\n");

        let mut found = false;
        for text in [&rust, &dec, &hex, &plain] {
            if seed_candidates(text)
                .iter()
                .any(|c| SigningKey::from_bytes(c).verifying_key() == seed.verifying_key())
            {
                found = true;
            }
        }
        assert!(found, "扫描器抓不到植入的私钥：这条守卫本身就是假的");

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(
            LicenseSourceScan::new(&root).any(|p| p.ends_with("crates/hermes-core/src/license.rs")),
            "遍历器走不到本文件，守卫会假绿"
        );
    }

    /// 出厂公钥对应的**签名私钥不允许出现在仓库里**。
    ///
    /// 真实事故（见 `docs/records/20260918-reaudit.md` P0-1）：`DEV_SEED` 与出厂公钥
    /// 配对写进源码并提交，任何拿到源码的人都能伪造任意人物、任意期限的授权码。
    /// 这条测试扫描全仓（工作树）里所有「32 字节字面量 / 64 位十六进制串」，
    /// 只要有一个能推出出厂公钥就报错——换锁之后还要防复发。
    #[test]
    fn the_signing_seed_for_the_shipped_key_is_not_in_the_repo() {
        use ed25519_dalek::SigningKey;

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut candidates = 0usize;
        let mut scanned = 0usize;

        for path in LicenseSourceScan::new(&root) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            scanned += 1;
            for seed in seed_candidates(&text) {
                candidates += 1;
                if SigningKey::from_bytes(&seed).verifying_key().to_bytes() == PUBLIC_KEY_BYTES {
                    panic!(
                        "出厂签名私钥出现在 {} —— 必须立刻轮换密钥对，而不是只删这一行",
                        path.display()
                    );
                }
            }
        }

        assert!(scanned > 0, "一个文件都没扫到，测试本身失效了");
        assert!(candidates > 0, "一个候选 seed 都没找到，扫描逻辑失效了");
    }
}
