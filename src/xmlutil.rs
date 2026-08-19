/// Namespace-tolerant XML helpers. HP firmware mixes prefixes (`wscn:`, `scc:`,
/// `scan:`) and default xmlns; we only ever match local names.

pub fn local_name(raw: &str) -> &str {
    raw.rsplit(':').next().unwrap_or(raw)
}

/// Walk `xml` and collect every text node whose enclosing element's local name
/// equals `name` (case-sensitive, prefix stripped).
pub fn texts_named(xml: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut buf = String::new();
    let bytes = xml.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if !buf.trim().is_empty() {
                if stack.last().map(|s| s.as_str()) == Some(name) {
                    out.push(buf.trim().to_string());
                }
            }
            buf.clear();
            let start = i + 1;
            if start < bytes.len() && bytes[start] == b'!' {
                if bytes.get(start + 1) == Some(&b'-') {
                    if let Some(end) = find_sub(bytes, i, b"-->") {
                        i = end + 3;
                        continue;
                    }
                }
                if let Some(end) = find_byte(bytes, start, b'>') {
                    i = end + 1;
                    continue;
                }
            }
            if start < bytes.len() && bytes[start] == b'?' {
                if let Some(end) = find_sub(bytes, i, b"?>") {
                    i = end + 2;
                    continue;
                }
            }
            if let Some(end) = find_byte(bytes, start, b'>') {
                let tag = std::str::from_utf8(&bytes[start..end]).unwrap_or("");
                if let Some(stripped) = tag.strip_prefix('/') {
                    let ln = local_name(stripped.split_whitespace().next().unwrap_or(""));
                    if stack.last().map(|s| s.as_str()) == Some(ln) {
                        stack.pop();
                    }
                    i = end + 1;
                    continue;
                }
                let self_close = tag.ends_with('/');
                let name_part = tag.trim_end_matches('/').split_whitespace().next().unwrap_or("");
                let ln = local_name(name_part).to_string();
                if !self_close && !ln.is_empty() {
                    stack.push(ln);
                }
                i = end + 1;
                continue;
            }
            break;
        } else {
            buf.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

pub fn first_text(xml: &str, name: &str) -> Option<String> {
    texts_named(xml, name).into_iter().next()
}

pub fn contains_local(xml: &str, name: &str) -> bool {
    let needle_a = format!("<{name}");
    let needle_b = format!(":{name}");
    xml.contains(&needle_a) || xml.split('<').any(|chunk| {
        let tag = chunk.split(|c: char| c == '>' || c == ' ' || c == '/').next().unwrap_or("");
        local_name(tag) == name
    }) || xml.contains(&needle_b)
}

/// Attribute value `attr="..."` or `attr='...'` on the first element whose
/// local name is `element`.
pub fn attr_on(xml: &str, element: &str, attr: &str) -> Option<String> {
    let bytes = xml.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let start = i + 1;
        if start >= bytes.len() {
            break;
        }
        if matches!(bytes[start], b'/' | b'!' | b'?') {
            i += 1;
            continue;
        }
        if let Some(end) = find_byte(bytes, start, b'>') {
            let tag = std::str::from_utf8(&bytes[start..end]).unwrap_or("");
            let name_part = tag.trim_end_matches('/').split_whitespace().next().unwrap_or("");
            if local_name(name_part) == element {
                return parse_attr(tag, attr);
            }
            i = end + 1;
        } else {
            break;
        }
    }
    None
}

fn parse_attr(tag: &str, attr: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let key = format!("{attr}={quote}");
        if let Some(idx) = tag.find(&key) {
            let rest = &tag[idx + key.len()..];
            if let Some(end) = rest.find(quote) {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

fn find_byte(hay: &[u8], from: usize, b: u8) -> Option<usize> {
    hay[from..].iter().position(|&c| c == b).map(|p| p + from)
}

fn find_sub(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_prefixes_and_finds_text() {
        let xml = r#"<wscn:ScanElements><item>jfif</item><item>hpraw</item></wscn:ScanElements>"#;
        assert_eq!(texts_named(xml, "item"), ["jfif", "hpraw"]);
        assert_eq!(first_text(xml, "ScanElements").as_deref(), None);
        assert!(contains_local(xml, "ScanElements"));
    }
}
