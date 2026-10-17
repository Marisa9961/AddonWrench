//! User groups are separate from addon state and never rename or remove VPKs.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    pub members: BTreeSet<String>,
}

impl Group {
    /// Empty groups cannot be toggled; mixed groups first enable all installed members.
    pub fn toggle_target(&self, addons: &[crate::addons::Addon]) -> Option<bool> {
        let mut installed = addons
            .iter()
            .filter(|addon| self.members.contains(&addon.key))
            .peekable();
        installed.peek()?;
        Some(!installed.all(|addon| addon.enabled))
    }

    pub fn changes(&self, addons: &[crate::addons::Addon], enabled: bool) -> Vec<(String, bool)> {
        addons
            .iter()
            .filter(|a| self.members.contains(&a.key) && a.enabled != enabled)
            .map(|a| (a.key.clone(), enabled))
            .collect()
    }
}

/// Count only installed members whose enabled state will actually change.
pub fn affected_groups(
    groups: &[Group],
    source: usize,
    changes: &[(String, bool)],
) -> Vec<(usize, usize)> {
    groups
        .iter()
        .enumerate()
        .filter_map(|(index, group)| {
            if index == source {
                return None;
            }
            let count = changes
                .iter()
                .filter(|(key, _)| group.members.contains(key))
                .count();
            (count > 0).then_some((index, count))
        })
        .collect()
}

pub fn overlaps(groups: &[Group], source: usize) -> Vec<(usize, usize)> {
    let Some(group) = groups.get(source) else {
        return Vec::new();
    };
    groups
        .iter()
        .enumerate()
        .filter_map(|(index, other)| {
            let count = group.members.intersection(&other.members).count();
            (index != source && count > 0).then_some((index, count))
        })
        .collect()
}

pub fn validate_name(
    name: &str,
    groups: &[Group],
    editing: Option<usize>,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        return Err("组名应为 1–40 个字符，不能包含换行或控制字符".into());
    }
    if name == "全部模组"
        || groups
            .iter()
            .enumerate()
            .any(|(i, g)| Some(i) != editing && g.name.to_lowercase() == name.to_lowercase())
    {
        return Err("这个组名已存在，请换一个名称".into());
    }
    Ok(name.into())
}

pub fn path() -> PathBuf {
    std::env::var_os("ADDON_WRENCH_GROUPS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::discovery::settings_path().with_file_name("groups.json"))
}

fn key(game: &Path) -> String {
    game.to_string_lossy().replace('/', "\\").to_lowercase()
}
fn read(path: &Path) -> Result<BTreeMap<String, Vec<Group>>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("分组文件无法读取：{e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(format!("分组文件无法读取：{e}")),
    }
}
pub fn load(path: &Path, game: &Path) -> Result<Vec<Group>, String> {
    if path == crate::discovery::data_dir().join("groups.json") {
        crate::discovery::migrate_legacy_file("groups.json", path)?;
    }
    Ok(read(path)?.remove(&key(game)).unwrap_or_default())
}
pub fn save(path: &Path, game: &Path, before: &[Group], after: &[Group]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let lock_path = path.with_extension("lock");
    let lock = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| format!("分组正在被其他实例修改，或无法写入：{e}"))?;
    let result = (|| {
        let mut all = read(path)?;
        if all.get(&key(game)).map(Vec::as_slice).unwrap_or_default() != before {
            return Err("分组已被其他实例修改，请刷新列表后重试".into());
        }
        all.insert(key(game), after.to_vec());
        let bytes = serde_json::to_vec_pretty(&all).map_err(|e| e.to_string())?;
        let temp = path.with_extension("tmp");
        fs::write(&temp, bytes).map_err(|e| e.to_string())?;
        fs::rename(&temp, path).map_err(|e| format!("分组保存失败：{e}"))
    })();
    drop(lock);
    let _ = fs::remove_file(lock_path);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_switch_handles_empty_missing_mixed_and_enabled_groups() {
        let mut group = Group {
            name: "Test".into(),
            members: ["a".into(), "b".into(), "missing".into()].into(),
        };
        let addon = |key: &str, enabled| crate::addons::Addon {
            key: key.into(),
            name: key.into(),
            path: key.into(),
            bytes: 0,
            modified: None,
            workshop: false,
            enabled,
            metadata: Default::default(),
        };
        assert_eq!(group.toggle_target(&[]), None);
        assert_eq!(group.toggle_target(&[addon("outside", true)]), None);
        assert_eq!(
            group.toggle_target(&[addon("a", true), addon("b", false)]),
            Some(true)
        );
        assert_eq!(
            group.toggle_target(&[addon("a", false), addon("b", false)]),
            Some(true)
        );
        assert_eq!(
            group.toggle_target(&[addon("a", true), addon("b", true)]),
            Some(false)
        );
        // Missing members do not prevent turning off all installed members.
        assert_eq!(group.toggle_target(&[addon("a", true)]), Some(false));
        group.members.clear();
        assert_eq!(group.toggle_target(&[addon("a", true)]), None);
    }

    #[test]
    fn overlap_impact_counts_only_actual_changes_and_excludes_the_source_group() {
        let group = |name: &str, keys: &[&str]| Group {
            name: name.into(),
            members: keys.iter().map(|key| (*key).into()).collect(),
        };
        let groups = vec![
            group("A", &["one", "two", "missing"]),
            group("B", &["one", "two"]),
            group("C", &["other"]),
        ];
        assert_eq!(overlaps(&groups, 0), vec![(1, 2)]);
        assert_eq!(
            affected_groups(&groups, 0, &[("one".into(), false)]),
            vec![(1, 1)]
        );
        assert!(affected_groups(&groups, 0, &[]).is_empty());
    }
    #[test]
    fn whole_group_ignores_search_and_preserves_outside_mods() {
        use crate::{addons, browser::Browser};
        let root = std::env::temp_dir().join(format!("aw-group-toggle-{}", std::process::id()));
        fs::create_dir_all(root.join("addons")).unwrap();
        for key in ["a.vpk", "b.vpk", "outside.vpk"] {
            fs::write(root.join("addons").join(key), b"fixture").unwrap();
        }
        let all = addons::scan(&root).unwrap();
        let group = Group {
            name: "测试".into(),
            members: ["a.vpk".into(), "b.vpk".into(), "missing.vpk".into()].into(),
        };
        let browser = Browser {
            query: "a.vpk".into(),
            members: Some(group.members.iter().cloned().collect()),
            ..Default::default()
        };
        assert_eq!(browser.visible(&all).len(), 1);
        let changes = group.changes(&all, false);
        assert_eq!(changes.len(), 2);
        addons::set_enabled(&root, &changes).unwrap();
        let updated = addons::scan(&root).unwrap();
        assert!(
            updated
                .iter()
                .all(|a| a.enabled == (a.key == "outside.vpk"))
        );
        assert!(group.changes(&updated, false).is_empty());
        assert_eq!(group.changes(&updated, true).len(), 2);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn names_and_persistence_keep_games_separate_and_reject_stale_writes() {
        let dir = std::env::temp_dir().join(format!("aw-groups-{}", std::process::id()));
        let path = dir.join("groups.json");
        let a = Path::new("A");
        let b = Path::new("B");
        let group = Group {
            name: "武器".into(),
            members: ["123.vpk".into(), "workshop/456.vpk".into()].into(),
        };
        assert!(validate_name(" ", &[], None).is_err());
        let groups = std::slice::from_ref(&group);
        assert!(validate_name("武器", groups, None).is_err());
        assert!(validate_name("武器", groups, Some(0)).is_ok());
        save(&path, a, &[], groups).unwrap();
        assert_eq!(load(&path, a).unwrap(), groups);
        assert!(load(&path, b).unwrap().is_empty());
        save(&path, b, &[], groups).unwrap();
        assert!(save(&path, a, &[], &[]).is_err());
        save(&path, a, groups, &[]).unwrap();
        assert_eq!(load(&path, b).unwrap(), [group]);
        fs::write(&path, b"corrupt").unwrap();
        assert!(save(&path, a, &[], &[]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"corrupt");
        fs::remove_dir_all(dir).unwrap();
    }
}
