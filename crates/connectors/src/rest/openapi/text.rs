//! The words of an OpenAPI document as a REST declaration holds them: an
//! operation's name (`snake_case`, from its `operationId` or its method and
//! path, made unique), texts without hidden characters and within their
//! bounds, a cut said by `…`, and what a note may quote of the document.
//! Only the beginning of a text its bound may need is read.

use std::collections::HashSet;

use mimorya_core::plain_text::{is_hidden, without_hidden};

use crate::rest::checks::MAX_OPERATION_NAME;
use crate::rest::declaration::HttpMethod;

/// What a note quotes of a path or a name.
const MAX_QUOTED_CHARS: usize = 80;

/// The beginning of a text that a bound of `max` characters may need:
/// hidden characters may be taken out of it, never more than that.
#[derive(Clone, Copy)]
pub(super) struct Read<'t> {
    text: &'t str,
    /// Whether the text goes on beyond what is read.
    goes_on: bool,
}

impl<'t> Read<'t> {
    pub(super) fn bytes(&self) -> usize {
        self.text.len()
    }
}

/// What a bound of `max` characters reads of a text.
pub(super) fn beginning(text: &str, max: usize) -> Read<'_> {
    let read = max.saturating_mul(4).saturating_add(64);
    match text.char_indices().nth(read) {
        Some((at, _)) => Read {
            text: &text[..at],
            goes_on: true,
        },
        None => Read {
            text,
            goes_on: false,
        },
    }
}

/// The operation's name: its `operationId` in `snake_case`, or its method
/// and path when it has none that reads so.
pub(super) fn operation_name(id: Option<&str>, method: HttpMethod, path: &str) -> String {
    id.map(snake)
        .filter(|name| name.len() >= 2)
        .unwrap_or_else(|| snake(&format!("{} {path}", method.as_str())))
}

/// `getTicketByID` → `get_ticket_by_id`: lowercase letters and digits
/// separated by `_`, a letter first, at most the length of a name.
fn snake(text: &str) -> String {
    let mut name = String::new();
    let mut previous: Option<char> = None;
    for c in beginning(text, MAX_OPERATION_NAME).text.chars() {
        if c.is_ascii_alphanumeric() {
            let starts_word = c.is_ascii_uppercase()
                && previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit());
            if starts_word && !name.is_empty() && !name.ends_with('_') {
                name.push('_');
            }
            name.push(c.to_ascii_lowercase());
        } else if !name.is_empty() && !name.ends_with('_') {
            name.push('_');
        }
        previous = Some(c);
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert_str(0, "op_");
    }
    name.truncate(MAX_OPERATION_NAME);
    name.trim_end_matches('_').to_string()
}

/// The name, or the first of `name_2`, `name_3`… no other operation has.
pub(super) fn unique(name: &str, taken: &HashSet<String>) -> String {
    let mut candidate = name.to_string();
    let mut n = 2;
    while taken.contains(&candidate) {
        let suffix = format!("_{n}");
        let room = MAX_OPERATION_NAME - suffix.len();
        let base = name[..name.len().min(room)].trim_end_matches('_');
        candidate = format!("{base}{suffix}");
        n += 1;
    }
    candidate
}

/// The text read without hidden characters (`stripped` says if any was
/// taken out), line breaks and tabs kept, trimmed, cut at `max` characters.
pub(super) fn bounded(read: Read, max: usize, stripped: &mut bool) -> String {
    let text = read.text.replace("\r\n", "\n");
    let visible = without_hidden(&text);
    *stripped |= visible.len() != text.len();
    cut(visible.trim(), max, read.goes_on)
}

/// The text read on one line: as `bounded`, every run of blanks one space.
pub(super) fn one_line(read: Read, max: usize, stripped: &mut bool) -> String {
    let text = read.text.replace("\r\n", "\n");
    let visible = without_hidden(&text);
    *stripped |= visible.len() != text.len();
    let joined = visible.split_whitespace().collect::<Vec<_>>().join(" ");
    cut(&joined, max, read.goes_on)
}

/// What a note quotes of the document: on one line, short, a hidden
/// character shown as `�` rather than taken out, so that two names that
/// differ never read alike.
pub(super) fn quoted(text: &str) -> String {
    let shown: String = text
        .chars()
        .take(MAX_QUOTED_CHARS + 1)
        .map(|c| match c {
            c if c.is_whitespace() => ' ',
            c if is_hidden(c) => '\u{FFFD}',
            c => c,
        })
        .collect();
    cut(shown.trim(), MAX_QUOTED_CHARS, false)
}

/// At most `max` characters, the last one `…` when the text is cut or goes
/// on beyond what was read.
fn cut(text: &str, max: usize, goes_on: bool) -> String {
    if !goes_on && text.chars().count() <= max {
        return text.to_string();
    }
    let mut kept: String = text.chars().take(max.saturating_sub(1)).collect();
    kept.truncate(kept.trim_end().len());
    kept.push('…');
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_an_operation_as_a_tool_may_be_named() {
        let name = |id: Option<&str>, path| operation_name(id, HttpMethod::Get, path);
        assert_eq!(name(Some("getTicketByID"), "/x"), "get_ticket_by_id");
        assert_eq!(name(Some("tickets.list"), "/x"), "tickets_list");
        assert_eq!(name(Some("2fa-enable"), "/x"), "op_2fa_enable");
        assert_eq!(name(Some("_"), "/tickets/{id}"), "get_tickets_id");
        assert_eq!(name(Some("élément"), "/"), "l_ment");
        assert_eq!(name(None, "/"), "get");
        let long = name(Some(&"a_".repeat(40)), "/x");
        assert!(
            long.len() <= MAX_OPERATION_NAME && !long.ends_with('_'),
            "{long}"
        );
        let taken: HashSet<String> = ["get".to_string(), "get_2".to_string()].into();
        assert_eq!(unique("get", &taken), "get_3");
        assert_eq!(unique("list", &taken), "list");
        let full = "a".repeat(MAX_OPERATION_NAME);
        let again = unique(&full, &[full.clone()].into());
        assert_eq!(again.len(), MAX_OPERATION_NAME);
        assert!(again.ends_with("_2"));
    }

    #[test]
    fn keeps_texts_visible_and_bounded() {
        let mut stripped = false;
        let text = " Lire\r\nun\u{200b} ticket ";
        assert_eq!(
            bounded(beginning(text, 100), 100, &mut stripped),
            "Lire\nun ticket"
        );
        assert!(stripped);
        let mut stripped = false;
        let text = "Lire\n\tun  ticket";
        assert_eq!(
            one_line(beginning(text, 100), 100, &mut stripped),
            "Lire un ticket"
        );
        assert!(!stripped);
        let text = "Lire un ticket";
        assert_eq!(one_line(beginning(text, 6), 6, &mut stripped), "Lire…");
        assert_eq!(quoted(&"é".repeat(200)).chars().count(), MAX_QUOTED_CHARS);
        assert_eq!(quoted("/a\u{202e}b\nc"), "/a\u{fffd}b c");
    }

    #[test]
    fn reads_only_the_beginning_a_bound_needs() {
        // Hidden characters up front, then words: what is read is cut there,
        // and the cut is said.
        let text = format!("{}{}", "\u{200b}".repeat(100), "mot ".repeat(1000));
        let read = beginning(&text, 10);
        assert!(read.bytes() < 500, "{}", read.bytes());
        let mut stripped = false;
        let kept = one_line(read, 10, &mut stripped);
        assert!(stripped);
        assert_eq!(kept, "mot…");
        // Hidden characters only: nothing, said as cut.
        let hidden = "\u{200b}".repeat(1000);
        assert_eq!(one_line(beginning(&hidden, 10), 10, &mut false), "…");
    }
}
