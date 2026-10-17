//! Read only the metadata entries from a VPK; never unpack arbitrary archive paths.
use crate::keyvalues::tokenize;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::Arc,
};

const MAX_TREE: u64 = 32 * 1024 * 1024;
pub const MAX_COVER: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: String,
    pub description: String,
    pub local_tags: Vec<String>,
    #[serde(default)]
    pub custom_tags: Vec<String>,
    pub workshop_tags: Vec<String>,
    pub workshop_id: Option<String>,
    pub workshop_verified: bool,
    #[serde(default)]
    pub workshop_title: String,
    #[serde(default)]
    pub workshop_description: String,
    #[serde(default)]
    pub workshop_fetched: u64,
    #[serde(skip)]
    pub cover: Option<Arc<Vec<u8>>>,
    #[serde(default)]
    pub cover_prepared: bool,
    pub cover_source: String,
    pub note: String,
}

impl Metadata {
    pub fn has_tag(&self, value: &str) -> bool {
        let value = value.to_lowercase();
        self.tags().any(|tag| tag.to_lowercase() == value)
    }

    pub fn tags(&self) -> impl Iterator<Item = &String> {
        self.workshop_tags
            .iter()
            .chain(self.local_tags.iter())
            .chain(self.custom_tags.iter())
    }
}

#[cfg(feature = "desktop")]
pub fn thumbnail(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Cursor;
    if bytes.len() > MAX_COVER {
        return Err("封面文件过大".into());
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(80 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?;
    let image = if image.width() > 384 || image.height() > 384 {
        image.thumbnail(384, 384)
    } else {
        image
    };
    let mut result = Cursor::new(Vec::new());
    image
        .write_to(&mut result, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(result.into_inner())
}

#[cfg(feature = "desktop")]
pub fn prepare_cover(metadata: &mut Metadata) {
    if metadata.cover_prepared {
        return;
    }
    if let Some(bytes) = metadata.cover.take() {
        match thumbnail(&bytes) {
            Ok(bytes) => metadata.cover = Some(Arc::new(bytes)),
            Err(_) => {
                metadata.cover_source.clear();
                if metadata.note.is_empty() {
                    metadata.note = "封面损坏或格式不支持，已使用占位图".into();
                }
            }
        }
    }
    metadata.cover_prepared = true;
}

pub fn candidate_id(path: &Path, in_workshop: bool) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    if in_workshop && stem.bytes().all(|byte| byte.is_ascii_digit()) {
        workshop_id(stem)
    } else {
        None
    }
}

/// Accept only a positive Steam published-file ID or an exact Steam Workshop URL.
pub fn workshop_id(value: &str) -> Option<String> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return value
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .map(|id| id.to_string());
    }
    if let Some(id) = value.strip_prefix("steam://url/CommunityFilePage/") {
        if !id.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        return id
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .map(|id| id.to_string());
    }
    let path = value
        .strip_prefix("https://steamcommunity.com/")
        .or_else(|| value.strip_prefix("http://steamcommunity.com/"))?;
    let (path, query) = path.split_once('?')?;
    if !matches!(
        path.trim_end_matches('/'),
        "sharedfiles/filedetails" | "workshop/filedetails"
    ) {
        return None;
    }
    let mut ids = query
        .split('#')
        .next()?
        .split('&')
        .filter_map(|pair| pair.strip_prefix("id="));
    let id = ids.next()?;
    if ids.next().is_some() {
        return None;
    }
    // Do not recurse into a URL embedded in a query parameter.
    if !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    workshop_id(id)
}

pub fn has_local_title(addon: &crate::addons::Addon) -> bool {
    addon.metadata.title.as_deref().is_some_and(|title| {
        let title = title.trim();
        !title.is_empty()
            && !(title.bytes().all(|b| b.is_ascii_digit())
                && (addon.metadata.workshop_id.as_deref() == Some(title)
                    || addon.path.file_stem().is_some_and(|stem| stem == title)))
    })
}

pub fn has_resolved_title(addon: &crate::addons::Addon) -> bool {
    has_local_title(addon)
        || (addon.metadata.workshop_verified && !addon.metadata.workshop_title.trim().is_empty())
}

pub fn read(path: &Path, in_workshop: bool) -> Metadata {
    let mut result = Metadata {
        workshop_id: candidate_id(path, in_workshop),
        ..Default::default()
    };
    match entries(path) {
        Ok((info, cover)) => {
            if let Some(info) = info {
                match parse_info(&info) {
                    Ok(parsed) => {
                        result.title = parsed.title;
                        result.author = parsed.author;
                        result.description = parsed.description;
                        result.local_tags = parsed.local_tags;
                        result.custom_tags = parsed.custom_tags;
                        if result.workshop_id.is_none() {
                            result.workshop_id = parsed.workshop_id;
                        }
                    }
                    Err(e) => result.note = e,
                }
            }
            if let Some(cover) = cover {
                result.cover = Some(Arc::new(cover));
                result.cover_source = "VPK 内的 addonimage".into();
            }
        }
        Err(e) => result.note = format!("文件元数据未读取：{e}"),
    }
    // Sidecars can be copied together with the VPK into any addon directory.
    // Their presence does not establish a Workshop identity.
    {
        for extension in ["jpg", "png", "jpeg", "gif", "webp"] {
            let sidecar = path.with_extension(extension);
            if let Ok(file) = File::open(sidecar) {
                let mut bytes = Vec::new();
                if file
                    .take(MAX_COVER as u64 + 1)
                    .read_to_end(&mut bytes)
                    .is_ok()
                    && !bytes.is_empty()
                    && bytes.len() <= MAX_COVER
                {
                    result.cover = Some(Arc::new(bytes));
                    result.cover_source = format!("模组目录中的同名预览图（.{extension}）");
                    break;
                }
            }
        }
    }
    result
}

fn decode(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if !bytes.len().is_multiple_of(2) {
            return Err("addoninfo UTF-16 长度无效".into());
        }
        let little = bytes[0] == 0xff;
        let units: Vec<_> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| {
                if little {
                    u16::from_le_bytes([b[0], b[1]])
                } else {
                    u16::from_be_bytes([b[0], b[1]])
                }
            })
            .collect();
        String::from_utf16(&units).map_err(|_| "addoninfo UTF-16 内容无效".into())
    } else if let Ok(text) = std::str::from_utf8(bytes) {
        Ok(text.to_string())
    } else {
        let (text, _, errors) = encoding_rs::GBK.decode(bytes);
        if errors {
            Err("addoninfo 不是有效的 UTF-8 / UTF-16 / GBK 文本".into())
        } else {
            Ok(text.into_owned())
        }
    }
}

fn push_tag(tags: &mut Vec<String>, value: &str) {
    let tag = value.trim().chars().take(128).collect::<String>();
    if tags.len() < 128
        && !tag.is_empty()
        && !tags
            .iter()
            .any(|existing| existing.to_lowercase() == tag.to_lowercase())
    {
        tags.push(tag);
    }
}

pub fn parse_info(bytes: &[u8]) -> Result<Metadata, String> {
    let text = decode(bytes)?;
    let tokens = tokenize(text.trim_end_matches('\0'))?;
    if tokens.len() < 3
        || !tokens[0].text.eq_ignore_ascii_case("AddonInfo")
        || tokens[1].text != "{"
        || tokens.last().unwrap().text != "}"
    {
        return Err("addoninfo 缺少 AddonInfo 对象".into());
    }
    let mut result = Metadata::default();
    let mut index = 2;
    while index < tokens.len() - 1 {
        let key = &tokens[index];
        let value = tokens.get(index + 1).ok_or("addoninfo 缺少字段值")?;
        if key.brace || value.brace {
            return Err("addoninfo 字段格式无效".into());
        }
        let key = key.text.to_ascii_lowercase();
        let value = value.text.trim();
        match key.as_str() {
            "addontitle" if !value.is_empty() => {
                result.title = Some(value.chars().take(512).collect())
            }
            "addonauthor" => result.author = value.chars().take(512).collect(),
            "addondescription" => result.description = value.chars().take(8192).collect(),
            "addonurl0" | "addonurl" | "workshopurl" | "workshopid" | "publishedfileid" => {
                if result.workshop_id.is_none() {
                    result.workshop_id = workshop_id(value);
                }
            }
            "addontags" | "addontag" | "tags" | "tag" => {
                for tag in value.split([',', ';', '|', '，', '；']) {
                    push_tag(&mut result.custom_tags, tag);
                }
            }
            _ => {
                if let Some(category) = key.strip_prefix("addoncontent_")
                    && matches!(value.to_ascii_lowercase().as_str(), "1" | "true")
                {
                    let label = match category {
                        "campaign" => "战役",
                        "map" => "地图",
                        "music" => "音乐",
                        "sound" => "音效",
                        "skin" => "皮肤",
                        "weapon" => "武器",
                        "survivor" => "生还者",
                        "infected" => "感染者",
                        "script" => "脚本",
                        "versus" => "对抗",
                        "coop" => "合作",
                        "survival" => "生存",
                        "scavenge" => "清道夫",
                        "spray" => "喷漆",
                        "other" => "其他",
                        _ => "",
                    };
                    if label.is_empty() {
                        let original = &tokens[index].text["addoncontent_".len()..];
                        push_tag(&mut result.custom_tags, original);
                    } else {
                        push_tag(&mut result.local_tags, label);
                    }
                }
            }
        }
        index += 2;
    }
    Ok(result)
}

fn string<'a>(tree: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], String> {
    let rest = tree.get(*cursor..).ok_or("VPK 目录越界")?;
    let end = rest
        .iter()
        .position(|b| *b == 0)
        .ok_or("VPK 字符串未终止")?;
    *cursor += end + 1;
    Ok(&rest[..end])
}

type Entries = (Option<Vec<u8>>, Option<Vec<u8>>);
fn entries(path: &Path) -> Result<Entries, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    let mut header = [0u8; 12];
    file.read_exact(&mut header)
        .map_err(|_| "VPK 文件头被截断")?;
    let word = |offset| u32::from_le_bytes(header[offset..offset + 4].try_into().unwrap()) as u64;
    if word(0) != 0x55aa1234 {
        return Err("VPK 签名无效".into());
    }
    let header_size = match word(4) {
        1 => 12,
        2 => 28,
        _ => return Err("VPK 版本不支持".into()),
    };
    let tree_size = word(8);
    if tree_size == 0 || tree_size > MAX_TREE || header_size + tree_size > length {
        return Err("VPK 目录大小无效".into());
    }
    let data_start = header_size + tree_size;
    let payload_size = if header_size == 28 {
        let mut extra = [0; 16];
        file.read_exact(&mut extra)
            .map_err(|_| "VPK v2 文件头被截断")?;
        let sections: u64 = extra
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_le_bytes(*b) as u64)
            .sum();
        if data_start + sections > length {
            return Err("VPK v2 数据段被截断".into());
        }
        u32::from_le_bytes(extra[..4].try_into().unwrap()) as u64
    } else {
        length - data_start
    };
    let mut tree = vec![0; tree_size as usize];
    file.read_exact(&mut tree).map_err(|e| e.to_string())?;
    let mut cursor = 0;
    let (mut info, mut cover) = (None, None);
    loop {
        let extension = string(&tree, &mut cursor)?;
        if extension.is_empty() {
            break;
        }
        loop {
            let folder = string(&tree, &mut cursor)?;
            if folder.is_empty() {
                break;
            }
            loop {
                let name = string(&tree, &mut cursor)?;
                if name.is_empty() {
                    break;
                }
                let entry = tree.get(cursor..cursor + 18).ok_or("VPK 目录项被截断")?;
                let preload = u16::from_le_bytes(entry[4..6].try_into().unwrap()) as usize;
                let archive = u16::from_le_bytes(entry[6..8].try_into().unwrap());
                let offset = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as u64;
                let size = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as u64;
                if entry[16..18] != [255, 255] {
                    return Err("VPK 目录终止标记无效".into());
                }
                cursor += 18;
                let preloaded = tree
                    .get(cursor..cursor + preload)
                    .ok_or("VPK 预载数据被截断")?;
                cursor += preload;
                let is_info = folder == b" "
                    && name.eq_ignore_ascii_case(b"addoninfo")
                    && extension.eq_ignore_ascii_case(b"txt");
                let is_cover = folder == b" "
                    && name.eq_ignore_ascii_case(b"addonimage")
                    && [b"jpg".as_slice(), b"jpeg", b"png", b"gif", b"webp"]
                        .iter()
                        .any(|ext| extension.eq_ignore_ascii_case(ext));
                if !is_info && !is_cover {
                    continue;
                }
                let limit = if is_info { 256 * 1024 } else { MAX_COVER };
                if preload as u64 + size > limit as u64 {
                    continue;
                }
                let mut bytes = preloaded.to_vec();
                if size > 0 {
                    let mut external;
                    let reader = if archive == 0x7fff {
                        if offset + size > payload_size {
                            return Err("VPK 元数据超出数据段".into());
                        }
                        file.seek(SeekFrom::Start(data_start + offset))
                            .map_err(|e| e.to_string())?;
                        &mut file
                    } else {
                        let stem = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .and_then(|s| s.strip_suffix("_dir"))
                            .ok_or("分卷 VPK 缺少 _dir 文件名")?;
                        external =
                            File::open(path.with_file_name(format!("{stem}_{archive:03}.vpk")))
                                .map_err(|e| e.to_string())?;
                        if offset + size > external.metadata().map_err(|e| e.to_string())?.len() {
                            return Err("VPK 分卷元数据被截断".into());
                        }
                        external
                            .seek(SeekFrom::Start(offset))
                            .map_err(|e| e.to_string())?;
                        &mut external
                    };
                    bytes.resize(preload + size as usize, 0);
                    reader
                        .read_exact(&mut bytes[preload..])
                        .map_err(|e| e.to_string())?;
                }
                if is_info {
                    info = Some(bytes);
                } else {
                    cover = Some(bytes);
                }
            }
        }
    }
    if cursor != tree.len() {
        return Err("VPK 目录尾部无效".into());
    }
    Ok((info, cover))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn workshop_links_require_exact_hosts_paths_and_positive_ids() {
        for value in [
            "123",
            " https://steamcommunity.com/sharedfiles/filedetails/?id=123&searchtext=x ",
            "https://steamcommunity.com/workshop/filedetails/?id=123#comments",
            "steam://url/CommunityFilePage/123",
        ] {
            assert_eq!(workshop_id(value).as_deref(), Some("123"), "{value}");
        }
        for value in [
            "0",
            "-1",
            "123abc",
            "steam://url/CommunityFilePage/+123",
            "steam://url/CommunityFilePage/steam://url/CommunityFilePage/123",
            "18446744073709551616",
            "https://steamcommunity.com.evil.test/sharedfiles/filedetails/?id=123",
            "https://user@steamcommunity.com/sharedfiles/filedetails/?id=123",
            "https://steamcommunity.com/sharedfiles/filedetails/?id=123&id=456",
            "https://steamcommunity.com/sharedfiles/filedetails/?id=",
            "https://steamcommunity.com/sharedfiles/filedetails/?id=https://steamcommunity.com/sharedfiles/filedetails/?id=123",
        ] {
            assert!(workshop_id(value).is_none(), "{value}");
        }
        let parsed = parse_info(br#"AddonInfo { addonTitle "Local" addonURL0 "https://steamcommunity.com/sharedfiles/filedetails/?id=123" }"#).unwrap();
        assert_eq!(parsed.workshop_id.as_deref(), Some("123"));
    }

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "aw-metadata-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn archive(
        version: u32,
        archive_index: u16,
        content: &[u8],
        preload: usize,
        cover: &[u8],
    ) -> Vec<u8> {
        let mut tree = b"txt\0 \0addoninfo\0".to_vec();
        tree.extend(0u32.to_le_bytes());
        tree.extend((preload as u16).to_le_bytes());
        tree.extend(archive_index.to_le_bytes());
        tree.extend(0u32.to_le_bytes());
        tree.extend(((content.len() - preload) as u32).to_le_bytes());
        tree.extend(0xffffu16.to_le_bytes());
        tree.extend(&content[..preload]);
        tree.extend([0, 0]);
        if !cover.is_empty() {
            tree.extend(b"jpg\0 \0addonimage\0");
            tree.extend(0u32.to_le_bytes());
            tree.extend((cover.len() as u16).to_le_bytes());
            tree.extend(0x7fffu16.to_le_bytes());
            tree.extend(0u32.to_le_bytes());
            tree.extend(0u32.to_le_bytes());
            tree.extend(0xffffu16.to_le_bytes());
            tree.extend(cover);
            tree.extend([0, 0]);
        }
        tree.push(0);
        let mut bytes = Vec::new();
        bytes.extend(0x55aa1234u32.to_le_bytes());
        bytes.extend(version.to_le_bytes());
        bytes.extend((tree.len() as u32).to_le_bytes());
        let data = if archive_index == 0x7fff {
            &content[preload..]
        } else {
            &[]
        };
        if version == 2 {
            bytes.extend((data.len() as u32).to_le_bytes());
            bytes.extend([0; 12]);
        }
        bytes.extend(tree);
        bytes.extend(data);
        bytes
    }

    #[test]
    fn reads_author_tags_without_losing_custom_names_or_splitting_spaces() {
        let text = r#""AddonInfo" {
            "addonTags" "Anime; Team Fortress 2,中文，ANIME|主题"
            "addonTag" "自定义"
            "tags" "主题；其他"
            "addonContent_CustomMode" "1"
            "addonContent_ignored" "0"
            "addonContent_weapon" "1"
        }"#;
        let metadata = parse_info(text.as_bytes()).unwrap();
        assert_eq!(metadata.local_tags, ["武器"]);
        assert_eq!(
            metadata.custom_tags,
            [
                "Anime",
                "Team Fortress 2",
                "中文",
                "主题",
                "自定义",
                "其他",
                "CustomMode"
            ]
        );
        assert!(metadata.has_tag("anime"));
        assert!(metadata.has_tag("custommode"));
        assert!(metadata.has_tag("武器"));
        assert!(!metadata.has_tag("ignored"));
    }

    #[test]
    fn reads_preloaded_and_inline_metadata_for_both_versions() {
        let f = Fixture::new();
        let info = "\"AddonInfo\" { addonTitle \"真实标题\" addonAuthor \"作者\" addonContent_Weapon 1 addonContent_Map 0 }".as_bytes();
        for version in [1, 2] {
            let path = f.0.join("123.vpk");
            fs::write(&path, archive(version, 0x7fff, info, 17, b"cover-bytes")).unwrap();
            let metadata = read(&path, true);
            assert_eq!(metadata.title.as_deref(), Some("真实标题"));
            assert_eq!(metadata.author, "作者");
            assert_eq!(metadata.local_tags, ["武器"]);
            assert_eq!(metadata.cover.unwrap().as_slice(), b"cover-bytes");
            assert_eq!(metadata.workshop_id.as_deref(), Some("123"));
            assert!(!metadata.workshop_verified);
        }
    }
    #[test]
    fn supports_metadata_in_external_chunk() {
        let f = Fixture::new();
        let path = f.0.join("maps_dir.vpk");
        let info = b"AddonInfo { addonTitle Maps }";
        fs::write(&path, archive(1, 0, info, 4, &[])).unwrap();
        fs::write(f.0.join("maps_000.vpk"), &info[4..]).unwrap();
        assert_eq!(read(&path, false).title.as_deref(), Some("Maps"));
        fs::write(f.0.join("maps_000.vpk"), b"truncated").unwrap();
        assert!(read(&path, false).title.is_none());
    }
    #[test]
    fn sidecars_are_read_in_local_directories_without_assuming_workshop() {
        let f = Fixture::new();
        for extension in ["jpg", "png", "jpeg", "gif", "webp"] {
            let path = f.0.join(format!("sample-{extension}.vpk"));
            fs::write(
                &path,
                archive(1, 0x7fff, b"AddonInfo { addonTitle Test }", 0, &[]),
            )
            .unwrap();
            fs::write(path.with_extension(extension), b"sidecar").unwrap();
            let metadata = read(&path, false);
            assert_eq!(metadata.cover.unwrap().as_slice(), b"sidecar");
            assert!(metadata.workshop_id.is_none());
            assert!(metadata.cover_source.contains(extension));
        }
    }

    #[test]
    fn numeric_local_files_are_not_assumed_to_be_workshop() {
        assert!(candidate_id(Path::new("123.vpk"), false).is_none());
        assert!(candidate_id(Path::new("123abc.vpk"), true).is_none());
        assert!(candidate_id(Path::new("0.vpk"), true).is_none());
        assert!(candidate_id(Path::new("99999999999999999999999999.vpk"), true).is_none());
        assert_eq!(
            candidate_id(Path::new("123.vpk"), true).as_deref(),
            Some("123")
        );
    }
    #[test]
    fn decodes_utf16_and_gbk_and_ignores_false_flags() {
        let text = "AddonInfo { addonTitle \"中文标题\" addonContent_Weapon false addonContent_Sound true }";
        let mut utf16 = vec![0xff, 0xfe];
        for ch in text.encode_utf16() {
            utf16.extend(ch.to_le_bytes());
        }
        let (gbk, _, _) = encoding_rs::GBK.encode(text);
        for bytes in [&utf16[..], &gbk[..]] {
            let metadata = parse_info(bytes).unwrap();
            assert_eq!(metadata.title.as_deref(), Some("中文标题"));
            assert_eq!(metadata.local_tags, ["音效"]);
        }
    }
    #[test]
    fn damaged_vpk_metadata_does_not_remove_file_from_scan() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("addons")).unwrap();
        fs::write(f.0.join("addons/broken.vpk"), b"truncated").unwrap();
        let addons = crate::addons::scan(&f.0).unwrap();
        assert_eq!(addons.len(), 1);
        assert!(!addons[0].metadata.note.is_empty());
        assert_eq!(addons[0].name, "broken.vpk");
    }
}
