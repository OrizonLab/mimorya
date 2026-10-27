//! Text of what Google gives as HTML (ADR-122): an e-mail's snippet, its
//! entities decoded, and the text of an e-mail or an event's description
//! written in HTML, its tags taken out. Not a renderer: what the model
//! reads, from an answer already bounded.

use std::sync::LazyLock;

use regex::{Captures, Regex};

/// Comments, and blocks whose content is not text.
static HIDDEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?is)<!--.*?-->|<script\b.*?</script\s*>|<style\b.*?</style\s*>|<head\b.*?</head\s*>",
    )
    .expect("valid regex")
});
/// Where a line ends in what the HTML shows.
static BREAKS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<\s*(br|/p|/div|/li|/tr|/h[1-6]|/blockquote)\b[^>]*>").expect("valid regex")
});
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").expect("valid regex"));
static ENTITIES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"&(#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[a-zA-Z]{2,8});").expect("valid regex")
});

/// The text an HTML document shows, its lines trimmed, blank lines kept
/// one at a time.
pub fn html_to_text(html: &str) -> String {
    let shown = HIDDEN.replace_all(html, "");
    let broken = BREAKS.replace_all(&shown, "\n");
    let bare = TAGS.replace_all(&broken, "");
    tidy(&decode_entities(&bare))
}

/// The text with its HTML entities decoded; one unknown is kept as is.
pub fn decode_entities(text: &str) -> String {
    ENTITIES
        .replace_all(text, |found: &Captures| {
            let name = &found[1];
            let decoded = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => numeric(name),
            };
            decoded.map_or_else(|| found[0].to_string(), String::from)
        })
        .into_owned()
}

fn numeric(name: &str) -> Option<char> {
    let code = match name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => name.strip_prefix('#')?.parse().ok()?,
    };
    char::from_u32(code).filter(|c| !c.is_control() || c.is_whitespace())
}

/// Lines trimmed, spaces within a line collapsed, at most one blank line
/// between two.
pub fn tidy(text: &str) -> String {
    let mut kept: Vec<String> = Vec::new();
    for line in text.replace("\r\n", "\n").split('\n') {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() && kept.last().is_none_or(String::is_empty) {
            continue;
        }
        kept.push(line);
    }
    while kept.last().is_some_and(String::is_empty) {
        kept.pop();
    }
    kept.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_are_decoded_and_unknown_ones_kept() {
        assert_eq!(
            decode_entities("L&#39;offre &amp; la facture &lt;3&gt; &#xE9;t&eacute;"),
            "L'offre & la facture <3> ét&eacute;"
        );
        // A control character is not made of an entity.
        assert_eq!(decode_entities("a&#0;b&#27;c"), "a&#0;b&#27;c");
    }

    #[test]
    fn html_reads_as_its_text() {
        let html = "<html><head><title>T</title></head><body><style>p{}</style>\
                    <p>Bonjour&nbsp;Léa,</p><p>Voici   la <b>facture</b>.<br>Merci</p>\
                    <!-- caché --><script>alert(1)</script><div></div><div>Fin</div></body></html>";
        assert_eq!(
            html_to_text(html),
            "Bonjour Léa,\nVoici la facture.\nMerci\n\nFin"
        );
    }

    #[test]
    fn blank_lines_are_kept_one_at_a_time() {
        assert_eq!(tidy("\n\n a \r\n\r\n\n  b  c \n\n"), "a\n\nb c");
    }
}
