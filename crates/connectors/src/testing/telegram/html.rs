//! The HTML the test Telegram reads (ADR-135), as Telegram does: only the
//! tags it knows, each closed where it opens, and `&` starting one of its
//! entities.

/// Whether Telegram reads `text` as HTML: only the tags it knows (`<a>`
/// with its `href` alone), each closed where it opens, and `&` starting
/// one of its entities.
pub(super) fn readable_html(text: &str) -> bool {
    const TAGS: [&str; 5] = ["b", "i", "code", "pre", "a"];
    const ENTITIES: [&str; 4] = ["&amp;", "&lt;", "&gt;", "&quot;"];
    let mut open: Vec<&str> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(['<', '&', '>']) {
        let tail = &rest[at..];
        if tail.starts_with('>') {
            return false;
        }
        if tail.starts_with('&') {
            let Some(entity) = ENTITIES.iter().find(|e| tail.starts_with(**e)) else {
                return false;
            };
            rest = &tail[entity.len()..];
            continue;
        }
        let Some(end) = tail.find('>') else {
            return false;
        };
        let tag = &tail[1..end];
        if let Some(name) = tag.strip_prefix('/') {
            if open.pop() != Some(name) {
                return false;
            }
        } else {
            let (name, attributes) = tag.split_once(' ').unwrap_or((tag, ""));
            let href = attributes
                .strip_prefix("href=\"")
                .and_then(|a| a.strip_suffix('"'));
            let attributes_read = match name {
                "a" => href.is_some_and(|h| !h.contains('"')),
                _ => attributes.is_empty(),
            };
            if !TAGS.contains(&name) || !attributes_read {
                return false;
            }
            open.push(name);
        }
        rest = &tail[end + 1..];
    }
    open.is_empty()
}

#[cfg(test)]
mod tests {
    use super::readable_html;

    #[test]
    fn only_html_telegram_reads_is_taken() {
        assert!(readable_html(
            "<b>a</b> <i>b <b>c</b></i> <code>&lt;d&gt;</code> <pre>e &amp; f</pre> \
             <a href=\"https://x.example/?a=1&amp;b=&quot;2&quot;\">g</a>"
        ));
        for text in [
            "<b>ouvert",
            "<b>a</i>",
            "</b>",
            "<u>souligné</u>",
            "<b class=\"x\">a</b>",
            "<a>lien</a>",
            "a & b",
            "a > b",
            "<b",
        ] {
            assert!(!readable_html(text), "{text}");
        }
    }
}
