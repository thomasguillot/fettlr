use std::collections::BTreeMap;

pub(crate) fn markdown_body(markdown: &str) -> String {
    let normalized = markdown
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut lines = normalized
        .split('\n')
        .skip_while(|line| line.trim().is_empty());
    if lines.next().is_some_and(|opening| opening.trim() == "---") {
        let rest: Vec<&str> = lines.collect();
        if let Some(closing) = rest
            .iter()
            .position(|line| matches!(line.trim(), "---" | "..."))
        {
            return rest[closing + 1..].join("\n").trim().to_string();
        }
    }
    normalized.trim().to_string()
}

pub fn parse_frontmatter(markdown: &str) -> BTreeMap<String, String> {
    // U+FEFF is not `char::is_whitespace`, so an editor that writes a byte order mark would leave
    // the opening fence unrecognisable and discard the whole block.
    let normalized = markdown
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut lines = normalized
        .split('\n')
        .skip_while(|line| line.trim().is_empty());

    match lines.next() {
        Some(opening) if opening.trim() == "---" => {}
        _ => return BTreeMap::new(),
    }

    let mut fields = BTreeMap::new();
    let mut block: Option<(String, char)> = None;
    let mut plain: Option<(String, String)> = None;
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "..." {
            return fields;
        }
        if trimmed.is_empty() {
            continue;
        }
        if is_indented(line) {
            if let Some((key, style)) = &block {
                let folded = fields.entry(key.clone()).or_insert_with(String::new);
                if !folded.is_empty() {
                    folded.push(if *style == '|' { '\n' } else { ' ' });
                }
                folded.push_str(trimmed);
            }
            if let Some((key, raw)) = &mut plain
                && !trimmed.starts_with('#')
            {
                raw.push(' ');
                raw.push_str(trimmed);
                fields.insert(key.clone(), unquoted(raw).to_string());
            }
            continue;
        }
        block = None;
        plain = None;
        if trimmed.starts_with('#') {
            continue;
        }
        let Some(separator) = trimmed.find(':') else {
            continue;
        };
        let key = trimmed[..separator].trim();
        if key.is_empty() {
            continue;
        }
        let value = trimmed[separator + 1..].trim();
        // A block scalar's `>` or `|` introduces the value on the lines below it. Storing the
        // indicator itself would put a literal ">" on screen where a description belongs.
        if let Some(style) = block_style(value) {
            block = Some((key.to_string(), style));
            fields.insert(key.to_string(), String::new());
            continue;
        }
        fields.insert(key.to_string(), unquoted(value).to_string());
        if !value.is_empty() {
            plain = Some((key.to_string(), value.to_string()));
        }
    }

    BTreeMap::new()
}

fn is_indented(line: &str) -> bool {
    line.starts_with(' ') || line.starts_with('\t')
}

fn block_style(value: &str) -> Option<char> {
    let mut characters = value.chars();
    let style = characters
        .next()
        .filter(|first| *first == '>' || *first == '|')?;
    characters
        .all(|rest| rest == '-' || rest == '+' || rest.is_ascii_digit())
        .then_some(style)
}

fn unquoted(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() < 2 {
        return value;
    }
    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    if first == last && (first == b'"' || first == b'\'') {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_starts_after_the_closing_fence() {
        let markdown = "---\nname: a\n---\n\n# Title\n\nText\n";
        assert_eq!(markdown_body(markdown), "# Title\n\nText");
    }

    #[test]
    fn a_file_without_frontmatter_is_all_body() {
        assert_eq!(markdown_body("\n# Title\n\nText\n"), "# Title\n\nText");
    }

    #[test]
    fn a_bom_and_crlf_do_not_change_the_body() {
        let plain = "---\nname: a\n---\n\n# Title\n\nText\n";
        let windows = format!("\u{feff}{}", plain.replace('\n', "\r\n"));
        assert_eq!(markdown_body(&windows), markdown_body(plain));
    }
}
