//! What the gateways read of an agent's answer, written in Markdown
//! (ADR-127, ADR-135): its web links, emphasis, headings and list items,
//! which each service writes in its own notation (`slack::from_markdown`,
//! `telegram::from_markdown`); and how a text too long for one message is
//! cut where it can be read, saying so (`bounded`).

use regex::Regex;
use std::sync::LazyLock;

/// A web link in Markdown: `[label](https://…)`.
pub(crate) static LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[([^\]\n]{1,300})\]\((https?://[^\s()<>|]{1,2000})\)").expect("valid regex")
});
/// Bold, `**…**` or `__…__`.
pub(crate) static BOLD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*\*([^*\n]+)\*\*|__([^_\n]+)__").expect("valid regex"));
/// Italic with single stars, `*…*`.
pub(crate) static ITALIC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*([^*\s][^*\n]*)\*").expect("valid regex"));
/// A heading, `# …` to `###### …`.
pub(crate) static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#{1,6}\s+(.+?)\s*#*\s*$").expect("valid regex"));
/// A list item, `- …`, `* …` or `+ …`.
pub(crate) static ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)[-*+]\s+(.*)$").expect("valid regex"));

/// What closes a text that was cut.
const CUT: &str = "…";

/// At most `max` characters of `text`, cut at the end of a line or a word
/// when one is near, a fenced code block left open closed, `…` added;
/// whether it was cut.
pub fn bounded(text: &str, max: usize) -> (String, bool) {
    if text.chars().count() <= max {
        return (text.to_owned(), false);
    }
    let head: String = text.chars().take(max).collect();
    // Back to a line's end, else a word's, within the last fifth.
    let floor = head.len() - head.len() / 5;
    let end = head
        .rfind('\n')
        .filter(|at| *at >= floor)
        .or_else(|| head.rfind(' ').filter(|at| *at >= floor))
        .unwrap_or(head.len());
    let mut cut = head[..end].trim_end().to_owned();
    if cut
        .lines()
        .filter(|l| l.trim_start().starts_with("```"))
        .count()
        % 2
        == 1
    {
        cut.push_str("\n```");
    }
    cut.push_str(CUT);
    (cut, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_text_is_cut_where_it_can_be_read() {
        assert_eq!(bounded("court", 10), ("court".to_owned(), false));
        let text = format!("{}\nsuite de la réponse", "a".repeat(90));
        let (cut, was) = bounded(&text, 100);
        assert!(was);
        assert_eq!(cut, format!("{}…", "a".repeat(90)));
        // No line nor word end near: cut where it must be.
        let (cut, _) = bounded(&"é".repeat(50), 10);
        assert_eq!(cut, format!("{}…", "é".repeat(10)));
        // A code block left open is closed.
        let code = format!("```\n{}", "ligne\n".repeat(30));
        let (cut, _) = bounded(&code, 40);
        assert!(cut.ends_with("\n```…"), "{cut}");
    }
}
