//! The words of an OpenAPI document as a REST declaration holds them: an
//! operation's name (`snake_case`, from its `operationId` or its method and
//! path, made unique), texts without hidden characters and within their
//! bounds, a cut said by `…`, and what a note may quote of the document.

use std::collections::HashSet;

use mimorya_core::plain_text::without_hidden;

use crate::rest::checks::MAX_OPERATION_NAME;
use crate::rest::declaration::HttpMethod;

/// What a note quotes of a path or a name.
const MAX_QUOTED_CHARS: usize = 80;

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
    for c in text.chars() {
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

/// The text without hidden characters (`stripped` says if any was taken
/// out), line breaks and tabs kept, trimmed, cut at `max` characters.
pub(super) fn bounded(text: &str, max: usize, stripped: &mut bool) -> String {
    let text = text.replace("\r\n", "\n");
    let visible = without_hidden(&text);
    *stripped |= visible.chars().count() != text.chars().count();
    cut(visible.trim(), max)
}

/// The text on one line: as `bounded`, every run of blanks one space.
pub(super) fn one_line(text: &str, max: usize, stripped: &mut bool) -> String {
    let text = bounded(text, usize::MAX, stripped);
    cut(&text.split_whitespace().collect::<Vec<_>>().join(" "), max)
}

/// What a note quotes of the document: on one line, visible, short.
pub(super) fn quoted(text: &str) -> String {
    one_line(text, MAX_QUOTED_CHARS, &mut false)
}

/// At most `max` characters, the last one `…` when the text is cut.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
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
        assert_eq!(
            bounded(" Lire\r\nun\u{200b} ticket ", 100, &mut stripped),
            "Lire\nun ticket"
        );
        assert!(stripped);
        let mut stripped = false;
        assert_eq!(
            one_line("Lire\n\tun  ticket", 100, &mut stripped),
            "Lire un ticket"
        );
        assert!(!stripped);
        assert_eq!(one_line("Lire un ticket", 6, &mut stripped), "Lire…");
        assert_eq!(quoted(&"é".repeat(200)).chars().count(), MAX_QUOTED_CHARS);
    }
}
