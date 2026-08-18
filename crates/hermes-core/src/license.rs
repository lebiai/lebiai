//! Software license (signed token) + trial window for desktop delivery.
//!
//! Format: `LEBI1.<base64url(payload_json)>.<base64url(ed25519_sig)>`
//! Payload fields: `product`, `exp` (unix secs), optional `iat`, `lic_id`, `plan`.
//!
//! Spec: `docs/spec/license-ux.md`. Private key never ships in the client.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey, PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH};
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

/// Production verifying key (Ed25519, 32 bytes). Pair with `scripts/issue-license`.
/// Rotate by issuing a new keypair and shipping a client update.
const PUBLIC_KEY_BYTES: [u8; PUBLIC_KEY_LENGTH] = [
    0xae, 0x66, 0xe2, 0xa8, 0xc5, 0xb2, 0xd5, 0x2a, 0x67, 0x21, 0x67, 0xfc, 0x59, 0xaa, 0xae, 0xdb,
    0xaa, 0x63, 0xe0, 0x1a, 0x83, 0x26, 0x0d, 0x7b, 0x4a, 0x94, 0x92, 0x26, 0xdb, 0xc4, 0x45, 0x1f,
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
}

fn verifying_key() -> VerifyingKey {
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

/// Sign a payload (used by issue tooling and tests). `signing_key_bytes` = 32-byte seed.
pub fn sign_token_with_seed(
    seed: &[u8; 32],
    exp_unix: i64,
    iat_unix: Option<i64>,
    lic_id: Option<String>,
    plan: Option<String>,
) -> Result<String, LicenseError> {
    use ed25519_dalek::{Signer, SigningKey};
    let sk = SigningKey::from_bytes(seed);
    let payload = LicensePayload {
        product: PRODUCT_ID.to_string(),
        exp: exp_unix,
        iat: iat_unix,
        lic_id,
        plan,
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
    verifying_key()
        .verify(&json, &sig)
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

/// Path to license state file under the product data root.
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

fn active_verified(file: &LicenseFile, now: i64) -> Option<VerifiedLicense> {
    let token = file.token.as_ref()?;
    let v = verify_token(token).ok()?;
    if is_unexpired(v.exp_unix, now) {
        Some(v)
    } else {
        None
    }
}

fn build_status(file: &LicenseFile, now: i64) -> LicenseStatus {
    let today = local_date_string();
    let nudged_today = file.last_nudge_date.as_deref() == Some(today.as_str());

    if let Some(v) = active_verified(file, now) {
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
        return LicenseStatus {
            phase: LicensePhase::Licensed,
            urgency,
            can_use_main: true,
            show_full_lock: false,
            should_nudge: expiring && !nudged_today,
            expires_at: Some(
                Utc.timestamp_opt(v.exp_unix, 0)
                    .single()
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default(),
            ),
            remaining_secs: remaining,
            remaining_ratio: ratio,
            on_trial: false,
            wechat: WECHAT_CONTACT.to_string(),
            lic_id: v.lic_id,
            plan: v.plan,
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
                expires_at: Some(
                    Utc.timestamp_opt(end, 0)
                        .single()
                        .map(|d| d.to_rfc3339())
                        .unwrap_or_default(),
                ),
                remaining_secs: remaining,
                remaining_ratio: ratio,
                on_trial: true,
                wechat: WECHAT_CONTACT.to_string(),
                lic_id: None,
                plan: None,
            };
        }
    }

    // Locked
    let expires_at = file
        .token
        .as_ref()
        .and_then(|t| verify_token(t).ok())
        .map(|v| {
            Utc.timestamp_opt(v.exp_unix, 0)
                .single()
                .map(|d| d.to_rfc3339())
                .unwrap_or_default()
        })
        .or_else(|| {
            trial_end.map(|end| {
                Utc.timestamp_opt(end, 0)
                    .single()
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default()
            })
        });

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
    }
}

/// Load (or init trial), update last_seen, return status.
pub fn load_status() -> Result<LicenseStatus, LicenseError> {
    load_status_at(&license_file_path())
}

pub fn load_status_at(path: &Path) -> Result<LicenseStatus, LicenseError> {
    let mut file = load_file(path)?;
    let mut dirty = ensure_trial_started(&mut file);
    let now = now_unix();
    // Soft clock tracking
    if file.last_seen_unix.map(|t| now + 86400 < t).unwrap_or(false) {
        tracing::warn!("system clock appears to have moved backwards significantly");
    }
    if file.last_seen_unix != Some(now) {
        file.last_seen_unix = Some(now);
        dirty = true;
    }
    if dirty {
        save_file(path, &file)?;
    }
    Ok(build_status(&file, now))
}

/// Apply a new token. Same path for activate and renew.
pub fn apply_token(raw: &str) -> Result<LicenseStatus, LicenseError> {
    apply_token_at(&license_file_path(), raw)
}

pub fn apply_token_at(path: &Path, raw: &str) -> Result<LicenseStatus, LicenseError> {
    let raw = raw.trim();
    let verified = verify_token(raw)?;
    let now = now_unix();
    if !is_unexpired(verified.exp_unix, now) {
        return Err(LicenseError::Expired);
    }

    let mut file = load_file(path)?;
    ensure_trial_started(&mut file);

    if let Some(cur) = file.token.as_ref() {
        if cur.trim() == raw {
            return Err(LicenseError::SameAsCurrent);
        }
        if let Ok(cur_v) = verify_token(cur) {
            if is_unexpired(cur_v.exp_unix, now) && verified.exp_unix < cur_v.exp_unix {
                return Err(LicenseError::OlderThanCurrent);
            }
        }
    }

    file.token = Some(verified.token);
    file.last_seen_unix = Some(now);
    // Clear nudge so a new cycle can remind later if needed
    file.last_nudge_date = None;
    save_file(path, &file)?;
    Ok(build_status(&file, now))
}

/// Mark daily renew nudge as shown for local today.
pub fn mark_nudge_seen() -> Result<LicenseStatus, LicenseError> {
    mark_nudge_seen_at(&license_file_path())
}

pub fn mark_nudge_seen_at(path: &Path) -> Result<LicenseStatus, LicenseError> {
    let mut file = load_file(path)?;
    ensure_trial_started(&mut file);
    file.last_nudge_date = Some(local_date_string());
    file.last_seen_unix = Some(now_unix());
    save_file(path, &file)?;
    Ok(build_status(&file, now_unix()))
}

/// Whether main features may run (dialogue).
pub fn can_use_main() -> bool {
    load_status().map(|s| s.can_use_main).unwrap_or(false)
}

// ── Developer helpers (debug builds or LEBI_DEV_TOOLS=1) ─────────────────

/// Visible only to the product owner in debug / explicit env — never for end users in release.
pub fn dev_tools_enabled() -> bool {
    cfg!(debug_assertions) || std::env::var_os("LEBI_DEV_TOOLS").is_some()
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
    Ok(build_status(&file, now_unix()))
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

    /// Test-only: temporarily we need tokens signed with PUBLIC_KEY.
    /// Use the real seed paired with PUBLIC_KEY_BYTES.
    const DEV_SEED: [u8; 32] = [
        0xb3, 0x6d, 0xd8, 0xc9, 0xe2, 0x35, 0xb0, 0xe3, 0x09, 0x7f, 0x97, 0x29, 0xc1, 0x45, 0x19,
        0x0d, 0x78, 0x07, 0xf3, 0x14, 0xa3, 0x0a, 0xb5, 0x29, 0x52, 0x7c, 0xf0, 0xec, 0xbd, 0x1f,
        0x23, 0x3b,
    ];

    fn future_exp(days: i64) -> i64 {
        Utc::now().timestamp() + days * 86400
    }

    #[test]
    fn sign_verify_roundtrip() {
        let exp = future_exp(30);
        let tok = sign_token_with_seed(
            &DEV_SEED,
            exp,
            Some(Utc::now().timestamp()),
            Some("lic-1".into()),
            Some("month".into()),
        )
        .unwrap();
        let v = verify_token(&tok).unwrap();
        assert_eq!(v.exp_unix, exp);
        assert_eq!(v.lic_id.as_deref(), Some("lic-1"));
    }

    #[test]
    fn trial_then_lock() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        // Force trial started 4 days ago
        let started = (Utc::now() - Duration::days(4)).to_rfc3339();
        let file = LicenseFile {
            trial_started_at: Some(started),
            ..Default::default()
        };
        save_file(&path, &file).unwrap();
        let st = load_status_at(&path).unwrap();
        assert_eq!(st.phase, LicensePhase::Locked);
        assert!(st.show_full_lock);
        assert!(!st.can_use_main);
    }

    #[test]
    fn apply_and_status_licensed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        let tok = sign_token_with_seed(&DEV_SEED, future_exp(10), None, None, None).unwrap();
        let st = apply_token_at(&path, &tok).unwrap();
        assert_eq!(st.phase, LicensePhase::Licensed);
        assert!(st.can_use_main);
        assert!(!st.show_full_lock);
    }

    #[test]
    fn reject_older_token() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        let long = sign_token_with_seed(&DEV_SEED, future_exp(30), None, None, None).unwrap();
        apply_token_at(&path, &long).unwrap();
        let short = sign_token_with_seed(&DEV_SEED, future_exp(5), None, None, None).unwrap();
        let err = apply_token_at(&path, &short).unwrap_err();
        assert!(matches!(err, LicenseError::OlderThanCurrent));
    }

    #[test]
    fn same_token_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("license.json");
        let tok = sign_token_with_seed(&DEV_SEED, future_exp(10), None, None, None).unwrap();
        apply_token_at(&path, &tok).unwrap();
        let err = apply_token_at(&path, &tok).unwrap_err();
        assert!(matches!(err, LicenseError::SameAsCurrent));
    }

    #[test]
    fn public_key_matches_seed() {
        let sk = SigningKey::from_bytes(&DEV_SEED);
        assert_eq!(sk.verifying_key().to_bytes(), PUBLIC_KEY_BYTES);
    }

    #[test]
    fn bad_signature_rejected() {
        let mut sk_bytes = [0u8; 32];
        sk_bytes[0] = 1;
        let tok = sign_token_with_seed(&sk_bytes, future_exp(10), None, None, None).unwrap();
        // If random key equals ours (impossible-ish), skip
        if SigningKey::from_bytes(&sk_bytes).verifying_key().to_bytes() == PUBLIC_KEY_BYTES {
            return;
        }
        assert!(matches!(
            verify_token(&tok),
            Err(LicenseError::BadSignature | LicenseError::InvalidFormat)
        ));
    }

    #[test]
    fn _osrng_compiles() {
        let _ = SigningKey::generate(&mut OsRng);
    }
}
