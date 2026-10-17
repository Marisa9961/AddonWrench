use crate::keyvalues::AddonList;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub type Result<T> = std::result::Result<T, String>;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct Addon {
    pub key: String,
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub modified: Option<std::time::SystemTime>,
    pub workshop: bool,
    pub enabled: bool,
    pub metadata: crate::metadata::Metadata,
}

fn read_list(game: &Path) -> Result<Option<String>> {
    match fs::read_to_string(game.join("addonlist.txt")) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("无法读取 addonlist.txt：{e}")),
    }
}

pub fn scan(game: &Path) -> Result<Vec<Addon>> {
    scan_with_cache(game, &crate::cache::Cache::default_path())
}

pub fn scan_with_cache(game: &Path, cache_path: &Path) -> Result<Vec<Addon>> {
    scan_sources(
        game,
        cache_path,
        true,
        &[
            (game.join("addons"), false),
            (game.join("addons/workshop"), true),
        ],
    )
}

/// Inspect custom VPKs without assuming that their runtime enabled state is known.
pub fn scan_readonly(game: &Path) -> Result<Vec<Addon>> {
    let mut sources = vec![
        (game.join("addons"), false),
        (game.join("addons/workshop"), true),
        (game.join("custom"), false),
    ];
    match fs::read_dir(game.join("custom")) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    sources.push((entry.path(), false));
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法读取 custom 目录：{error}")),
    }
    sources.sort_by(|a, b| a.0.cmp(&b.0));
    scan_sources(game, &crate::cache::Cache::default_path(), false, &sources)
}

fn scan_sources(
    game: &Path,
    cache_path: &Path,
    managed: bool,
    sources: &[(PathBuf, bool)],
) -> Result<Vec<Addon>> {
    if !fs::metadata(game)
        .map_err(|e| format!("无法读取游戏目录：{e}"))?
        .is_dir()
    {
        return Err("游戏目录无效".into());
    }
    let text = (if managed { read_list(game)? } else { None })
        .unwrap_or_else(|| "\"AddonList\"\n{\n}\n".into());
    let list = AddonList::parse(&text)?;
    // Cache failures must not prevent users from managing their installed addons.
    let cache = crate::cache::Cache::open(cache_path).ok();
    let transaction = cache.as_ref().is_some_and(|c| c.begin().is_ok());
    let mut addons = Vec::new();
    for (folder, workshop) in sources {
        let workshop = *workshop;
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(e) if (workshop || !managed) && e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("无法读取 {}：{e}", folder.display())),
        };
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                continue;
            }
            let path = entry.path();
            if !is_vpk(&path) || is_chunk(&path) {
                continue;
            }
            let filename = entry
                .file_name()
                .into_string()
                .map_err(|_| "存在非 UTF-8 文件名")?;
            let key = if !managed {
                path.strip_prefix(game)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/")
            } else if workshop {
                format!("workshop/{filename}")
            } else {
                filename.clone()
            };
            let source_stamp = crate::cache::fingerprint(&path);
            let fingerprint = source_stamp
                .as_ref()
                .map(|s| format!("{s}:{}:{workshop}", cfg!(feature = "desktop")));
            let cache_key = format!("local:{}", path.to_string_lossy());
            let cached = cache
                .as_ref()
                .zip(fingerprint.as_ref())
                .and_then(|(cache, stamp)| {
                    let (payload, cover) = cache.get(&cache_key, stamp)?;
                    let mut metadata: crate::metadata::Metadata =
                        serde_json::from_slice(&payload).ok()?;
                    metadata.cover = (!cover.is_empty()).then(|| std::sync::Arc::new(cover));
                    Some(metadata)
                });
            let metadata = cached.unwrap_or_else(|| {
                let metadata = crate::metadata::read(&path, workshop);
                #[cfg(feature = "desktop")]
                let metadata = {
                    let mut metadata = metadata;
                    crate::metadata::prepare_cover(&mut metadata);
                    metadata
                };
                if transaction
                    && let Some((cache, stamp)) = cache.as_ref().zip(fingerprint.as_ref())
                    && crate::cache::fingerprint(&path) == source_stamp
                    && let Ok(payload) = serde_json::to_vec(&metadata)
                {
                    let _ = cache.put(
                        &cache_key,
                        stamp,
                        &payload,
                        metadata
                            .cover
                            .as_deref()
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                    );
                }
                metadata
            });
            let file_metadata = entry.metadata().map_err(|e| e.to_string())?;
            let mut addon = Addon {
                enabled: list.enabled(&key),
                key,
                name: metadata.title.clone().unwrap_or_else(|| filename.clone()),
                bytes: file_metadata.len(),
                modified: file_metadata.modified().ok(),
                path,
                workshop,
                metadata,
            };
            if !crate::metadata::has_local_title(&addon) {
                addon.name = filename;
            }
            addons.push(addon);
        }
    }
    addons.sort_by_cached_key(|a| (a.workshop, a.name.to_lowercase()));
    if transaction && let Some(cache) = &cache {
        let live = addons
            .iter()
            .map(|addon| format!("local:{}", addon.path.to_string_lossy()))
            .collect();
        if cache.prune_missing_local(game, &live).is_ok() {
            let _ = cache.commit();
        } else {
            let _ = cache.rollback();
        }
    }
    Ok(addons)
}

fn is_vpk(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("vpk"))
}

fn is_chunk(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.rsplit_once('_'))
        .is_some_and(|(base, tail)| {
            tail.len() == 3
                && tail.bytes().all(|b| b.is_ascii_digit())
                && path.with_file_name(format!("{base}_dir.vpk")).exists()
        })
}

fn unique_file(directory: &Path, prefix: &str) -> Result<(PathBuf, File)> {
    for _ in 0..100 {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = directory.join(format!(
            "{prefix}-{stamp}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("无法创建临时文件".into())
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Reread before editing; preserve comments and unlisted/Workshop entries.
/// The game does not participate in our file lock; check again before publishing.
pub fn set_enabled(game: &Path, changes: &[(String, bool)]) -> Result<()> {
    crate::runtime::ensure_stopped(game)?;
    let lock_path = game.join(".addonwrench.lock");
    let lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| format!("无法锁定配置（若上次异常退出，请检查 .addonwrench.lock）：{e}"))?;
    let _cleanup = Cleanup(lock_path);
    // Drop the handle before Cleanup on Windows.
    drop(lock);
    let old = read_list(game)?;
    let original = old.as_deref().unwrap_or("\"AddonList\"\n{\n}\n");
    let list = AddonList::parse(original)?;
    let updated = list.update(original, changes)?;
    if updated == original {
        return Ok(());
    }
    let (temporary, mut file) = unique_file(game, ".addonlist.tmp")?;
    let _temp_cleanup = Cleanup(temporary.clone());
    file.write_all(updated.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    if read_list(game)? != old {
        return Err("配置已被游戏或其他程序修改，请刷新后重试".into());
    }
    crate::runtime::ensure_stopped(game)?;
    fs::rename(&temporary, game.join("addonlist.txt")).map_err(|e| format!("无法保存配置：{e}"))?;
    Ok(())
}

/// Validate the directory tree and require all payloads to be in this single VPK.
/// CRC verification is intentionally outside this lightweight structural check.
pub fn validate_vpk(path: &Path) -> Result<()> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    let mut header = [0u8; 12];
    file.read_exact(&mut header)
        .map_err(|_| "VPK 文件头不完整")?;
    let word = |offset| u32::from_le_bytes(header[offset..offset + 4].try_into().unwrap());
    if word(0) != 0x55aa1234 {
        return Err("不是有效的 VPK 文件".into());
    }
    let version = word(4);
    let header_size = match version {
        1 => 12u64,
        2 => 28u64,
        _ => return Err("仅支持 VPK v1 / v2".into()),
    };
    let tree_size = word(8) as usize;
    if tree_size == 0 || tree_size > 64 * 1024 * 1024 || header_size + tree_size as u64 > length {
        return Err("VPK 目录树大小无效".into());
    }
    let payload_size = if version == 2 {
        let mut extra = [0u8; 16];
        file.read_exact(&mut extra).map_err(|e| e.to_string())?;
        let data_size = u32::from_le_bytes(extra[0..4].try_into().unwrap()) as u64;
        let total_sections: u64 = extra
            .as_chunks::<4>()
            .0
            .iter()
            .map(|x| u32::from_le_bytes(*x) as u64)
            .sum();
        if header_size + tree_size as u64 + total_sections > length {
            return Err("VPK v2 数据段被截断".into());
        }
        data_size
    } else {
        length - header_size - tree_size as u64
    };
    let mut tree = vec![0; tree_size];
    file.read_exact(&mut tree).map_err(|e| e.to_string())?;
    let mut cursor = 0;
    while !cstring(&tree, &mut cursor)?.is_empty() {
        while !cstring(&tree, &mut cursor)?.is_empty() {
            while !cstring(&tree, &mut cursor)?.is_empty() {
                let entry = tree.get(cursor..cursor + 18).ok_or("VPK 目录项被截断")?;
                let preload = u16::from_le_bytes(entry[4..6].try_into().unwrap()) as usize;
                let archive = u16::from_le_bytes(entry[6..8].try_into().unwrap());
                let offset = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as u64;
                let size = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as u64;
                if entry[16..18] != [255, 255] {
                    return Err("VPK 目录项结束标记无效".into());
                }
                if size > 0 && archive != 0x7fff {
                    return Err("暂不支持导入分卷 VPK，请使用单文件 VPK".into());
                }
                if size > 0 && offset + size > payload_size {
                    return Err("VPK 数据被截断".into());
                }
                cursor += 18 + preload;
                if cursor > tree.len() {
                    return Err("VPK 预载数据被截断".into());
                }
            }
        }
    }
    if cursor != tree.len() {
        return Err("VPK 目录树存在无效尾部".into());
    }
    Ok(())
}

fn cstring<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<&'a [u8]> {
    let rest = bytes.get(*cursor..).ok_or("VPK 字符串越界")?;
    let end = rest
        .iter()
        .position(|b| *b == 0)
        .ok_or("VPK 字符串未终止")?;
    *cursor += end + 1;
    Ok(&rest[..end])
}

pub fn import(game: &Path, source: &Path) -> Result<PathBuf> {
    crate::runtime::ensure_stopped(game)?;
    if !is_vpk(source) {
        return Err("请选择 .vpk 文件".into());
    }
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("文件名不是有效 UTF-8")?;
    if name.contains(['"', '\r', '\n']) {
        return Err("文件名包含不支持的字符".into());
    }
    let folder = game.join("addons");
    let destination = folder.join(name);
    if destination.exists() {
        return Err(format!("{name} 已存在，未覆盖"));
    }
    validate_vpk(source)?;
    let (temporary, mut out) = unique_file(&folder, ".import.tmp")?;
    let _cleanup = Cleanup(temporary.clone());
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    io::copy(&mut input, &mut out)
        .and_then(|_| out.sync_all())
        .map_err(|e| e.to_string())?;
    drop(out);
    validate_vpk(&temporary)?;
    // Publishing via a hard link is atomic and cannot overwrite an existing name.
    crate::runtime::ensure_stopped(game)?;
    fs::hard_link(&temporary, &destination)
        .map_err(|e| format!("无法安装 {name}（目标文件系统需支持硬链接）：{e}"))?;
    Ok(destination)
}

pub fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "addonwrench-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(path.join("addons/workshop")).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn vpk() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(0x55aa1234u32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.push(0);
        bytes
    }
    #[test]
    fn scans_both_sources_and_preserves_config_without_backup() {
        let f = Fixture::new();
        fs::write(f.0.join("addons").join("local.vpk"), vpk()).unwrap();
        fs::write(f.0.join("addons/workshop/123.VPK"), vpk()).unwrap();
        fs::write(f.0.join("addons/ignore.txt"), "x").unwrap();
        let original =
            "\"AddonList\" { // user comment\n\"workshop\\123.VPK\" \"0\" \"unknown.vpk\" \"1\" }";
        fs::write(f.0.join("addonlist.txt"), original).unwrap();
        let mods = scan(&f.0).unwrap();
        assert_eq!(mods.len(), 2);
        assert!(mods[0].enabled);
        assert!(!mods[1].enabled);
        set_enabled(&f.0, &[("local.vpk".into(), false)]).unwrap();
        assert!(!f.0.join("addonwrench-backups").exists());
        let updated = fs::read_to_string(f.0.join("addonlist.txt")).unwrap();
        assert!(updated.contains("// user comment"));
        assert!(updated.contains("\"unknown.vpk\" \"1\""));
        assert!(scan(&f.0).unwrap().iter().all(|a| !a.enabled));
        assert!(!f.0.join(".addonwrench.lock").exists());
    }

    #[test]
    fn sqlite_hits_refresh_state_and_invalidate_changed_sources() {
        let f = Fixture::new();
        let path = f.0.join("addons").join("local.vpk");
        let db = f.0.join("data/cache.sqlite3");
        fs::write(&path, vpk()).unwrap();
        let initial = scan_with_cache(&f.0, &db).unwrap();
        assert_eq!(initial[0].name, "local.vpk");
        assert!(!crate::metadata::has_resolved_title(&initial[0]));
        assert_eq!(&fs::read(&db).unwrap()[..16], b"SQLite format 3\0");
        // An observable sentinel proves unchanged files are served from SQLite.
        let mut metadata = initial[0].metadata.clone();
        metadata.title = Some("缓存命中".into());
        let key = format!("local:{}", path.to_string_lossy());
        let stamp = format!(
            "{}:{}:false",
            crate::cache::fingerprint(&path).unwrap(),
            cfg!(feature = "desktop")
        );
        let cache = crate::cache::Cache::open(&db).unwrap();
        cache
            .put(&key, &stamp, &serde_json::to_vec(&metadata).unwrap(), &[])
            .unwrap();
        drop(cache);
        fs::write(
            f.0.join("addonlist.txt"),
            "\"AddonList\" { \"local.vpk\" \"0\" }",
        )
        .unwrap();
        let warm = scan_with_cache(&f.0, &db).unwrap();
        assert_eq!(warm[0].name, "缓存命中");
        assert!(crate::metadata::has_resolved_title(&warm[0]));
        assert!(!warm[0].enabled);
        fs::write(path.with_extension("jpg"), b"added preview").unwrap();
        assert_eq!(scan_with_cache(&f.0, &db).unwrap()[0].name, "local.vpk");
        fs::write(&path, b"modified VPK").unwrap();
        assert_eq!(scan_with_cache(&f.0, &db).unwrap()[0].bytes, 12);
        fs::remove_file(path).unwrap();
        assert!(scan_with_cache(&f.0, &db).unwrap().is_empty());
    }

    #[test]
    fn complete_scans_prune_missing_cache_but_failures_and_other_games_preserve_it() {
        let a = Fixture::new();
        let b = Fixture::new();
        let db = a.0.join("data/cache.sqlite3");
        let path_a = a.0.join("addons/a.vpk");
        let path_b = b.0.join("addons/b.vpk");
        fs::write(&path_a, vpk()).unwrap();
        fs::write(&path_b, vpk()).unwrap();
        scan_with_cache(&a.0, &db).unwrap();
        scan_with_cache(&b.0, &db).unwrap();
        let key_a = format!("local:{}", path_a.to_string_lossy());
        let key_b = format!("local:{}", path_b.to_string_lossy());
        let cache = crate::cache::Cache::open(&db).unwrap();
        cache.put(&key_a, "test", b"a", b"cover").unwrap();
        cache.put(&key_b, "test", b"b", b"cover").unwrap();
        drop(cache);
        fs::remove_file(&path_a).unwrap();
        fs::remove_file(&path_b).unwrap();
        // A failure in the second source must preserve all existing records.
        fs::remove_dir(a.0.join("addons/workshop")).unwrap();
        fs::write(a.0.join("addons/workshop"), b"not a directory").unwrap();
        assert!(scan_with_cache(&a.0, &db).is_err());
        let cache = crate::cache::Cache::open(&db).unwrap();
        assert!(cache.get(&key_a, "test").is_some());
        drop(cache);
        fs::remove_file(a.0.join("addons/workshop")).unwrap();
        assert!(scan_with_cache(&a.0, &db).unwrap().is_empty());
        let cache = crate::cache::Cache::open(&db).unwrap();
        assert!(cache.get(&key_a, "test").is_none());
        assert!(cache.get(&key_b, "test").is_some());
        drop(cache);
        scan_with_cache(&b.0, &db).unwrap();
        assert!(
            crate::cache::Cache::open(&db)
                .unwrap()
                .get(&key_b, "test")
                .is_none()
        );
    }

    #[test]
    fn sqlite_corruption_falls_back_and_games_have_separate_entries() {
        let a = Fixture::new();
        let b = Fixture::new();
        for game in [&a.0, &b.0] {
            fs::write(game.join("addons").join("same.vpk"), vpk()).unwrap();
        }
        let db = a.0.join("data/cache.sqlite3");
        scan_with_cache(&a.0, &db).unwrap();
        scan_with_cache(&b.0, &db).unwrap();
        let cache = crate::cache::Cache::open(&db).unwrap();
        for game in [&a.0, &b.0] {
            let path = game.join("addons").join("same.vpk");
            let key = format!("local:{}", path.to_string_lossy());
            let stamp = format!(
                "{}:{}:false",
                crate::cache::fingerprint(&path).unwrap(),
                cfg!(feature = "desktop")
            );
            assert!(cache.get(&key, &stamp).is_some());
            cache.put(&key, &stamp, b"broken JSON", &[]).unwrap();
        }
        drop(cache);
        assert_eq!(scan_with_cache(&a.0, &db).unwrap().len(), 1);
        fs::write(&db, b"broken database").unwrap();
        assert_eq!(scan_with_cache(&a.0, &db).unwrap().len(), 1);
        let blocked = a.0.join("file-not-directory");
        fs::write(&blocked, b"file").unwrap();
        assert_eq!(
            scan_with_cache(&a.0, &blocked.join("cache.sqlite3"))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn external_chunk_changes_invalidate_fingerprint() {
        let f = Fixture::new();
        let path = f.0.join("addons").join("map_dir.vpk");
        let chunk = f.0.join("addons").join("map_000.vpk");
        fs::write(&path, vpk()).unwrap();
        let initial = crate::cache::fingerprint(&path).unwrap();
        fs::write(&chunk, b"external metadata").unwrap();
        let added = crate::cache::fingerprint(&path).unwrap();
        assert_ne!(initial, added);
        fs::write(&chunk, b"updated external metadata").unwrap();
        assert_ne!(added, crate::cache::fingerprint(&path).unwrap());
        fs::remove_file(chunk).unwrap();
        assert_eq!(initial, crate::cache::fingerprint(&path).unwrap());
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn sqlite_scan_timing() {
        let f = Fixture::new();
        let db = f.0.join("cache.sqlite3");
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(512, 512)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        for i in 0..64 {
            let path = f.0.join("addons").join(format!("{i}.vpk"));
            fs::write(&path, vpk()).unwrap();
            fs::write(path.with_extension("png"), png.get_ref()).unwrap();
        }
        let start = std::time::Instant::now();
        let cold = scan_with_cache(&f.0, &db).unwrap();
        let cold_time = start.elapsed();
        let start = std::time::Instant::now();
        let warm = scan_with_cache(&f.0, &db).unwrap();
        let warm_time = start.elapsed();
        assert_eq!(cold.len(), warm.len());
        assert!(
            cold.iter()
                .zip(&warm)
                .all(|(a, b)| a.metadata.cover == b.metadata.cover)
        );
        println!("64 VPKs with 512px sidecars: cold={cold_time:?}, warm={warm_time:?}");
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn sqlite_reuses_png_thumbnails_and_refreshes_previews() {
        let f = Fixture::new();
        let path = f.0.join("addons/preview.vpk");
        let db = f.0.join("cache.sqlite3");
        fs::write(&path, vpk()).unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(512, 256)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        fs::write(path.with_extension("png"), png.get_ref()).unwrap();
        let cold = scan_with_cache(&f.0, &db).unwrap();
        let warm = scan_with_cache(&f.0, &db).unwrap();
        assert!(warm[0].metadata.cover_prepared);
        assert_eq!(cold[0].metadata.cover, warm[0].metadata.cover);
        let decoded = image::load_from_memory(warm[0].metadata.cover.as_ref().unwrap()).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (384, 192));
        fs::remove_file(path.with_extension("png")).unwrap();
        assert!(
            scan_with_cache(&f.0, &db).unwrap()[0]
                .metadata
                .cover
                .is_none()
        );
    }
    #[test]
    fn bad_config_and_existing_lock_are_not_overwritten() {
        let f = Fixture::new();
        fs::write(f.0.join("addonlist.txt"), "broken").unwrap();
        assert!(set_enabled(&f.0, &[("x.vpk".into(), false)]).is_err());
        assert_eq!(
            fs::read_to_string(f.0.join("addonlist.txt")).unwrap(),
            "broken"
        );
        fs::write(f.0.join(".addonwrench.lock"), "locked").unwrap();
        assert!(set_enabled(&f.0, &[]).is_err());
        assert!(f.0.join(".addonwrench.lock").exists());
    }
    #[test]
    fn missing_configuration_is_created() {
        let f = Fixture::new();
        set_enabled(&f.0, &[("a.vpk".into(), false)]).unwrap();
        let text = fs::read_to_string(f.0.join("addonlist.txt")).unwrap();
        assert!(!AddonList::parse(&text).unwrap().enabled("a.vpk"));
    }
    #[test]
    fn import_rejects_bad_files_and_never_overwrites() {
        let f = Fixture::new();
        let source = f.0.join("test.vpk");
        fs::write(&source, b"not a vpk").unwrap();
        assert!(import(&f.0, &source).is_err());
        assert!(!f.0.join("addons/test.vpk").exists());
        fs::write(&source, vpk()).unwrap();
        let destination = import(&f.0, &source).unwrap();
        assert!(import(&f.0, &source).is_err());
        assert_eq!(fs::read(destination).unwrap(), vpk());
        assert_eq!(fs::read_dir(f.0.join("addons")).unwrap().count(), 2);
    }
    #[test]
    fn rejects_truncated_vpk_tree() {
        let f = Fixture::new();
        let source = f.0.join("bad.vpk");
        let mut bytes = vpk();
        bytes[12] = b'a';
        fs::write(&source, bytes).unwrap();
        assert!(validate_vpk(&source).is_err());
    }

    fn payload_vpk(archive: u16, offset: u32, size: u32) -> Vec<u8> {
        let mut tree = b"txt\0 \0addoninfo\0".to_vec();
        tree.extend(0u32.to_le_bytes()); // CRC not checked by the structural validator.
        tree.extend(0u16.to_le_bytes());
        tree.extend(archive.to_le_bytes());
        tree.extend(offset.to_le_bytes());
        tree.extend(size.to_le_bytes());
        tree.extend(0xffffu16.to_le_bytes());
        tree.extend([0, 0, 0]);
        let mut bytes = Vec::new();
        bytes.extend(0x55aa1234u32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.extend((tree.len() as u32).to_le_bytes());
        bytes.extend(tree);
        bytes.extend(b"test");
        bytes
    }

    #[test]
    fn validates_payload_bounds_and_rejects_external_archives() {
        let f = Fixture::new();
        let source = f.0.join("payload.vpk");
        fs::write(&source, payload_vpk(0x7fff, 0, 4)).unwrap();
        validate_vpk(&source).unwrap();
        fs::write(&source, payload_vpk(0x7fff, 1, 4)).unwrap();
        assert!(validate_vpk(&source).is_err());
        fs::write(&source, payload_vpk(0, 0, 4)).unwrap();
        assert!(validate_vpk(&source).unwrap_err().contains("分卷"));
    }

    #[test]
    fn validates_v2_sections() {
        let f = Fixture::new();
        let source = f.0.join("v2.vpk");
        let mut bytes = vpk();
        bytes[4..8].copy_from_slice(&2u32.to_le_bytes());
        bytes.splice(12..12, [0u8; 16]);
        fs::write(&source, &bytes).unwrap();
        validate_vpk(&source).unwrap();
        bytes[12..16].copy_from_slice(&100u32.to_le_bytes());
        fs::write(&source, bytes).unwrap();
        assert!(validate_vpk(&source).is_err());
    }

    #[test]
    fn recognizes_game_root_and_skips_archive_chunks() {
        let f = Fixture::new();
        assert!(crate::discovery::game_dir(&f.0).is_err());
        fs::write(f.0.join("gameinfo.txt"), "test").unwrap();
        assert_eq!(
            crate::discovery::game_dir(&f.0).unwrap(),
            f.0.canonicalize().unwrap()
        );
        fs::write(f.0.join("addons/map_dir.vpk"), vpk()).unwrap();
        fs::write(f.0.join("addons/map_000.vpk"), b"chunk").unwrap();
        fs::write(f.0.join("addons/custom_123.vpk"), vpk()).unwrap();
        assert_eq!(scan(&f.0).unwrap().len(), 2);
    }
}
