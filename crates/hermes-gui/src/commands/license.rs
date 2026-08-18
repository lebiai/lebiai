//! License / trial Tauri commands (docs/spec/license-ux.md).

use hermes_core::{
    apply_token, dev_has_license_backup, dev_restore_license_backup, dev_simulate_expired,
    dev_tools_enabled, load_status, mark_nudge_seen, LicenseError, LicenseStatus,
};
use serde::Serialize;

use crate::error::GuiError;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyLicenseResult {
    pub status: LicenseStatus,
    pub message: String,
}

/// Stable error codes for frontend i18n (`license.err.*`).
fn map_err(e: LicenseError) -> GuiError {
    match e {
        LicenseError::InvalidFormat => GuiError::Config("license_invalid_format".into()),
        LicenseError::BadSignature => GuiError::Config("license_bad_signature".into()),
        LicenseError::WrongProduct => GuiError::Config("license_wrong_product".into()),
        LicenseError::Expired => GuiError::Config("license_expired".into()),
        LicenseError::OlderThanCurrent => GuiError::Config("license_older".into()),
        LicenseError::SameAsCurrent => GuiError::Config("license_same".into()),
        LicenseError::Io(e) => GuiError::Internal(e.to_string()),
        LicenseError::Parse(e) => GuiError::Internal(e),
    }
}

#[tauri::command]
pub fn get_license_status() -> Result<LicenseStatus, GuiError> {
    load_status().map_err(map_err)
}

#[tauri::command]
pub fn apply_license(token: String) -> Result<ApplyLicenseResult, GuiError> {
    let status = apply_token(&token).map_err(map_err)?;
    Ok(ApplyLicenseResult {
        status,
        message: "ok".into(),
    })
}

#[tauri::command]
pub fn mark_license_nudge_seen() -> Result<LicenseStatus, GuiError> {
    mark_nudge_seen().map_err(map_err)
}

/// Debug / owner only: `true` in debug builds or when `LEBI_DEV_TOOLS` is set.
#[tauri::command]
pub fn license_dev_tools_enabled() -> bool {
    dev_tools_enabled()
}

#[tauri::command]
pub fn license_dev_has_backup() -> bool {
    dev_has_license_backup()
}

/// Snapshot current license, force full lock screen (expired). Real apply_license unlocks.
#[tauri::command]
pub fn license_dev_simulate_expired() -> Result<LicenseStatus, GuiError> {
    dev_simulate_expired().map_err(map_err)
}

/// Restore pre-simulate backup (or wipe → new trial if none).
#[tauri::command]
pub fn license_dev_restore_backup() -> Result<LicenseStatus, GuiError> {
    dev_restore_license_backup().map_err(map_err)
}
