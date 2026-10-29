//! Text the bot writes in Telegram (ADR-135), in the HTML Telegram reads
//! (`parse_mode: HTML`): `<b>`, `<i>`, `<code>`, `<pre>` and `<a href>`,
//! nothing else, each tag closed where it opens. What is quoted has `&`,
//! `<` and `>` escaped, as Telegram asks: nothing in it becomes a tag. An
//! agent's answer, written in Markdown, keeps its emphasis, headings, list
//! items and web links; code, in spans or fenced blocks, is kept as written.

use crate::gateways::markdown::{BOLD, HEADING, ITALIC, ITEM, LINK};

/// Where a bold span opens and closes, while its text is read: never in
/// a text, which loses them first.
const BOLD_OPEN: char = '\u{1}';
const BOLD_CLOSE: char = '\u{2}';

/// `text` quoted in Telegram's HTML: `&`, `<` and `>` escaped.
pub fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A link to `url` labelled `label`, in Telegram's HTML.
pub fn link(url: &str, label: &str) -> String {
    let href = escaped(url).replace('"', "&quot;");
    format!("<a href=\"{href}\">{}</a>", escaped(label))
}

/// `markdown` (an agent's answer) in Telegram's HTML, escaped: bold and
/// headings in `<b>`, italic in `<i>`, list items as `•`, web links in
/// `<a>`, code spans in `<code>`, fenced blocks in `<pre>` (one left open
/// is closed).
pub fn from_markdown(markdown: &str) -> String {
    let markdown: String = (markdown.chars())
        .filter(|&c| c != BOLD_OPEN && c != BOLD_CLOSE)
        .collect();
    let mut lines = Vec::new();
    let mut code: Option<Vec<String>> = None;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            match code.take() {
                Some(block) => lines.push(format!("<pre>{}</pre>", block.join("\n"))),
                None => code = Some(Vec::new()),
            }
        } else if let Some(block) = code.as_mut() {
            block.push(escaped(line));
        } else if let Some(heading) = HEADING.captures(line) {
            lines.push(format!("<b>{}</b>", inline(&heading[1])));
        } else if let Some(item) = ITEM.captures(line) {
            lines.push(format!("{}• {}", &item[1], inline(&item[2])));
        } else {
            lines.push(inline(line));
        }
    }
    if let Some(block) = code {
        lines.push(format!("<pre>{}</pre>", block.join("\n")));
    }
    lines.join("\n")
}

/// A line's text, its closed code spans in `<code>`; a backtick left open
/// is kept as written.
fn inline(line: &str) -> String {
    let parts: Vec<&str> = line.split('`').collect();
    // An even count of parts: the last backtick opens no span.
    let closed = if parts.len().is_multiple_of(2) {
        parts.len() - 1
    } else {
        parts.len()
    };
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i >= closed {
            out.push('`');
            out.push_str(&prose(part));
        } else if i % 2 == 1 {
            out.push_str(&format!("<code>{}</code>", escaped(part)));
        } else {
            out.push_str(&prose(part));
        }
    }
    out
}

/// Text outside code: its web links in `<a>`, the rest escaped with its
/// emphasis.
fn prose(text: &str) -> String {
    let mut out = String::new();
    let mut rest = 0;
    for found in LINK.captures_iter(text) {
        let whole = found.get(0).expect("the match");
        out.push_str(&emphasis(&escaped(&text[rest..whole.start()])));
        out.push_str(&link(&found[2], &found[1]));
        rest = whole.end();
    }
    out.push_str(&emphasis(&escaped(&text[rest..])));
    out
}

fn emphasis(text: &str) -> String {
    // Bold first, marked apart, so that its stars are not taken for italic:
    // an italic span holds a bold one whole, or none of it.
    let marked = BOLD.replace_all(text, |caps: &regex::Captures<'_>| {
        let inner = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map_or("", |m| m.as_str());
        format!("{BOLD_OPEN}{inner}{BOLD_CLOSE}")
    });
    let italic = ITALIC.replace_all(&marked, "<i>${1}</i>");
    italic.replace(BOLD_OPEN, "<b>").replace(BOLD_CLOSE, "</b>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_text_cannot_become_a_tag() {
        assert_eq!(
            escaped("<b>gras</b> & <a href=\"x\">"),
            "&lt;b&gt;gras&lt;/b&gt; &amp; &lt;a href=\"x\"&gt;"
        );
        assert_eq!(
            from_markdown("Prévenez <b>tous</b> & <script>"),
            "Prévenez &lt;b&gt;tous&lt;/b&gt; &amp; &lt;script&gt;"
        );
    }

    #[test]
    fn emphasis_headings_and_lists_are_written_in_html() {
        let markdown = "## Interventions\n**Demain** : *trois* visites\n- Lyon __Nord__\n  * Pompe P2\n+ *Ville **urbaine***";
        assert_eq!(
            from_markdown(markdown),
            "<b>Interventions</b>\n<b>Demain</b> : <i>trois</i> visites\n• Lyon <b>Nord</b>\n  • Pompe P2\n• <i>Ville <b>urbaine</b></i>"
        );
        // A star alone is no emphasis; the marks of bold are never a text's.
        assert_eq!(from_markdown("2 * 3 = 6"), "2 * 3 = 6");
        assert_eq!(from_markdown("a\u{1}b\u{2}c **d**"), "abc <b>d</b>");
    }

    #[test]
    fn web_links_become_links_and_nothing_else_does() {
        assert_eq!(
            from_markdown("Voir [le <rapport>](https://x.example/r?a=1&b=\"2\") ici"),
            "Voir <a href=\"https://x.example/r?a=1&amp;b=&quot;2&quot;\">le &lt;rapport&gt;</a> ici"
        );
        assert_eq!(
            from_markdown("[c](javascript:alert(1))"),
            "[c](javascript:alert(1))"
        );
        assert_eq!(
            link("https://app.example/chat/1", "ouvrir"),
            "<a href=\"https://app.example/chat/1\">ouvrir</a>"
        );
    }

    #[test]
    fn code_is_kept_as_written() {
        let markdown = "Lancez `**pas** <gras>` puis :\n```sh\n- ligne <1>\n**brut**\n```\n- fin";
        assert_eq!(
            from_markdown(markdown),
            "Lancez <code>**pas** &lt;gras&gt;</code> puis :\n<pre>- ligne &lt;1&gt;\n**brut**</pre>\n• fin"
        );
        // A span or a block left open: the backtick kept, the block closed.
        assert_eq!(from_markdown("un ` seul **gras**"), "un ` seul <b>gras</b>");
        assert_eq!(from_markdown("`a` et `b"), "<code>a</code> et `b");
        assert_eq!(
            from_markdown("```\nouvert <x>"),
            "<pre>ouvert &lt;x&gt;</pre>"
        );
    }

    #[test]
    fn every_tag_opened_is_closed() {
        let markdown = "**a *b** c* `d **e` [f](https://x.example) *g __h__* ```\n**i";
        let html = from_markdown(markdown);
        for tag in ["b", "i", "code", "pre", "a"] {
            let opened = html.matches(&format!("<{tag}>")).count()
                + html.matches(&format!("<{tag} ")).count();
            let closed = html.matches(&format!("</{tag}>")).count();
            assert_eq!(opened, closed, "{tag}: {html}");
        }
    }
}
