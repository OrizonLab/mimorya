//! An agent's answer as the bot writes it in Teams (ADR-140), in the
//! Markdown Teams reads: rebuilt from what the gateways read of it
//! (emphasis, headings, list items, web links), everything else escaped
//! (`escaped`), so that nothing in it reads as Markdown the agent did not
//! write, nor as HTML. A heading is written in bold, a list item as `- `;
//! emphasis is not nested (a span holds no other). Code,
//! in spans or fenced blocks, is kept as written, its HTML characters
//! escaped; a fenced block left open is closed.

use super::text::escaped;
use crate::gateways::markdown::{BOLD, HEADING, ITALIC, ITEM, LINK};

/// `markdown` (an agent's answer) in Teams' Markdown, escaped.
pub fn from_markdown(markdown: &str) -> String {
    let mut lines = Vec::new();
    let mut code: Option<Vec<String>> = None;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            match code.take() {
                Some(block) => lines.push(fenced(&block)),
                None => code = Some(Vec::new()),
            }
        } else if let Some(block) = code.as_mut() {
            block.push(html_escaped(line));
        } else if let Some(heading) = HEADING.captures(line) {
            lines.push(format!("**{}**", escaped(&heading[1])));
        } else if let Some(item) = ITEM.captures(line) {
            lines.push(format!("{}- {}", &item[1], inline(&item[2])));
        } else {
            lines.push(inline(line));
        }
    }
    if let Some(block) = code {
        lines.push(fenced(&block));
    }
    lines.join("\n")
}

fn fenced(block: &[String]) -> String {
    format!("```\n{}\n```", block.join("\n"))
}

/// What only HTML would read, written as entities.
fn html_escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A line's text, its closed code spans kept; a backtick left open is
/// written escaped.
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
            out.push_str("\\`");
            out.push_str(&prose(part));
        } else if i % 2 == 1 {
            out.push_str(&format!("`{}`", html_escaped(part)));
        } else {
            out.push_str(&prose(part));
        }
    }
    out
}

/// Text outside code: its web links kept, the rest escaped with its
/// emphasis.
fn prose(text: &str) -> String {
    let mut out = String::new();
    let mut rest = 0;
    for found in LINK.captures_iter(text) {
        let whole = found.get(0).expect("the match");
        out.push_str(&emphasis(&text[rest..whole.start()]));
        out.push_str(&web_link(&found[2], &found[1]));
        rest = whole.end();
    }
    out.push_str(&emphasis(&text[rest..]));
    out
}

/// A web link the agent wrote: kept when its address holds nothing that
/// Markdown or HTML would read; otherwise its label, then its address,
/// both escaped.
fn web_link(url: &str, label: &str) -> String {
    let plain = url.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(
                c,
                '/' | ':'
                    | '.'
                    | '-'
                    | '_'
                    | '#'
                    | '~'
                    | '='
                    | '%'
                    | '?'
                    | '&'
                    | '+'
                    | ','
                    | ';'
                    | '@'
                    | '!'
                    | '$'
            )
    });
    if plain {
        format!("[{}]({url})", escaped(label))
    } else {
        format!("{} ({})", escaped(label), escaped(url))
    }
}

/// `text`'s bold spans, then the italic ones between them, kept; all
/// escaped.
fn emphasis(text: &str) -> String {
    let mut out = String::new();
    let mut rest = 0;
    for found in BOLD.captures_iter(text) {
        let whole = found.get(0).expect("the match");
        let inner = found
            .get(1)
            .or_else(|| found.get(2))
            .map_or("", |m| m.as_str());
        out.push_str(&italic(&text[rest..whole.start()]));
        out.push_str(&format!("**{}**", escaped(inner)));
        rest = whole.end();
    }
    out.push_str(&italic(&text[rest..]));
    out
}

fn italic(text: &str) -> String {
    let mut out = String::new();
    let mut rest = 0;
    for found in ITALIC.captures_iter(text) {
        let whole = found.get(0).expect("the match");
        out.push_str(&escaped(&text[rest..whole.start()]));
        out.push_str(&format!("*{}*", escaped(&found[1])));
        rest = whole.end();
    }
    out.push_str(&escaped(&text[rest..]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_keeps_its_markdown_and_nothing_else() {
        assert_eq!(
            from_markdown("**Trois** visites, dont *une* <demain> & 2 [ici](x)."),
            "**Trois** visites, dont *une* &lt;demain&gt; &amp; 2 \\[ici\\]\\(x\\)\\."
        );
        assert_eq!(
            from_markdown("## Planning\n- Lyon : `B-12`\n  * Paris"),
            "**Planning**\n- Lyon : `B-12`\n  - Paris"
        );
        assert_eq!(
            from_markdown("Voir [le site](https://ex.example/a?b=1&c=2)."),
            "Voir [le site](https://ex.example/a?b=1&c=2)\\."
        );
    }

    #[test]
    fn nothing_the_agent_did_not_write_as_markdown_reads_as_markup() {
        // HTML, and a link whose address would close it or hold markup.
        assert_eq!(
            from_markdown("<img src=x onerror=alert(1)>"),
            "&lt;img src=x onerror=alert\\(1\\)&gt;"
        );
        assert_eq!(
            from_markdown("[clic](https://ex.example/\"a*b\")"),
            "clic (https://ex\\.example/\"a\\*b\")"
        );
        // An underscore, a lone star, a backtick left open.
        assert_eq!(from_markdown("a_b * c `d"), "a\\_b \\* c \\`d");
        // Bold, then italic.
        assert_eq!(from_markdown("**a** et *b*"), "**a** et *b*");
    }

    #[test]
    fn code_is_kept_as_written_its_html_escaped() {
        assert_eq!(
            from_markdown("```\nif a < b { *x }\n```\nfin"),
            "```\nif a &lt; b { *x }\n```\nfin"
        );
        // A block left open is closed.
        assert_eq!(from_markdown("```\nlet a = 1;"), "```\nlet a = 1;\n```");
    }
}
