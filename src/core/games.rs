//! Game identity and capabilities. Only explicitly implemented adapters may write game files.
use crate::{addons, discovery, keyvalues::tokenize};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adapter {
    L4d2,
    BrowseOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub path: PathBuf,
    pub name: String,
    pub app_id: Option<u32>,
    pub adapter: Adapter,
}

impl Game {
    pub fn launch_url(&self) -> Option<String> {
        self.app_id
            .filter(|id| *id > 0)
            .map(|id| format!("steam://rungameid/{id}"))
    }

    pub fn can_manage(&self) -> bool {
        self.adapter == Adapter::L4d2
    }
    pub fn workshop_app(&self) -> Option<u32> {
        // Workshop association is part of the adapter, not implied by any Steam App ID.
        if self.adapter == Adapter::L4d2 {
            self.app_id
        } else {
            None
        }
    }
    pub fn scan(&self) -> Result<Vec<addons::Addon>, String> {
        match self.adapter {
            Adapter::L4d2 => addons::scan(&self.path),
            Adapter::BrowseOnly => addons::scan_readonly(&self.path),
        }
    }
    pub fn set_enabled(&self, changes: &[(String, bool)]) -> Result<(), String> {
        if !self.can_manage() {
            return Err("此游戏尚未适配启用操作，仅支持浏览模组".into());
        }
        addons::set_enabled(&self.path, changes)
    }
    pub fn import(&self, source: &Path) -> Result<PathBuf, String> {
        if !self.can_manage() {
            return Err("此游戏尚未适配安装操作，仅支持浏览模组".into());
        }
        addons::import(&self.path, source)
    }
}

fn value(text: &str, key: &str) -> Option<String> {
    let tokens = tokenize(text).ok()?;
    tokens
        .windows(2)
        .find(|pair| pair[0].text.eq_ignore_ascii_case(key) && !pair[0].brace && !pair[1].brace)
        .map(|pair| pair[1].text.clone())
}

pub fn identify(path: &Path) -> Result<Game, String> {
    let path = discovery::game_dir(path)?;
    let info = fs::read_to_string(path.join("gameinfo.txt"))
        .map_err(|e| format!("无法读取游戏信息：{e}"))?;
    let mut app_id = value(&info, "SteamAppId")
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|id| *id > 0);
    let mut name = value(&info, "game").filter(|s| !s.trim().is_empty());
    // A manifest identifies the actual installed game, even when gameinfo mounts a base game's content.
    for ancestor in path.ancestors().skip(1).take(4) {
        if ancestor
            .file_name()
            .is_some_and(|s| s.eq_ignore_ascii_case("steamapps"))
        {
            let install = path
                .strip_prefix(ancestor.join("common"))
                .ok()
                .and_then(|p| p.components().next())
                .map(|c| c.as_os_str().to_string_lossy().into_owned());
            if let (Some(install), Ok(entries)) = (install, fs::read_dir(ancestor)) {
                for entry in entries.flatten() {
                    let filename = entry.file_name().to_string_lossy().into_owned();
                    if !filename.starts_with("appmanifest_") || !filename.ends_with(".acf") {
                        continue;
                    }
                    if let Ok(text) = fs::read_to_string(entry.path())
                        && value(&text, "installdir")
                            .is_some_and(|dir| dir.eq_ignore_ascii_case(&install))
                    {
                        app_id = value(&text, "appid")
                            .and_then(|s| s.parse().ok())
                            .filter(|id| *id > 0);
                        name = value(&text, "name").or(name);
                        break;
                    }
                }
            }
            break;
        }
    }
    if app_id.is_none() {
        for root in [path.as_path(), path.parent().unwrap_or(&path)] {
            if let Ok(text) = fs::read_to_string(root.join("steam_appid.txt")) {
                app_id = text.trim().parse().ok().filter(|id| *id > 0);
                if app_id.is_some() {
                    break;
                }
            }
        }
    }
    let l4d2_directory = path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("left4dead2"))
        && path.join("addons").is_dir();
    // Keep copied/development L4D2 content directories usable when Steam metadata is absent.
    // An explicit ID for a different game always takes precedence over the directory name.
    if app_id.is_none() && l4d2_directory {
        app_id = Some(550);
    }
    let l4d2 = app_id == Some(550) && l4d2_directory;
    Ok(Game {
        name: name.unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        }),
        path,
        app_id,
        adapter: if l4d2 {
            Adapter::L4d2
        } else {
            Adapter::BrowseOnly
        },
    })
}

pub fn scan_path(path: &Path) -> Result<Vec<addons::Addon>, String> {
    identify(path)?.scan()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub paths: Vec<PathBuf>,
    pub selected: Option<PathBuf>,
}
impl Library {
    pub fn add(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        if !self.paths.contains(&path) {
            self.paths.push(path);
        }
    }
}
pub fn path() -> PathBuf {
    discovery::data_dir().join("games.json")
}
pub fn load(path: &Path) -> Result<Library, String> {
    match fs::read(path) {
        Ok(bytes) => {
            let library: Library =
                serde_json::from_slice(&bytes).map_err(|e| format!("游戏列表损坏：{e}"))?;
            let mut normalized = Library::default();
            for entry in library.paths {
                normalized.add(entry);
            }
            normalized.selected = library
                .selected
                .map(|entry| entry.canonicalize().unwrap_or(entry));
            Ok(normalized)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Library::default()),
        Err(e) => Err(format!("无法读取游戏列表：{e}")),
    }
}
pub fn save(path: &Path, library: &Library) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let lock_path = path.with_extension("lock");
    let lock = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| format!("游戏列表正在被修改：{e}"))?;
    let result = (|| {
        // Merge remembered directories so a second instance cannot erase newly added games.
        let mut merged = load(path)?;
        for entry in &library.paths {
            merged.add(entry.clone());
        }
        merged.selected = library.selected.clone();
        let bytes = serde_json::to_vec_pretty(&merged).map_err(|e| e.to_string())?;
        let temp = path.with_extension("tmp");
        fs::write(&temp, bytes).map_err(|e| e.to_string())?;
        fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    drop(lock);
    let _ = fs::remove_file(lock_path);
    result
}

pub fn discover_in_roots(roots: &[PathBuf]) -> Vec<Game> {
    let mut games = Vec::new();
    for root in roots {
        let steamapps = root.join("steamapps");
        if let Ok(manifests) = fs::read_dir(&steamapps) {
            for manifest in manifests.flatten() {
                let filename = manifest.file_name().to_string_lossy().into_owned();
                if !filename.starts_with("appmanifest_") || !filename.ends_with(".acf") {
                    continue;
                }
                let Ok(text) = fs::read_to_string(manifest.path()) else {
                    continue;
                };
                let Some(app_id) = value(&text, "appid")
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|id| *id > 0)
                else {
                    continue;
                };
                if value(&text, "StateFlags")
                    .and_then(|s| s.parse::<u32>().ok())
                    .is_none_or(|flags| flags & 4 == 0)
                {
                    continue;
                }
                let Some(install) = value(&text, "installdir") else {
                    continue;
                };
                let mut components = Path::new(&install).components();
                if !matches!(components.next(), Some(std::path::Component::Normal(_)))
                    || components.next().is_some()
                {
                    continue;
                }
                let candidates = discovery::content_dirs(&steamapps.join("common").join(install));
                // Multiple content directories often include a bundled base game; list one entry per installation.
                let primary = candidates
                    .iter()
                    .find(|path| {
                        fs::read_to_string(path.join("gameinfo.txt"))
                            .ok()
                            .and_then(|info| value(&info, "SteamAppId"))
                            .and_then(|id| id.parse::<u32>().ok())
                            == Some(app_id)
                    })
                    .or_else(|| {
                        candidates.iter().find(|path| {
                            app_id == 550
                                && path
                                    .file_name()
                                    .is_some_and(|name| name.eq_ignore_ascii_case("left4dead2"))
                        })
                    })
                    .or_else(|| candidates.first());
                if let Some(primary) = primary
                    && let Ok(game) = identify(primary)
                {
                    games.push(game);
                }
            }
        }
    }
    games.sort_by(|a, b| {
        (!a.can_manage(), a.name.to_lowercase(), &a.path).cmp(&(
            !b.can_manage(),
            b.name.to_lowercase(),
            &b.path,
        ))
    });
    games.dedup_by(|a, b| a.path == b.path);
    games
}

pub fn catalog() -> Result<Library, String> {
    let remembered = load(&path())?;
    let mut library = Library {
        paths: Vec::new(),
        selected: remembered.selected,
    };
    let preferred = std::env::args_os()
        .nth(1)
        .or_else(|| std::env::var_os("ADDON_WRENCH_GAME_DIR"))
        .or_else(|| std::env::var_os("L4D2_DIR"))
        .and_then(|p| identify(Path::new(&p)).ok());
    for game in discover_in_roots(&discovery::steam_roots()) {
        library.add(game.path);
    }
    // The selected/manual development directory remains accessible through Settings or a launch argument.
    // Old, unselected directories do not become home page game choices.
    if let Some(selected) = library
        .selected
        .as_ref()
        .and_then(|path| identify(path).ok())
    {
        library.add(selected.path);
    }
    if let Some(game) = preferred {
        library.add(game.path.clone());
        library.selected = Some(game.path);
    }
    if library
        .selected
        .as_ref()
        .is_none_or(|p| identify(p).is_err())
    {
        library.selected = library.paths.iter().find(|p| identify(p).is_ok()).cloned();
    }
    Ok(library)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "aw-games-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn game(&self, folder: &str, app: u32) -> PathBuf {
            let path = self.0.join(folder);
            fs::create_dir_all(path.join("addons")).unwrap();
            fs::write(
                path.join("gameinfo.txt"),
                format!("\"GameInfo\" {{ game \"Test {app}\" FileSystem {{ SteamAppId {app} }} }}"),
            )
            .unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn launch_requires_a_valid_game_app_id_but_not_a_write_adapter() {
        let mut game = Game {
            path: "test".into(),
            name: "Test".into(),
            app_id: None,
            adapter: Adapter::BrowseOnly,
        };
        assert!(game.launch_url().is_none());
        game.app_id = Some(0);
        assert!(game.launch_url().is_none());
        game.app_id = Some(440);
        assert_eq!(game.launch_url().as_deref(), Some("steam://rungameid/440"));
    }

    #[test]
    fn resolves_install_roots_and_requires_an_explicit_adapter() {
        let fixture = Fixture::new();
        let l4d2 = fixture.game("install/left4dead2", 550);
        let game = identify(&fixture.0.join("install")).unwrap();
        assert_eq!(game.path, l4d2.canonicalize().unwrap());
        assert!(game.can_manage());
        assert_eq!(game.workshop_app(), Some(550));
        fixture.game("install/another", 440);
        assert!(identify(&fixture.0.join("install")).is_err());
        let unknown = identify(&fixture.0.join("install/another")).unwrap();
        assert!(!unknown.can_manage());
        assert_eq!(unknown.workshop_app(), None);
        assert!(
            unknown
                .set_enabled(&[("example.vpk".into(), false)])
                .is_err()
        );
        assert!(unknown.import(&l4d2.join("file.vpk")).is_err());
        assert!(!unknown.path.join("addonlist.txt").exists());
    }

    #[test]
    fn manifests_override_base_game_ids_and_discovery_keeps_multiple_games() {
        let fixture = Fixture::new();
        let root = &fixture.0;
        fixture.game("steamapps/common/Example/content", 550);
        fixture.game("steamapps/common/L4D2/left4dead2", 550);
        fs::write(
            root.join("steamapps/appmanifest_440.acf"),
            "AppState { appid 440 name \"Actual Game\" installdir Example StateFlags 4 }",
        )
        .unwrap();
        fs::write(
            root.join("steamapps/appmanifest_550.acf"),
            "AppState { appid 550 name L4D2 installdir L4D2 StateFlags 4 }",
        )
        .unwrap();
        let games = discover_in_roots(std::slice::from_ref(root));
        assert_eq!(games.len(), 2);
        assert!(games[0].can_manage());
        let other = games
            .iter()
            .find(|game| game.name == "Actual Game")
            .unwrap();
        assert_eq!(other.app_id, Some(440));
        assert_eq!(other.launch_url().as_deref(), Some("steam://rungameid/440"));
        assert!(!other.can_manage());
    }

    #[test]
    fn read_only_browsing_ignores_foreign_enable_config_and_preserves_files() {
        let fixture = Fixture::new();
        let path = fixture.game("content", 440);
        fs::create_dir_all(path.join("custom/package")).unwrap();
        fs::write(
            path.join("custom/package/test.vpk"),
            b"damaged but still visible",
        )
        .unwrap();
        fs::write(path.join("addonlist.txt"), b"unrelated config format").unwrap();
        let game = identify(&path).unwrap();
        let addons = game.scan().unwrap();
        assert_eq!(addons.len(), 1);
        assert_eq!(addons[0].key, "custom/package/test.vpk");
        assert_eq!(
            fs::read(path.join("addonlist.txt")).unwrap(),
            b"unrelated config format"
        );
        assert!(!path.join(".addonwrench.lock").exists());
    }

    #[test]
    fn library_merges_games_and_never_overwrites_corruption_or_an_existing_lock() {
        let fixture = Fixture::new();
        let path = fixture.0.join("games.json");
        let a = Library {
            paths: vec!["a".into()],
            selected: Some("a".into()),
        };
        let b = Library {
            paths: vec!["b".into()],
            selected: Some("b".into()),
        };
        save(&path, &a).unwrap();
        save(&path, &b).unwrap();
        let saved = load(&path).unwrap();
        assert_eq!(saved.paths.len(), 2);
        assert_eq!(saved.selected, b.selected);
        fs::write(path.with_extension("lock"), "lock").unwrap();
        assert!(save(&path, &a).is_err());
        assert_eq!(load(&path).unwrap(), saved);
        fs::remove_file(path.with_extension("lock")).unwrap();
        fs::write(&path, b"broken").unwrap();
        assert!(save(&path, &a).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken");
    }

    #[test]
    fn l4d2_adapter_imports_and_changes_only_its_own_configuration() {
        let fixture = Fixture::new();
        let a = fixture.game("A/left4dead2", 550);
        let b = fixture.game("B/left4dead2", 550);
        let source = fixture.0.join("example.vpk");
        let mut bytes = Vec::new();
        bytes.extend(0x55aa1234u32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.push(0);
        fs::write(&source, bytes).unwrap();
        let game = identify(&a).unwrap();
        game.import(&source).unwrap();
        assert_eq!(game.scan().unwrap().len(), 1);
        game.set_enabled(&[("example.vpk".into(), false)]).unwrap();
        assert!(!game.scan().unwrap()[0].enabled);
        assert!(!b.join("addons/example.vpk").exists());
        assert!(!b.join("addonlist.txt").exists());
        assert_eq!(identify(&b).unwrap().scan().unwrap().len(), 0);
    }

    #[test]
    fn library_normalizes_existing_directory_aliases_before_display_and_save() {
        let fixture = Fixture::new();
        let content = fixture.game("content", 440);
        let canonical = content.canonicalize().unwrap();
        let file = fixture.0.join("games.json");
        let old = Library {
            paths: vec![content.clone(), content.join(".")],
            selected: Some(content),
        };
        fs::write(&file, serde_json::to_vec(&old).unwrap()).unwrap();
        let library = load(&file).unwrap();
        assert_eq!(library.paths, vec![canonical.clone()]);
        assert_eq!(library.selected, Some(canonical));
        save(&file, &library).unwrap();
        assert_eq!(load(&file).unwrap(), library);
    }

    #[test]
    fn copied_l4d2_directory_without_app_metadata_still_reads_and_updates_enabled_state() {
        let fixture = Fixture::new();
        let path = fixture.game("copied/left4dead2", 550);
        fs::write(path.join("gameinfo.txt"), "").unwrap();
        fs::write(path.join("addons/example.vpk"), b"test fixture").unwrap();
        let game = identify(&path).unwrap();
        assert!(game.can_manage());
        assert_eq!(game.app_id, Some(550));
        assert_eq!(game.workshop_app(), Some(550));
        game.set_enabled(&[("example.vpk".into(), false)]).unwrap();
        assert!(!game.scan().unwrap()[0].enabled);
        let other = fixture.game("other/left4dead2", 440);
        assert!(!identify(&other).unwrap().can_manage());
    }

    #[test]
    fn discovery_requires_installed_manifests_and_avoids_bundled_base_game_duplicates() {
        let fixture = Fixture::new();
        fixture.game("steamapps/common/Ready/left4dead2", 550);
        fixture.game("steamapps/common/Ready/hl2", 215);
        fixture.game("steamapps/common/Orphan/content", 440);
        fixture.game("steamapps/common/Queued/content", 440);
        fs::write(
            fixture.0.join("steamapps/appmanifest_550.acf"),
            "AppState { appid 550 name L4D2 installdir Ready StateFlags 4 }",
        )
        .unwrap();
        fs::write(
            fixture.0.join("steamapps/appmanifest_440.acf"),
            "AppState { appid 440 name Queued installdir Queued StateFlags 1024 }",
        )
        .unwrap();
        let games = discover_in_roots(std::slice::from_ref(&fixture.0));
        assert_eq!(games.len(), 1);
        assert!(games[0].can_manage());
        assert!(games[0].path.ends_with("left4dead2"));
    }
}
