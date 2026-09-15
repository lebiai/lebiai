//! 工位（人物）开关：设置里勾谁显示谁 + 授权码名单的闸门。
//!
//! 显示口径只有一处，别处不许再判一遍：
//! `enabled = builtin || (授权名单含该 id && 用户在设置里勾过)`。
//!
//! - 授权名单 = 授权码里的 `personas`（`hermes_core::LicenseStatus.personas`）。
//! - 试用期 / 老码没带名单 → 空 → 只有四个自带。
//! - 授权过期 → 名单照最后一份有效码，人物不消失（`LicenseStatus` 已经这么给）。
//! - 授权文件读不动 → **报错**，不假装「什么都没有」：静默收窄名单会连带把用户的选择抹掉。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use hermes_core::persona;

use crate::error::GuiError;
use crate::state::AppState;

/// 设置里「显示哪些人物」的落盘形状：数据根下 `personas.json`。
#[derive(Debug, Default, Serialize, Deserialize)]
struct PersonaPrefs {
    #[serde(default)]
    enabled: Vec<String>,
}

/// 前端人物条目（冻结接口，camelCase）。
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersonaItem {
    pub id: String,
    pub name: String,
    pub role: String,
    pub builtin: bool,
    /// 这份授权码点名了它。`false` 时 `enabled` 必为 `false`——设置页据此说
    /// 「不在你的授权内」，而不是给一个勾了没反应的开关。
    pub licensed: bool,
    pub enabled: bool,
}

/// 人物开关的落盘位置。授权落码时也要写它（自动开通名单里的工位），所以给命令层用。
/// 真源在 `hermes_core::persona::prefs_path`——GUI 与「本机开着的工位」必须同一个文件。
pub(crate) fn prefs_path() -> PathBuf {
    hermes_core::persona::prefs_path()
}

/// 授权码点名、且本版本认识的角色。试用期 / 老码没带名单 → 空（只剩自带）。
/// **授权闸门只此一处**：读盘、写盘、渲染都问它，不各判一遍。
pub fn licensed_ids() -> Result<BTreeSet<String>, GuiError> {
    hermes_core::load_status()
        .map(|s| s.personas.into_iter().collect())
        .map_err(|e| GuiError::Internal(e.to_string()))
}

fn is_selectable(id: &str, licensed: &BTreeSet<String>) -> bool {
    hermes_core::persona::is_selectable(id, licensed)
}

/// 读口在 `hermes_core::persona::selected_ids_at`——侧栏读它、「本机开着的工位」
/// （指路名册）也读它。两处各写一份判据，就会出现「侧栏有这个人、指路却说他不在」。
fn read_enabled(path: &Path, licensed: &BTreeSet<String>) -> BTreeSet<String> {
    hermes_core::persona::selected_ids_at(path, licensed)
}

/// 只写**选择**：自带角色永远在（`builtin`），未知与未授权 id 丢掉，所以文件里不留噪音。
/// 过滤放在写出口，任何调用方都写不出脏文件——手改本机文件也变不出没授权的角色。
fn write_enabled(
    path: &Path,
    ids: &BTreeSet<String>,
    licensed: &BTreeSet<String>,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let prefs = PersonaPrefs {
        enabled: ids
            .iter()
            .filter(|id| is_selectable(id, licensed))
            .cloned()
            .collect(),
    };
    let json = serde_json::to_string_pretty(&prefs).expect("persona prefs are serializable");
    std::fs::write(path, json)
}

/// 全部人物 + 是否授权 + 是否显示。**不筛掉未选中的**——设置页要拿它们当候选。
fn build_items(selected: &BTreeSet<String>, licensed: &BTreeSet<String>) -> Vec<PersonaItem> {
    persona::all()
        .iter()
        .map(|p| PersonaItem {
            id: p.id.clone(),
            name: p.name.clone(),
            role: p.role.clone(),
            builtin: p.builtin,
            licensed: licensed.contains(&p.id),
            enabled: p.builtin || selected.contains(&p.id),
        })
        .collect()
}

/// 某份选择 + 某份授权渲染出来的条目。路径可注入：测试用 tempdir，生产用 [`prefs_path`]。
pub(crate) fn items_at(path: &Path, licensed: &BTreeSet<String>) -> Vec<PersonaItem> {
    build_items(&read_enabled(path, licensed), licensed)
}

/// **唯一写盘口**：选择过闸门 → 落盘 → 返回盘上真正生效的 id（已去重、按字典序）。
/// 任何写这个文件的地方都走这里，别再写第二份判据。
fn commit(
    path: &Path,
    chosen: &BTreeSet<String>,
    licensed: &BTreeSet<String>,
) -> Result<Vec<String>, GuiError> {
    write_enabled(path, chosen, licensed).map_err(|e| GuiError::Internal(e.to_string()))?;
    Ok(read_enabled(path, licensed).into_iter().collect())
}

/// 落码后自动开通这名单里的工位（**并入**用户已有选择，不清空别的授权）。
///
/// `licensed` = 刚落定这张码的名单（`LicenseStatus.personas`，未知 id 已被 core 剔掉）：
/// - **空**（老格式码 / 试用 / 过期）→ 什么都不动、不报错：不能把用户已选的清空。
/// - 非空 → 名单里的角色全部开通；名单外的旧选择会被闸门挡掉（粘哪张码就显示哪张）。
///
/// 返回**落码后处于 enabled 的授权角色 id**（含此前已勾选、这张码仍然覆盖的）；
/// 自带角色不在其中——它们不占名单，也永远显示。
pub(crate) fn enable_licensed_at(
    path: &Path,
    licensed: &BTreeSet<String>,
) -> Result<Vec<String>, GuiError> {
    if licensed.is_empty() {
        return Ok(Vec::new());
    }
    let chosen: BTreeSet<String> = read_enabled(path, licensed)
        .into_iter()
        .chain(licensed.iter().cloned())
        .collect();
    commit(path, &chosen, licensed)
}

/// 会话绑定的人物：`None` = 没绑定，或 id 不认识。
/// 容错口径与会话 → 归属的转换（`persona::memory_owner_for`）完全一致：
/// 不认识的 id 不会 panic，只是这个会话没有人设/没有工位。
pub fn bound_persona(session_persona: Option<&str>) -> Option<&'static persona::Persona> {
    persona::get(session_persona?)
}

/// 只报「谁不认识」，不改口径：`new_session` 拿它挡掉叫错名字的开场。
pub fn require_persona(id: &str) -> Result<&'static persona::Persona, GuiError> {
    persona::get(id).ok_or_else(|| GuiError::NotFound(format!("persona {id}")))
}

/// 当前授权 + 当前这份选择渲染出来的条目。
fn items_from_disk(licensed: &BTreeSet<String>) -> Vec<PersonaItem> {
    items_at(&prefs_path(), licensed)
}

#[tauri::command]
pub fn list_personas(_state: State<'_, AppState>) -> Result<Vec<PersonaItem>, GuiError> {
    let licensed = licensed_ids()?;
    Ok(items_from_disk(&licensed))
}

/// 存盘后返回**盘上那一份**（不是内存里那一份）——前台看到的就是下次启动看到的。
#[tauri::command]
pub fn set_personas(
    _state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<Vec<PersonaItem>, GuiError> {
    let licensed = licensed_ids()?;
    let chosen: BTreeSet<String> = ids.into_iter().collect();
    commit(&prefs_path(), &chosen, &licensed)?;
    Ok(items_from_disk(&licensed))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 自带三人：谁都有、不进授权码。顺序 = `persona::SOURCES` 顺序。
    const BUILTINS: [&str; 3] = ["li-xian", "xiao-wen", "da-dao-yan"];

    fn roster(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn enabled_ids(items: &[PersonaItem]) -> Vec<&str> {
        items
            .iter()
            .filter(|i| i.enabled)
            .map(|i| i.id.as_str())
            .collect()
    }

    fn item<'a>(items: &'a [PersonaItem], id: &str) -> &'a PersonaItem {
        items
            .iter()
            .find(|i| i.id == id)
            .unwrap_or_else(|| panic!("{id} 必须在列表里"))
    }

    #[test]
    fn a_missing_file_shows_the_builtins_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let licensed = roster(&["xiao-xie", "yu-tian"]);
        let items = build_items(
            &read_enabled(&dir.path().join("personas.json"), &licensed),
            &licensed,
        );
        assert_eq!(enabled_ids(&items), BUILTINS);
        assert_eq!(
            items.len(),
            persona::all().len(),
            "设置页要拿到全部候选，未选中的也在列表里"
        );
    }

    #[test]
    fn a_saved_choice_comes_back_as_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("personas.json");
        let licensed = roster(&["xiao-xie", "yu-tian", "xiao-jin"]);
        write_enabled(&path, &roster(&["xiao-xie", "yu-tian"]), &licensed).unwrap();
        let items = build_items(&read_enabled(&path, &licensed), &licensed);
        assert_eq!(
            enabled_ids(&items),
            vec!["li-xian", "xiao-wen", "da-dao-yan", "xiao-xie", "yu-tian"]
        );
        assert!(item(&items, "xiao-jin").licensed, "授权里有 → 候选");
        assert!(!item(&items, "xiao-jin").enabled, "但用户没勾 → 不显示");
    }

    #[test]
    fn builtins_stay_on_even_when_the_file_is_silent_about_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("personas.json");
        std::fs::write(&path, r#"{"enabled":[]}"#).unwrap();
        let licensed = roster(&["xiao-yu"]);
        let items = build_items(&read_enabled(&path, &licensed), &licensed);
        assert_eq!(enabled_ids(&items), BUILTINS);
        for id in BUILTINS {
            assert!(item(&items, id).builtin);
        }
    }

    #[test]
    fn a_trial_never_shows_a_licensed_role_even_if_the_file_asks_for_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("personas.json");
        std::fs::write(&path, r#"{"enabled":["xiao-xie","xiao-jin"]}"#).unwrap();
        let trial = BTreeSet::new();
        let items = build_items(&read_enabled(&path, &trial), &trial);
        assert_eq!(enabled_ids(&items), BUILTINS, "试用期只有三个自带");
        assert!(!item(&items, "xiao-xie").licensed);
        assert!(!item(&items, "xiao-xie").enabled);
    }

    #[test]
    fn an_unauthorized_id_cannot_be_turned_on_from_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("personas.json");
        let licensed = roster(&["xiao-xie"]);
        std::fs::write(
            &path,
            r#"{"enabled":["xiao-xie","xiao-jin","not-a-person","da-dao-yan"]}"#,
        )
        .unwrap();
        assert_eq!(
            read_enabled(&path, &licensed)
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["xiao-xie".to_string()],
            "未授权 id、幽灵 id、自带都不进内存"
        );
        write_enabled(
            &path,
            &roster(&["xiao-jin", "ghost", "xiao-xie"]),
            &licensed,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"enabled\": [\n    \"xiao-xie\"\n  ]\n}",
            "落盘的只有已授权且非自带的真实选择"
        );
    }

    #[test]
    fn a_bound_session_resolves_to_its_persona_and_an_unknown_one_is_ignored() {
        assert_eq!(
            bound_persona(Some("wang-hai-yan")).unwrap().name,
            "情报王海燕"
        );
        assert!(bound_persona(Some("who-is-this")).is_none());
        assert!(bound_persona(None).is_none());
        assert!(require_persona("who-is-this").is_err());
        assert_eq!(require_persona("xiao-yu").unwrap().name, "主播小雨");
    }
}
