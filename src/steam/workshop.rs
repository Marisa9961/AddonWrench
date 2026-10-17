use addon_wrench::{addons::Addon, metadata::MAX_COVER};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const ENDPOINT: &str =
    "https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub app_id: u32,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub preview_url: String,
    pub fetched: u64,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn needs_refresh(fetched: u64) -> bool {
    fetched == 0 || now().saturating_sub(fetched) > 24 * 60 * 60
}
pub fn cache_dir() -> PathBuf {
    std::env::var_os("ADDON_WRENCH_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            addon_wrench::discovery::settings_path()
                .parent()
                .unwrap()
                .join("cache")
        })
        .join("workshop")
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.bytes().all(|b| b.is_ascii_digit())
        && id.parse::<u64>().is_ok_and(|n| n > 0)
}

pub struct Response {
    pub items: Vec<Item>,
    pub issues: HashMap<String, String>,
}

pub fn parse_response(bytes: &[u8], requested: &[String], app_id: u32) -> Result<Response, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let details = value["response"]["publishedfiledetails"]
        .as_array()
        .ok_or("Steam 响应缺少项目列表")?;
    let mut issues: HashMap<_, _> = requested
        .iter()
        .map(|id| {
            (
                id.clone(),
                "工坊资料暂不可用，项目可能已移除或限制访问".into(),
            )
        })
        .collect();
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for detail in details {
        let id = detail["publishedfileid"].as_str().unwrap_or_default();
        if !valid_id(id) || !requested.iter().any(|r| r == id) || !seen.insert(id.to_string()) {
            continue;
        }
        if detail["result"].as_u64() != Some(1) {
            if detail["result"].as_u64() == Some(15) {
                issues.insert(id.into(), "工坊项目限制访问".into());
            }
            continue;
        }
        if detail["consumer_app_id"].as_u64() != Some(u64::from(app_id)) {
            issues.insert(id.into(), "该工坊项目不属于当前游戏".into());
            continue;
        }
        let title = detail["title"].as_str().unwrap_or_default().trim();
        if title.is_empty() {
            continue;
        }
        let mut tags = Vec::new();
        if let Some(values) = detail["tags"].as_array() {
            for tag in values.iter().take(128).filter_map(|v| v["tag"].as_str()) {
                let tag: String = tag.trim().chars().take(128).collect();
                if !tag.is_empty() && !tags.iter().any(|t: &String| t.eq_ignore_ascii_case(&tag)) {
                    tags.push(tag);
                }
            }
        }
        issues.remove(id);
        items.push(Item {
            id: id.into(),
            app_id,
            title: title.chars().take(512).collect(),
            description: detail["description"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(65536)
                .collect(),
            tags,
            preview_url: detail["preview_url"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            fetched: now(),
        });
    }
    Ok(Response { items, issues })
}

#[cfg(test)]
use addon_wrench::metadata::thumbnail;

fn open_cache(dir: &Path) -> Option<addon_wrench::cache::Cache> {
    let path = dir.join("workshop.sqlite3");
    path.is_file()
        .then(|| addon_wrench::cache::Cache::open(&path).ok())
        .flatten()
}

fn load(
    cache: Option<&addon_wrench::cache::Cache>,
    id: &str,
    app_id: u32,
) -> Option<(Item, Vec<u8>)> {
    if !valid_id(id) {
        return None;
    }
    let (payload, cover) = cache?.get(id, &format!("workshop-v2:{app_id}"))?;
    let item: Item = serde_json::from_slice(&payload).ok()?;
    (item.app_id == app_id && item.id == id && !item.title.trim().is_empty())
        .then_some((item, cover))
}

fn apply(addon: &mut Addon, item: &Item, cover: &[u8]) {
    if addon.metadata.workshop_id.as_deref() != Some(&item.id) {
        return;
    }
    if !addon_wrench::metadata::has_local_title(addon) {
        addon.name = item.title.clone();
    }
    addon.metadata.workshop_title = item.title.clone();
    addon.metadata.workshop_description = super::description::plain_text(&item.description);
    addon.metadata.workshop_fetched = item.fetched;
    addon.metadata.workshop_verified = true;
    addon.metadata.workshop_tags = item.tags.clone();
    if addon.metadata.cover.is_none() && !cover.is_empty() && cover.len() <= MAX_COVER {
        addon.metadata.cover = Some(Arc::new(cover.to_vec()));
        addon.metadata.cover_prepared = true;
        addon.metadata.cover_source = "Steam 工坊预览图（本地缓存）".into();
    }
}

/// Call only after a complete successful scan; cleanup does not access the network.
pub fn maintain(addons: &[Addon], dir: &Path, game: &Path, app_id: u32) -> Result<(), String> {
    let Some(cache) = open_cache(dir) else {
        return Ok(());
    };
    let game = game.canonicalize().map_err(|error| error.to_string())?;
    let scope = format!("{app_id}:{}", game.to_string_lossy());
    let live: Vec<_> = addons
        .iter()
        .filter_map(|addon| addon.metadata.workshop_id.clone())
        .collect();
    cache.maintain_retained(
        &scope,
        &format!("workshop-v2:{app_id}"),
        &live,
        now(),
        30 * 24 * 60 * 60,
    )
}

/// Local data is available immediately; only PNG thumbnails enter the GUI.
pub fn prepare(addons: &mut [Addon], dir: &Path, app_id: Option<u32>) {
    let cache = open_cache(dir);
    for addon in addons {
        addon_wrench::metadata::prepare_cover(&mut addon.metadata);
        if let Some(id) = addon.metadata.workshop_id.as_ref()
            && let Some(app_id) = app_id
            && let Some((item, cover)) = load(cache.as_ref(), id, app_id)
        {
            apply(addon, &item, &cover);
        }
    }
}

pub struct Outcome {
    pub addons: Vec<Addon>,
    pub message: String,
    pub issues: HashMap<String, String>,
}

pub fn clear_online_cover(addon: &mut Addon) {
    if addon.metadata.cover_source.starts_with("Steam") {
        addon.metadata.cover = None;
        addon.metadata.cover_source.clear();
    }
}

pub fn merge_metadata(addon: &mut Addon, updated: &Addon) {
    if addon.metadata.workshop_id != updated.metadata.workshop_id {
        return;
    }
    if !addon_wrench::metadata::has_local_title(addon) {
        addon.name = updated.name.clone();
    }
    addon.metadata.workshop_verified = updated.metadata.workshop_verified;
    addon.metadata.workshop_title = updated.metadata.workshop_title.clone();
    addon.metadata.workshop_description = updated.metadata.workshop_description.clone();
    addon.metadata.workshop_fetched = updated.metadata.workshop_fetched;
    addon.metadata.workshop_tags = updated.metadata.workshop_tags.clone();
    if addon.metadata.cover.is_none() && updated.metadata.cover_source.starts_with("Steam") {
        addon.metadata.cover = updated.metadata.cover.clone();
        addon.metadata.cover_prepared = updated.metadata.cover_prepared;
        addon.metadata.cover_source = updated.metadata.cover_source.clone();
    }
}

/// A local rescan may finish after a simultaneous metadata request.
pub fn preserve_recent(addons: &mut [Addon], current: &[Addon]) {
    for addon in addons {
        if let Some(recent) = current.iter().find(|recent| {
            recent.key == addon.key
                && recent.path == addon.path
                && recent.bytes == addon.bytes
                && recent.modified == addon.modified
                && recent.metadata.workshop_verified
                && recent.metadata.workshop_fetched > addon.metadata.workshop_fetched
        }) {
            if addon.metadata.workshop_id != recent.metadata.workshop_id {
                clear_online_cover(addon);
                addon.metadata.workshop_id = recent.metadata.workshop_id.clone();
            }
            merge_metadata(addon, recent);
        }
    }
}

pub fn fetch(ids: &[String], addons: Vec<Addon>, dir: &Path, app_id: u32) -> Outcome {
    fetch_with(ids, addons, dir, app_id, |body| {
        super::steam_http::request(ENDPOINT, Some(body), 4 * 1024 * 1024)
    })
}

fn fetch_with(
    ids: &[String],
    mut addons: Vec<Addon>,
    dir: &Path,
    app_id: u32,
    mut request: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Outcome {
    let start = std::time::Instant::now();
    let mut fetched = 0;
    let mut issues = HashMap::new();
    let mut failure = None;
    let mut received = HashSet::new();
    let cache = match addon_wrench::cache::Cache::open(&dir.join("workshop.sqlite3")) {
        Ok(cache) => cache,
        Err(e) => {
            return Outcome {
                addons: Vec::new(),
                message: format!("无法保存工坊缓存：{e}"),
                issues: ids
                    .iter()
                    .map(|id| (id.clone(), format!("无法保存工坊缓存：{e}")))
                    .collect(),
            };
        }
    };
    for batch in ids.chunks(50) {
        if start.elapsed() > Duration::from_secs(90) {
            failure = Some("本次查询已到时间上限，可稍后继续更新".into());
            break;
        }
        let mut body = format!("itemcount={}", batch.len());
        for (i, id) in batch.iter().enumerate() {
            body.push_str(&format!("&publishedfileids%5B{i}%5D={id}"));
        }
        let result = request(&body).and_then(|bytes| parse_response(&bytes, batch, app_id));
        let response = match result {
            Ok(response) => response,
            Err(e) => {
                failure = Some(e);
                break;
            }
        };
        issues.extend(response.issues);
        for item in response.items {
            let cover = load(Some(&cache), &item.id, app_id)
                .map(|(_, cover)| cover)
                .unwrap_or_default();
            let saved = serde_json::to_vec(&item)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    cache.put(&item.id, &format!("workshop-v2:{app_id}"), &bytes, &cover)
                });
            if let Err(e) = saved {
                issues.insert(item.id.clone(), format!("资料已获取，但缓存未保存：{e}"));
                failure = Some(format!("缓存写入失败：{e}"));
            }
            for addon in &mut addons {
                apply(addon, &item, &cover);
            }
            fetched += 1;
            received.insert(item.id);
        }
    }
    let mut message = format!("已获取 {fetched}/{} 个工坊项目的信息", ids.len());
    if fetched < ids.len() {
        message.push_str("；未取得的信息继续使用文件内容或已有缓存");
    }
    if let Some(e) = failure {
        for id in ids.iter().filter(|id| !received.contains(*id)) {
            issues
                .entry(id.clone())
                .or_insert_with(|| format!("联网获取失败，可稍后重试：{e}"));
        }
        message.push_str(&format!("。{e}"));
    }
    addons.retain(|a| {
        a.metadata
            .workshop_id
            .as_ref()
            .is_some_and(|id| received.contains(id))
    });
    Outcome {
        addons,
        message,
        issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;

    fn test_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "aw-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn completion_preserves_local_name_description_and_enabling_state() {
        let mut addon = addon();
        addon.metadata.title = Some("Local title".into());
        addon.metadata.description = "Local description".into();
        let item = Item {
            id: "123".into(),
            app_id: 550,
            title: "Workshop title".into(),
            description: "[b]Online description[/b]".into(),
            tags: vec![],
            preview_url: String::new(),
            fetched: now(),
        };
        apply(&mut addon, &item, &[]);
        assert_eq!(addon.name, "Local title");
        assert_eq!(addon.metadata.description, "Local description");
        assert_eq!(addon.metadata.workshop_description, "Online description");
        assert_eq!(addon.metadata.workshop_title, "Workshop title");
        assert!(addon_wrench::metadata::has_resolved_title(&addon));
        addon.metadata.title = Some("123".into());
        apply(&mut addon, &item, &[]);
        assert_eq!(addon.name, "Workshop title");
        assert!(!addon.enabled);
        let mut different = addon.clone();
        different.metadata.workshop_id = Some("456".into());
        different.name = "Wrong association".into();
        merge_metadata(&mut addon, &different);
        assert_eq!(addon.name, "Workshop title");
    }

    #[test]
    fn anonymous_batch_caches_partial_success_and_keeps_failed_items_local() {
        let root = test_dir("partial-workshop");
        let mut second = addon();
        second.key = "workshop/124.vpk".into();
        second.metadata.workshop_id = Some("124".into());
        let response = br#"{"response":{"publishedfiledetails":[
            {"publishedfileid":"123","consumer_app_id":550,"result":1,"title":"Online title","description":"[h1]Details[/h1]"},
            {"publishedfileid":"124","result":15}]}}"#;
        let result = fetch_with(
            &["123".into(), "124".into()],
            vec![addon(), second.clone()],
            &root,
            550,
            |body| {
                assert_eq!(
                    body,
                    "itemcount=2&publishedfileids%5B0%5D=123&publishedfileids%5B1%5D=124"
                );
                Ok(response.to_vec())
            },
        );
        assert_eq!(result.addons.len(), 1);
        assert_eq!(result.issues["124"], "工坊项目限制访问");
        let mut offline = vec![addon(), second];
        prepare(&mut offline, &root, Some(550));
        assert_eq!(offline[0].name, "Online title");
        assert_eq!(offline[0].metadata.workshop_description, "Details");
        assert_eq!(offline[1].name, "Local title");
        let before = offline[0].clone();
        let failed = fetch_with(&["123".into()], offline.clone(), &root, 550, |_| {
            Err("Steam 请求过于频繁".into())
        });
        assert!(failed.addons.is_empty());
        assert!(failed.issues["123"].contains("请求过于频繁"));
        prepare(&mut offline, &root, Some(550));
        assert_eq!(offline[0].name, before.name);
        assert_eq!(
            offline[0].metadata.workshop_description,
            before.metadata.workshop_description
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_rescan_preserves_newer_online_metadata_without_restoring_enabled_state() {
        let mut scanned = addon();
        scanned.enabled = true;
        let mut recent = addon();
        recent.name = "New online title".into();
        recent.metadata.workshop_verified = true;
        recent.metadata.workshop_fetched = now();
        recent.metadata.workshop_description = "New description".into();
        let mut addons = vec![scanned];
        preserve_recent(&mut addons, &[recent.clone()]);
        assert_eq!(addons[0].name, "New online title");
        assert!(addons[0].enabled);
        addons[0].bytes += 1;
        addons[0].name = "Changed file".into();
        preserve_recent(&mut addons, &[recent]);
        assert_eq!(addons[0].name, "Changed file");
    }

    #[test]
    fn long_descriptions_and_missing_response_entries_are_handled() {
        let description = "中".repeat(20000);
        let bytes = serde_json::to_vec(&serde_json::json!({"response":{"publishedfiledetails":[{"publishedfileid":"123", "consumer_app_id":550,"result":1,"title":"title","description":description}]}})).unwrap();
        let response = parse_response(&bytes, &["123".into(), "124".into()], 550).unwrap();
        assert_eq!(response.items[0].description.chars().count(), 20000);
        assert!(response.issues.contains_key("124"));
    }
    #[test]
    fn gif_sidecar_with_jpg_extension_decodes_by_content() {
        let root = std::env::temp_dir().join(format!("aw-gif-{}-{}", std::process::id(), now()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("123.jpg");
        let mut bytes = Vec::new();
        image::codecs::gif::GifEncoder::new(&mut bytes)
            .encode(
                &[255, 0, 0, 255, 0, 255, 0, 255],
                2,
                1,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        fs::write(&path, bytes).unwrap();
        let metadata = addon_wrench::metadata::read(&path.with_extension("vpk"), false);
        let png = thumbnail(metadata.cover.as_ref().unwrap()).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 1));
        assert!(metadata.workshop_id.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    /// Always exercise directory scans with independent files; a real fixture is optional extra coverage.
    #[test]
    fn directory_covers_decode() {
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let fixture = Fixture(test_dir("directory-covers"));
        let addons_dir = fixture.0.join("addons");
        fs::create_dir_all(addons_dir.join("workshop")).unwrap();
        let image = image::DynamicImage::new_rgb8(32, 16);
        let mut empty_vpk = Vec::new();
        empty_vpk.extend(0x55aa1234u32.to_le_bytes());
        empty_vpk.extend(1u32.to_le_bytes());
        empty_vpk.extend(1u32.to_le_bytes());
        empty_vpk.push(0);
        for (index, format) in [
            image::ImageFormat::Png,
            image::ImageFormat::Jpeg,
            image::ImageFormat::Gif,
            image::ImageFormat::WebP,
        ]
        .into_iter()
        .enumerate()
        {
            let path = addons_dir
                .join("workshop")
                .join(format!("{}.vpk", index + 1));
            fs::write(&path, &empty_vpk).unwrap();
            // Steam sidecars can have a JPG extension even when the bytes use another format.
            let mut bytes = Cursor::new(Vec::new());
            image.write_to(&mut bytes, format).unwrap();
            fs::write(path.with_extension("jpg"), bytes.get_ref()).unwrap();
        }
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let mut tree = b"png\0 \0addonimage\0".to_vec();
        tree.extend(0u32.to_le_bytes());
        tree.extend((png.get_ref().len() as u16).to_le_bytes());
        tree.extend(0x7fffu16.to_le_bytes());
        tree.extend(0u32.to_le_bytes());
        tree.extend(0u32.to_le_bytes());
        tree.extend(0xffffu16.to_le_bytes());
        tree.extend(png.get_ref());
        tree.extend([0, 0, 0]);
        let mut embedded = Vec::new();
        embedded.extend(0x55aa1234u32.to_le_bytes());
        embedded.extend(1u32.to_le_bytes());
        embedded.extend((tree.len() as u32).to_le_bytes());
        embedded.extend(tree);
        fs::write(addons_dir.join("embedded.vpk"), embedded).unwrap();

        fn check(root: &Path, expected: Option<usize>) {
            let cache = root.join("__cover_test_no_cache__");
            let mut addons =
                addon_wrench::addons::scan_with_cache(root, &cache.join("local.sqlite3")).unwrap();
            assert!(!addons.is_empty());
            if let Some(expected) = expected {
                assert_eq!(addons.len(), expected);
            }
            prepare(&mut addons, &cache, None);
            for addon in addons {
                let bytes = addon
                    .metadata
                    .cover
                    .as_ref()
                    .unwrap_or_else(|| panic!("{}: {}", addon.key, addon.metadata.note));
                let image =
                    image::load_from_memory_with_format(bytes, image::ImageFormat::Png).unwrap();
                assert!(image.width() > 0 && image.height() > 0);
                assert!(addon.metadata.cover_prepared);
            }
            assert!(!root.join("addonlist.txt").exists());
        }
        check(&fixture.0, Some(5));
        if let Some(real) = std::env::var_os("ADDON_WRENCH_COVER_TEST_GAME") {
            // Read the external directory without creating any cache or changing enable configuration.
            let root = PathBuf::from(real);
            let before = fs::read(root.join("addonlist.txt")).ok();
            let mut addons =
                addon_wrench::addons::scan_with_cache(&root, &fixture.0.join("real.sqlite3"))
                    .unwrap();
            assert!(!addons.is_empty());
            prepare(&mut addons, &fixture.0.join("unused"), None);
            for addon in addons {
                let bytes = addon
                    .metadata
                    .cover
                    .as_ref()
                    .unwrap_or_else(|| panic!("{}: {}", addon.key, addon.metadata.note));
                image::load_from_memory_with_format(bytes, image::ImageFormat::Png).unwrap();
            }
            assert_eq!(fs::read(root.join("addonlist.txt")).ok(), before);
        }
    }

    #[test]
    fn response_checks_app_result_and_requested_id() {
        let json = br#"{"response":{"publishedfiledetails":[
            {"publishedfileid":"123","consumer_app_id":550,"result":1,"title":"Real title","tags":[{"tag":"Weapons"},{"tag":"weapons"}]},
            {"publishedfileid":"124","consumer_app_id":440,"result":1,"title":"Wrong game"},
            {"publishedfileid":"125","consumer_app_id":550,"result":9,"title":"Private"},
            {"publishedfileid":"126","consumer_app_id":550,"result":1,"title":"Not requested"}]}}"#;
        let response =
            parse_response(json, &["123".into(), "124".into(), "125".into()], 550).unwrap();
        assert_eq!(response.issues.len(), 2);
        let items = response.items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Real title");
        assert_eq!(items[0].tags, ["Weapons"]);
        let other = parse_response(json, &["123".into(), "124".into()], 440)
            .unwrap()
            .items;
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].id, "124");
        assert_eq!(other[0].app_id, 440);
    }
    #[test]
    fn damaged_cover_is_rejected() {
        assert!(thumbnail(b"not an image").is_err());
    }

    fn addon() -> Addon {
        Addon {
            key: "workshop/123.vpk".into(),
            name: "Local title".into(),
            path: "123.vpk".into(),
            bytes: 1,
            modified: None,
            enabled: false,
            workshop: true,
            metadata: addon_wrench::metadata::Metadata {
                workshop_id: Some("123".into()),
                local_tags: vec!["武器".into()],
                author: "File author".into(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn cached_workshop_metadata_and_cover_work_offline() {
        let root = std::env::temp_dir().join(format!(
            "aw-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let item = Item {
            id: "123".into(),
            app_id: 550,
            title: "Workshop title".into(),
            description: "Description".into(),
            tags: vec!["Weapons".into()],
            preview_url: String::new(),
            fetched: now(),
        };
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4, 4)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let cache = addon_wrench::cache::Cache::open(&root.join("workshop.sqlite3")).unwrap();
        cache
            .put(
                "123",
                "workshop-v2:550",
                &serde_json::to_vec(&item).unwrap(),
                png.get_ref(),
            )
            .unwrap();
        let mut addons = vec![addon()];
        prepare(&mut addons, &root, Some(550));
        assert_eq!(addons[0].name, "Workshop title");
        assert!(addons[0].metadata.workshop_verified);
        assert!(addons[0].metadata.cover.is_some());
        assert_eq!(addons[0].metadata.local_tags, ["武器"]);
        assert!(!addons[0].enabled);
        let mut wrong_game = vec![addon()];
        prepare(&mut wrong_game, &root, Some(440));
        assert_eq!(wrong_game[0].name, "Local title");
        assert!(!wrong_game[0].metadata.workshop_verified);

        cache
            .put("123", "workshop-v2:550", b"corrupt cache", &[])
            .unwrap();
        let mut fallback = vec![addon()];
        prepare(&mut fallback, &root, Some(550));
        assert_eq!(fallback[0].name, "Local title");
        drop(cache);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn late_metadata_response_preserves_state_and_local_fields() {
        let mut current = addon();
        let mut response = addon();
        response.enabled = true;
        response.name = "Online title".into();
        response.metadata.author = "Old file author".into();
        response.metadata.local_tags.clear();
        response.metadata.workshop_tags = vec!["Weapons".into()];
        response.metadata.workshop_verified = true;
        merge_metadata(&mut current, &response);
        assert!(!current.enabled);
        assert_eq!(current.name, "Online title");
        assert_eq!(current.metadata.author, "File author");
        assert_eq!(current.metadata.local_tags, ["武器"]);
        assert_eq!(current.metadata.workshop_tags, ["Weapons"]);
    }
}
