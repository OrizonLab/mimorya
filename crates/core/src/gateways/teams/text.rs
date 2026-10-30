//! How the bot's text is written in Teams (ADR-138): in the Markdown Teams
//! reads (`textFormat: markdown`), which also reads some HTML. What the
//! platform did not write itself (a name) is escaped, so that nothing in it
//! reads as markup; a link is written only from an address the platform
//! made, which holds nothing Markdown would read.

/// `text` as Teams shows it literally: HTML's `&`, `<` and `>` written as
/// entities, Markdown's own characters escaped.
pub fn escaped(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.'
            | '!' | '|' | '~' => {
                escaped.push('\\');
                escaped.push(c);
            }
            c if c.is_control() => escaped.push(' '),
            c => escaped.push(c),
        }
    }
    escaped
}

/// A link to `url` reading `label` (written by the platform): `None` when
/// the address holds anything that would end the link or read as markup.
pub fn link(url: &str, label: &str) -> Option<String> {
    let safe = url.starts_with("https://") || url.starts_with("http://");
    let plain = url.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '/' | ':' | '.' | '-' | '_' | '#' | '~' | '=' | '%')
    });
    (safe && plain).then(|| format!("[{}]({url})", escaped(label)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_reads_as_written_never_as_markup() {
        assert_eq!(
            escaped("Assistant <b>*Néo*</b> & [co](x)"),
            "Assistant &lt;b&gt;\\*Néo\\*&lt;/b&gt; &amp; \\[co\\]\\(x\\)"
        );
        assert_eq!(escaped("a\u{0007}b"), "a b");
        assert_eq!(escaped("1. # titre"), "1\\. \\# titre");
    }

    #[test]
    fn a_link_is_written_only_from_a_plain_address() {
        assert_eq!(
            link(
                "https://app.example/account/gateway-link#a.b_c-d",
                "relier mon compte"
            )
            .as_deref(),
            Some("[relier mon compte](https://app.example/account/gateway-link#a.b_c-d)")
        );
        for url in [
            "javascript:alert(1)",
            "https://app.example/a)b",
            "https://app.example/a b",
            "https://app.example/<b>",
            "ftp://app.example/",
        ] {
            assert_eq!(link(url, "x"), None, "{url}");
        }
    }
}
