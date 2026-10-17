//! Explicit local-file associations, kept separately from disposable Workshop data.
use addon_wrench::{
    addons::Addon,
    cache::{Cache, fingerprint},
};
use std::path::{Path, PathBuf};

pub fn path() -> PathBuf {
    addon_wrench::discovery::data_dir().join("workshop-links.sqlite3")
}

fn key(addon: &Addon) -> Option<String> {
    let path = addon
        .path
        .canonicalize()
        .ok()?
        .to_string_lossy()
        .into_owned();
    Some(if cfg!(windows) {
        path.to_lowercase()
    } else {
        path
    })
}

pub fn attach(path: &Path, addons: &mut [Addon]) {
    if !path.is_file() {
        return;
    }
    let Ok(cache) = Cache::open(path) else {
        return;
    };
    for addon in addons {
        if let Some((key, stamp)) = key(addon).zip(fingerprint(&addon.path))
            && let Some((id, _)) = cache.get(&key, &stamp)
            && let Ok(id) = String::from_utf8(id)
            && super::workshop::valid_id(&id)
        {
            addon.metadata.workshop_id = Some(id);
        }
    }
}

pub fn save(path: &Path, addon: &Addon, stamp: &str) -> Result<(), String> {
    let id = addon
        .metadata
        .workshop_id
        .as_deref()
        .filter(|id| super::workshop::valid_id(id))
        .ok_or("工坊 ID 无效")?;
    if fingerprint(&addon.path).as_deref() != Some(stamp) {
        return Err("模组文件已变化，请刷新后重试".into());
    }
    Cache::open(path)?.put(
        &key(addon).ok_or("无法定位模组文件")?,
        stamp,
        id.as_bytes(),
        &[],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn associations_survive_restart_and_reject_replaced_files() {
        let root = std::env::temp_dir().join(format!(
            "aw-links-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("local.vpk");
        std::fs::write(&file, b"original").unwrap();
        let mut addons = vec![Addon {
            path: file.clone(),
            key: "local.vpk".into(),
            name: "local".into(),
            bytes: 8,
            modified: None,
            enabled: false,
            workshop: false,
            metadata: Default::default(),
        }];
        addons[0].metadata.workshop_id = Some("123".into());
        let db = root.join("links.sqlite3");
        let stamp = fingerprint(&file).unwrap();
        save(&db, &addons[0], &stamp).unwrap();
        addons[0].metadata.workshop_id = None;
        attach(&db, &mut addons);
        assert_eq!(addons[0].metadata.workshop_id.as_deref(), Some("123"));
        std::fs::write(&file, b"replacement file").unwrap();
        addons[0].metadata.workshop_id = None;
        attach(&db, &mut addons);
        assert!(addons[0].metadata.workshop_id.is_none());
        assert!(save(&db, &addons[0], &stamp).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
