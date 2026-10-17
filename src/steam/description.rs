//! Convert Steam BBCode to readable plain text without loading embedded media.
pub fn plain_text(source: &str) -> String {
    let mut output = String::new();
    let mut rest = source;
    let mut hidden: Option<String> = None;
    while !rest.is_empty() {
        if rest.starts_with('[')
            && let Some(end) = rest
                .as_bytes()
                .iter()
                .take(2048)
                .position(|byte| *byte == b']')
        {
            let raw = &rest[1..end];
            let tag = raw
                .split(['=', ' ', '\t'])
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if let Some(media) = &hidden {
                if tag == format!("/{media}") {
                    hidden = None;
                }
                rest = &rest[end + 1..];
                continue;
            }
            if matches!(tag.as_str(), "img" | "youtube" | "previewyoutube" | "video") {
                hidden = Some(tag);
                rest = &rest[end + 1..];
                continue;
            }
            let name = tag.trim_start_matches('/');
            if matches!(
                name,
                "b" | "i" | "u" | "s" | "strike" | "spoiler" | "url" | "noparse"
            ) {
                rest = &rest[end + 1..];
                continue;
            }
            if matches!(
                name,
                "h1" | "h2"
                    | "h3"
                    | "p"
                    | "list"
                    | "olist"
                    | "quote"
                    | "code"
                    | "table"
                    | "tr"
                    | "hr"
                    | "br"
                    | "*"
            ) {
                output.push('\n');
                if tag == "*" {
                    output.push_str("• ");
                }
                rest = &rest[end + 1..];
                continue;
            }
            if matches!(name, "td" | "th") {
                output.push(' ');
                rest = &rest[end + 1..];
                continue;
            }
        }
        let ch = rest.chars().next().unwrap();
        if hidden.is_none() {
            output.push(ch);
        }
        rest = &rest[ch.len_utf8()..];
    }
    let mut decoded = String::new();
    let mut rest = output.as_str();
    while !rest.is_empty() {
        if rest.starts_with('&')
            && let Some(end) = rest
                .as_bytes()
                .iter()
                .take(16)
                .position(|byte| *byte == b';')
        {
            let entity = &rest[1..end];
            let ch = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                _ => entity
                    .strip_prefix("#x")
                    .and_then(|n| u32::from_str_radix(n, 16).ok())
                    .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse().ok()))
                    .and_then(char::from_u32),
            };
            if let Some(ch) = ch {
                decoded.push(ch);
                rest = &rest[end + 1..];
                continue;
            }
        }
        let ch = rest.chars().next().unwrap();
        decoded.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    let mut lines = Vec::new();
    for line in decoded.lines() {
        let line = line.trim();
        if !line.is_empty() || lines.last().is_some_and(|last: &&str| !last.is_empty()) {
            lines.push(line);
        }
    }
    lines.join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_text_and_lists_without_embedded_media() {
        assert_eq!(
            plain_text(
                "[h1]说明[/h1][b]中文[/b] &amp; &#x41;[list][*]第一项[*][url=https://example.com]第二项[/url][/list][img]https://example.com/a.jpg[/img][previewyoutube=123]video[/previewyoutube]"
            ),
            "说明\n中文 & A\n\n• 第一项\n• 第二项"
        );
    }
    #[test]
    fn malformed_and_literal_brackets_remain_readable() {
        assert_eq!(
            plain_text("text [unknown] [1, 2] &broken;\n\n\nend ["),
            "text [unknown] [1, 2] &broken;\n\nend ["
        );
        assert_eq!(plain_text("before[img]unclosed image"), "before");
    }
}
