use crate::keyvalues::tokenize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Accept a content directory directly; installation roots must resolve unambiguously.
pub fn game_dir(path: &Path) -> Result<PathBuf, String> {
    if path.join("gameinfo.txt").is_file() {
        return path.canonicalize().map_err(|e| e.to_string());
    }
    let candidates = content_dirs(path);
    match candidates.as_slice() {
        [candidate] => candidate.canonicalize().map_err(|e| e.to_string()),
        [] => Err("未找到 gameinfo.txt，请选择游戏内容目录".into()),
        _ => Err("目录包含多个游戏内容目录，请直接选择需要的子目录".into()),
    }
}

pub fn content_dirs(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if root.join("gameinfo.txt").is_file() {
        result.push(root.to_path_buf());
    } else if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.path().join("gameinfo.txt").is_file() {
                result.push(entry.path());
            }
        }
    }
    result.sort();
    result
}

pub fn settings_path() -> PathBuf {
    data_dir().join("game-path.txt")
}

/// Resolve relative to the executable, independent of the launch directory.
pub fn data_dir() -> PathBuf {
    env::current_exe()
        .expect("无法确定程序路径")
        .parent()
        .expect("程序路径没有父目录")
        .join("data")
}

/// Preserve existing settings when moving storage next to the executable.
pub fn migrate_legacy_file(name: &str, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Ok(());
    }
    let legacy = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .unwrap_or_else(env::temp_dir)
        .join("AddonWrench")
        .join(name);
    if !legacy.is_file() {
        return Ok(());
    }
    let bytes = fs::read(legacy).map_err(|e| format!("旧设置无法读取：{e}"))?;
    let parent = destination.parent().ok_or("设置路径没有父目录")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(
        ".migration-{}-{}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    let written = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = written.and_then(|_| match fs::hard_link(&temporary, destination) {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        result => result,
    });
    let _ = fs::remove_file(temporary);
    result.map_err(|e| format!("旧设置迁移失败：{e}"))
}

pub fn remember(path: &Path) -> Result<(), String> {
    let settings = settings_path();
    fs::create_dir_all(settings.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(settings, path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())
}

pub fn discover() -> Option<PathBuf> {
    crate::games::catalog().ok()?.selected
}

pub fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if let Ok(output) = std::process::Command::new("reg.exe")
            .args(["query", "HKCU\\Software\\Valve\\Steam", "/v", "SteamPath"])
            .creation_flags(0x08000000)
            .output()
        {
            let output = String::from_utf8_lossy(&output.stdout);
            if let Some(path) = output
                .lines()
                .find_map(|l| l.split_once("REG_SZ").map(|(_, p)| p.trim()))
            {
                roots.push(PathBuf::from(path));
            }
        }
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(path) = env::var_os(var) {
                roots.push(PathBuf::from(path).join("Steam"));
            }
        }
        for drive in ['C', 'D', 'E', 'F'] {
            roots.push(PathBuf::from(format!("{drive}:\\SteamLibrary")));
        }
    }
    if let Some(home) = env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join(".local/share/Steam"));
        roots.push(PathBuf::from(home).join(".steam/steam"));
    }
    for root in roots.clone() {
        if let Ok(text) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf"))
            && let Ok(tokens) = tokenize(&text)
        {
            for pair in tokens.windows(2) {
                if pair[0].text == "path" && !pair[1].brace {
                    roots.push(PathBuf::from(&pair[1].text));
                }
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}
