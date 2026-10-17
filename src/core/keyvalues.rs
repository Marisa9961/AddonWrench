use std::ops::Range;

#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    pub span: Range<usize>,
    pub brace: bool,
}

/// Retain byte offsets so configuration updates preserve comments and layout.
pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        if bytes[i] == b'{' || bytes[i] == b'}' {
            out.push(Token {
                text: input[i..i + 1].into(),
                span: i..i + 1,
                brace: true,
            });
            i += 1;
        } else if bytes[i] == b'"' {
            i += 1;
            let mut text = String::new();
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'"' {
                    i += 1;
                    closed = true;
                    break;
                }
                if bytes[i] == b'\\' && i + 1 < bytes.len() && matches!(bytes[i + 1], b'"' | b'\\')
                {
                    i += 1;
                }
                let c = input[i..].chars().next().unwrap();
                text.push(c);
                i += c.len_utf8();
            }
            if !closed {
                return Err("KeyValues 引号未闭合".into());
            }
            out.push(Token {
                text,
                span: start..i,
                brace: false,
            });
        } else {
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && !matches!(bytes[i], b'{' | b'}' | b'"')
            {
                i += 1;
            }
            out.push(Token {
                text: input[start..i].into(),
                span: start..i,
                brace: false,
            });
        }
    }
    Ok(out)
}

pub fn normalized(name: &str) -> String {
    name.replace('\\', "/").to_lowercase()
}

pub struct AddonList {
    pub entries: Vec<(String, bool, Range<usize>)>,
    close: usize,
}

impl AddonList {
    pub fn parse(input: &str) -> Result<Self, String> {
        let tokens = tokenize(input)?;
        if tokens.len() < 3
            || tokens[0].brace
            || !tokens[0].text.eq_ignore_ascii_case("AddonList")
            || !tokens[1].brace
            || tokens[1].text != "{"
        {
            return Err("addonlist.txt 不是有效的 AddonList 配置，未修改文件".into());
        }
        let mut entries = Vec::new();
        let mut i = 2;
        while i < tokens.len() && !tokens[i].brace {
            let value = tokens.get(i + 1).ok_or("配置缺少启用状态")?;
            if value.brace || !matches!(value.text.as_str(), "0" | "1") {
                return Err("配置包含无效的启用状态".into());
            }
            entries.push((
                normalized(&tokens[i].text),
                value.text == "1",
                value.span.clone(),
            ));
            i += 2;
        }
        if i != tokens.len() - 1 || !tokens[i].brace || tokens[i].text != "}" {
            return Err("配置括号或条目格式错误".into());
        }
        Ok(Self {
            entries,
            close: tokens[i].span.start,
        })
    }

    pub fn enabled(&self, key: &str) -> bool {
        let key = normalized(key);
        self.entries
            .iter()
            .rev()
            .find(|e| e.0 == key)
            .map(|e| e.1)
            .unwrap_or(true)
    }

    pub fn update(&self, input: &str, changes: &[(String, bool)]) -> Result<String, String> {
        let mut edits = Vec::new();
        let mut additions = String::new();
        for (key, enabled) in changes {
            if key.contains(['"', '\n', '\r']) {
                return Err("模组文件名包含不支持的字符".into());
            }
            let key = normalized(key);
            let value = if *enabled { "1" } else { "0" };
            let matching: Vec<_> = self.entries.iter().filter(|e| e.0 == key).collect();
            if matching.is_empty() {
                additions.push_str(&format!("\t\"{}\"\t\"{value}\"\n", key.replace('/', "\\")));
            } else {
                for entry in matching {
                    edits.push((entry.2.clone(), format!("\"{value}\"")));
                }
            }
        }
        if !additions.is_empty() {
            if self.close > 0 && !input[..self.close].ends_with('\n') {
                additions.insert(0, '\n');
            }
            edits.push((self.close..self.close, additions));
        }
        edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
        let mut result = input.to_owned();
        for (span, replacement) in edits {
            result.replace_range(span, &replacement);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_comments_unknown_entries_and_unicode() {
        let text = "\u{feff}\"AddonList\"\r\n{\r\n// keep me\r\n\"workshop\\123.vpk\" \"0\"\r\n\"中文.vpk\" \"1\"\r\n\"missing.vpk\" \"0\"\r\n}";
        let list = AddonList::parse(text).unwrap();
        assert!(!list.enabled("WORKSHOP/123.VPK"));
        let updated = list.update(text, &[("中文.vpk".into(), false)]).unwrap();
        assert_eq!(
            updated,
            text.replace("\"中文.vpk\" \"1\"", "\"中文.vpk\" \"0\"")
        );
    }
    #[test]
    fn adds_missing_and_updates_duplicate_entries() {
        let text = "AddonList { \"A.vpk\" 1 \"a.vpk\" 1 }";
        let updated = AddonList::parse(text)
            .unwrap()
            .update(text, &[("a.vpk".into(), false), ("b.vpk".into(), true)])
            .unwrap();
        let list = AddonList::parse(&updated).unwrap();
        assert!(list.entries.iter().filter(|e| e.0 == "a.vpk").all(|e| !e.1));
        assert!(list.enabled("b.vpk"));
    }
    #[test]
    fn rejects_malformed_configuration() {
        for text in [
            "",
            "AddonList {",
            "AddonList { x 3 }",
            "AddonList { x }",
            "AddonList { } trailing",
            "AddonList { \"x 1 }",
            "Other {}",
        ] {
            assert!(AddonList::parse(text).is_err(), "{text}");
        }
    }
}
