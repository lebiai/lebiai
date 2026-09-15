//! License / trial Tauri commands (docs/spec/license-ux.md).

use hermes_core::{
    dev_has_license_backup, dev_restore_license_backup, dev_simulate_expired, dev_tools_enabled,
    load_status, mark_nudge_seen, LicenseError, LicenseStatus,
};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

use crate::commands::personas;
use crate::error::GuiError;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyLicenseResult {
    pub status: LicenseStatus,
    pub message: String,
    /// 这张码落定后处于 enabled 的授权角色 id（含此前已勾选、这张码仍然覆盖的）；
    /// 自带角色不在其中（它们不占名单）。老格式码 / 试用 → 空。
    pub enabled_personas: Vec<String>,
}

/// Stable error codes for frontend i18n (`license.err.*`).
fn map_err(e: LicenseError) -> GuiError {
    match e {
        LicenseError::InvalidFormat => GuiError::Config("license_invalid_format".into()),
        LicenseError::BadSignature => GuiError::Config("license_bad_signature".into()),
        LicenseError::WrongProduct => GuiError::Config("license_wrong_product".into()),
        LicenseError::Expired => GuiError::Config("license_expired".into()),
        LicenseError::OlderThanCurrent => GuiError::Config("license_older".into()),
        LicenseError::Io(e) => GuiError::Internal(e.to_string()),
        LicenseError::Parse(e) => GuiError::Internal(e),
        // 落码路径已经自己消化了「同一张码」（见 `apply_license_at`）：这里只为穷尽
        // 枚举留着。真的漏出来时前端没有对应文案，会退回中性提示，不会白屏。
        LicenseError::SameAsCurrent => GuiError::Config("license_same".into()),
    }
}

#[tauri::command]
pub fn get_license_status() -> Result<LicenseStatus, GuiError> {
    load_status().map_err(map_err)
}

#[tauri::command]
pub fn apply_license(token: String) -> Result<ApplyLicenseResult, GuiError> {
    apply_license_at(
        &hermes_core::license::license_file_path(),
        &personas::prefs_path(),
        &token,
    )
}

/// 落码 + **自动开通码里点名的工位**。路径可注入，测试才能用 tempdir 走这条真实路径。
///
/// 用户实测反馈「输入授权码后页面没变化」的根因就在这里：以前只写了 `license.json`，
/// 名单里的角色在设置页是**没勾**的状态，侧栏自然一动不动。现在落码即勾选。
/// 同一张码重粘 = 把这份名单再开通一遍（不报「已是当前码」），因为用户重粘通常
/// 就是上一轮没看到变化。
pub(crate) fn apply_license_at(
    license_path: &Path,
    prefs_path: &Path,
    token: &str,
) -> Result<ApplyLicenseResult, GuiError> {
    let status = match hermes_core::license::apply_token_at(license_path, token) {
        Ok(status) => status,
        // 同一张码再粘一次：绝不拿「这已经是当前授权码」把人挡回去——他重粘，
        // 多半正是因为「粘了没反应」（老版本只写码、不开通工位）。照常开通名单。
        Err(LicenseError::SameAsCurrent) => {
            let status = hermes_core::license::load_status_at(license_path).map_err(map_err)?;
            let licensed: BTreeSet<String> = status.personas.iter().cloned().collect();
            let enabled_personas = personas::enable_licensed_at(prefs_path, &licensed)?;
            return Ok(ApplyLicenseResult {
                status,
                message: "ok".into(),
                enabled_personas,
            });
        }
        Err(e) => return Err(map_err(e)),
    };
    let licensed: BTreeSet<String> = status.personas.iter().cloned().collect();
    let enabled_personas = personas::enable_licensed_at(prefs_path, &licensed)?;
    Ok(ApplyLicenseResult {
        status,
        message: "ok".into(),
        enabled_personas,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::personas::{items_at, PersonaItem};

    /// 与 `scripts/issue-license.py` 的 `DEFAULT_SEED_HEX`、`hermes-core` 测试同一把种子
    /// （配对内置公钥）。签出来的就是客户端认的真码。
    const DEV_SEED: [u8; 32] = [
        0xb3, 0x6d, 0xd8, 0xc9, 0xe2, 0x35, 0xb0, 0xe3, 0x09, 0x7f, 0x97, 0x29, 0xc1, 0x45, 0x19,
        0x0d, 0x78, 0x07, 0xf3, 0x14, 0xa3, 0x0a, 0xb5, 0x29, 0x52, 0x7c, 0xf0, 0xec, 0xbd, 0x1f,
        0x23, 0x3b,
    ];

    /// 自带三人：谁都有、不进授权码。顺序 = `persona::SOURCES` 顺序。
    const BUILTINS: [&str; 3] = ["li-xian", "xiao-wen", "da-dao-yan"];

    fn token(days: i64, personas: Option<&[&str]>) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        hermes_core::license::sign_token_with_seed(
            &DEV_SEED,
            now + days * 86400,
            Some(now),
            Some("lic-test".into()),
            Some("year".into()),
            personas.map(|ids| ids.iter().map(|s| s.to_string()).collect()),
        )
        .unwrap()
    }

    /// 盘上这份授权 + 这份选择渲染出来的、处于 enabled 的工位 id。
    /// 走的是 `list_personas` 同一处渲染（`items_at`），不是另写一份断言口径。
    fn enabled_on_disk(license: &Path, prefs: &Path) -> Vec<String> {
        let licensed: BTreeSet<String> = hermes_core::license::load_status_at(license)
            .unwrap()
            .personas
            .into_iter()
            .collect();
        items_at(prefs, &licensed)
            .into_iter()
            .filter(|i: &PersonaItem| i.enabled)
            .map(|i| i.id)
            .collect()
    }

    #[test]
    fn a_code_with_a_roster_turns_its_workstations_on() {
        let dir = tempfile::tempdir().unwrap();
        let (license, prefs) = (
            dir.path().join("license.json"),
            dir.path().join("personas.json"),
        );
        let res = apply_license_at(
            &license,
            &prefs,
            &token(365, Some(&["wang-hai-yan", "xiao-xie"])),
        )
        .unwrap();

        assert!(res.status.can_use_main);
        assert_eq!(res.enabled_personas, vec!["wang-hai-yan", "xiao-xie"]);
        assert_eq!(
            enabled_on_disk(&license, &prefs),
            vec![
                "li-xian",
                "xiao-wen",
                "da-dao-yan",
                "wang-hai-yan",
                "xiao-xie"
            ],
            "用户没勾任何东西，侧栏也该直接多出这两个工位（自带永远在）"
        );
        assert_eq!(
            std::fs::read_to_string(&prefs).unwrap(),
            "{\n  \"enabled\": [\n    \"wang-hai-yan\",\n    \"xiao-xie\"\n  ]\n}",
            "落盘的只有授权角色：自带不占文件"
        );
    }

    #[test]
    fn re_pasting_the_same_code_still_opens_its_workstations() {
        let dir = tempfile::tempdir().unwrap();
        let (license, prefs) = (
            dir.path().join("license.json"),
            dir.path().join("personas.json"),
        );
        let code = token(365, Some(&["wang-hai-yan", "xiao-xie"]));
        apply_license_at(&license, &prefs, &code).unwrap();
        // 老版本的现场：码在盘上，但用户的选择文件从没写过 → 侧栏一动不动。
        std::fs::remove_file(&prefs).unwrap();

        let res = apply_license_at(&license, &prefs, &code).unwrap();

        assert_eq!(res.enabled_personas, vec!["wang-hai-yan", "xiao-xie"]);
        assert_eq!(
            enabled_on_disk(&license, &prefs),
            vec![
                "li-xian",
                "xiao-wen",
                "da-dao-yan",
                "wang-hai-yan",
                "xiao-xie"
            ],
            "重粘同一张码也要把工位开通出来，而不是报「已是当前码」"
        );
    }

    #[test]
    fn a_legacy_code_neither_errors_nor_clears_the_choices() {
        let dir = tempfile::tempdir().unwrap();
        let (license, prefs) = (
            dir.path().join("license.json"),
            dir.path().join("personas.json"),
        );
        std::fs::write(&prefs, "{\"enabled\": [\"xiao-jin\"]}").unwrap();
        let before = std::fs::read_to_string(&prefs).unwrap();

        let res = apply_license_at(&license, &prefs, &token(365, None)).unwrap();

        assert!(res.status.can_use_main, "老格式码照样能用");
        assert!(res.enabled_personas.is_empty());
        assert_eq!(
            std::fs::read_to_string(&prefs).unwrap(),
            before,
            "没带名单的码不许动用户已选的工位"
        );
        assert_eq!(
            enabled_on_disk(&license, &prefs),
            BUILTINS.to_vec(),
            "没带名单 = 只有三个自带（选择还在盘上，等下一张点名的码再回来）"
        );
    }

    #[test]
    fn an_unknown_id_is_dropped_without_taking_the_known_one_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let (license, prefs) = (
            dir.path().join("license.json"),
            dir.path().join("personas.json"),
        );
        let res = apply_license_at(
            &license,
            &prefs,
            &token(365, Some(&["ghost-role", "wang-hai-yan"])),
        )
        .unwrap();

        assert_eq!(res.status.unknown_personas, vec!["ghost-role"]);
        assert_eq!(res.enabled_personas, vec!["wang-hai-yan"]);
        assert_eq!(
            enabled_on_disk(&license, &prefs),
            vec!["li-xian", "xiao-wen", "da-dao-yan", "wang-hai-yan"]
        );
    }

    #[test]
    fn a_second_code_swaps_the_roster() {
        let dir = tempfile::tempdir().unwrap();
        let (license, prefs) = (
            dir.path().join("license.json"),
            dir.path().join("personas.json"),
        );
        apply_license_at(
            &license,
            &prefs,
            &token(365, Some(&["wang-hai-yan", "xiao-xie"])),
        )
        .unwrap();

        let res = apply_license_at(
            &license,
            &prefs,
            &token(366, Some(&["xiao-jin", "wang-hai-yan"])),
        )
        .unwrap();

        assert_eq!(res.enabled_personas, vec!["wang-hai-yan", "xiao-jin"]);
        assert_eq!(
            enabled_on_disk(&license, &prefs),
            vec![
                "li-xian",
                "xiao-wen",
                "da-dao-yan",
                "wang-hai-yan",
                "xiao-jin"
            ],
            "粘哪张码就显示哪张：小谢不在新名单里，得下去"
        );
    }
}
