//! Text the application writes in Slack (ADR-125, ADR-127), in Slack's own
//! markup (« mrkdwn »). What is quoted has `&`, `<` and `>` escaped, as
//! Slack asks: nothing in it becomes a link, a mention (`<!channel>`) or a
//! command. An agent's answer, written in Markdown, keeps its emphasis,
//! headings, lists and web links, in Slack's notation; code is kept as
//! written. A person's message, as Slack writes it, is read back as
//! plain text for the agent (`plain`).

use regex::Regex;
use std::sync::LazyLock;

use crate::gateways::markdown::{BOLD, HEADING, ITALIC, ITEM, LINK};

/// What Slack writes between `<` and `>`: a link, a mention, a channel, a
/// command, with its label after `|`.
static MARKUP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<([^<>|\s]{1,2000})(?:\|([^<>]{0,300}))?>").expect("valid regex")
});

/// `text` quoted in Slack's markup: `&`, `<` and `>` escaped, as Slack asks.
pub fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `markdown` (an agent's answer) in Slack's markup, escaped: bold and
/// headings as `*…*`, italic as `_…_`, list items as `•`, web links as
/// `<url|label>`. Code, in spans or fenced blocks, is kept as written.
pub fn from_markdown(markdown: &str) -> String {
    let mut lines = Vec::new();
    let mut in_code = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            lines.push(escaped(line.trim()));
        } else if in_code {
            lines.push(escaped(line));
        } else if let Some(heading) = HEADING.captures(line) {
            lines.push(format!("*{}*", inline(&heading[1])));
        } else if let Some(item) = ITEM.captures(line) {
            lines.push(format!("{}• {}", &item[1], inline(&item[2])));
        } else {
            lines.push(inline(line));
        }
    }
    lines.join("\n")
}

/// A line's text, its code spans kept as written.
fn inline(line: &str) -> String {
    line.split('`')
        .enumerate()
        .map(|(i, part)| {
            // Odd parts are inside a code span (an unclosed one included).
            if i % 2 == 1 {
                escaped(part)
            } else {
                prose(part)
            }
        })
        .collect::<Vec<_>>()
        .join("`")
}

/// Text outside code: its links in Slack's notation, the rest escaped with
/// its emphasis.
fn prose(text: &str) -> String {
    let mut out = String::new();
    let mut rest = 0;
    for link in LINK.captures_iter(text) {
        let whole = link.get(0).expect("the match");
        out.push_str(&emphasis(&escaped(&text[rest..whole.start()])));
        let label = escaped(&link[1].replace('|', "/"));
        out.push_str(&format!("<{}|{label}>", escaped(&link[2])));
        rest = whole.end();
    }
    out.push_str(&emphasis(&escaped(&text[rest..])));
    out
}

fn emphasis(text: &str) -> String {
    // Bold first, marked apart, so that its stars are not taken for italic.
    let marked = BOLD.replace_all(text, |caps: &regex::Captures<'_>| {
        let inner = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map_or("", |m| m.as_str());
        format!("\u{1}{inner}\u{1}")
    });
    let italic = ITALIC.replace_all(&marked, "_${1}_");
    italic.replace('\u{1}', "*")
}

/// A person's message, as Slack writes it, in plain text: a link as its
/// label and its address (an e-mail address alone), a mention, a channel
/// or a command by its label or name (`@…`, `#…`), the characters Slack
/// escapes unescaped.
pub fn plain(slack_text: &str) -> String {
    let read = MARKUP.replace_all(slack_text, |caps: &regex::Captures<'_>| {
        let target = &caps[1];
        let label = caps.get(2).map(|m| m.as_str()).filter(|l| !l.is_empty());
        if let Some(rest) = target.strip_prefix('@') {
            format!("@{}", label.unwrap_or(rest))
        } else if let Some(rest) = target.strip_prefix('#') {
            format!("#{}", label.unwrap_or(rest))
        } else if let Some(rest) = target.strip_prefix('!') {
            match label {
                Some(label) => label.to_owned(),
                None => format!("@{}", rest.split('^').next().unwrap_or(rest)),
            }
        } else if let Some(address) = target.strip_prefix("mailto:") {
            address.to_owned()
        } else {
            match label {
                Some(label) if label != target => format!("{label} ({target})"),
                _ => target.to_owned(),
            }
        }
    });
    read.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_text_cannot_mention_link_or_command() {
        assert_eq!(
            escaped("<!channel> & <https://x.example|clic>"),
            "&lt;!channel&gt; &amp; &lt;https://x.example|clic&gt;"
        );
        // Neither can an answer.
        assert_eq!(
            from_markdown("Prévenez <!here> & <@U123>"),
            "Prévenez &lt;!here&gt; &amp; &lt;@U123&gt;"
        );
    }

    #[test]
    fn emphasis_headings_and_lists_follow_slack_s_notation() {
        let markdown = "## Interventions\n**Demain** : *trois* visites\n- Lyon __Nord__\n  * Pompe P2\n+ Villeurbanne";
        assert_eq!(
            from_markdown(markdown),
            "*Interventions*\n*Demain* : _trois_ visites\n• Lyon *Nord*\n  • Pompe P2\n• Villeurbanne"
        );
        // A star alone is no emphasis.
        assert_eq!(from_markdown("2 * 3 = 6"), "2 * 3 = 6");
    }

    #[test]
    fn web_links_become_slack_s_and_nothing_else_does() {
        assert_eq!(
            from_markdown("Voir [le rapport](https://x.example/r?a=1&b=2) ici"),
            "Voir <https://x.example/r?a=1&amp;b=2|le rapport> ici"
        );
        // A label cannot end the link early; another scheme stays text.
        assert_eq!(
            from_markdown("[a|b](https://x.example) [c](javascript:alert(1))"),
            "<https://x.example|a/b> [c](javascript:alert(1))"
        );
    }

    #[test]
    fn code_is_kept_as_written() {
        let markdown = "Lancez `**pas** <gras>` puis :\n```\n- ligne <1>\n**brut**\n```\n- fin";
        assert_eq!(
            from_markdown(markdown),
            "Lancez `**pas** &lt;gras&gt;` puis :\n```\n- ligne &lt;1&gt;\n**brut**\n```\n• fin"
        );
    }

    #[test]
    fn a_message_is_read_back_as_plain_text() {
        assert_eq!(
            plain(
                "Rappelle <@U0LEA> et <!here> : voir <https://x.example/a?b=1&amp;c=2|le plan> &amp; <https://x.example>"
            ),
            "Rappelle @U0LEA et @here : voir le plan (https://x.example/a?b=1&c=2) & https://x.example"
        );
        assert_eq!(
            plain(
                "<#C024BE7LR|général> <!subteam^S0614TZR7|@equipe> <mailto:lea@x.example|lea@x.example>"
            ),
            "#général @equipe lea@x.example"
        );
        // What the person typed as `<` and `>` is theirs again.
        assert_eq!(plain("a &lt;b&gt; &amp;lt;"), "a <b> &lt;");
    }
}
