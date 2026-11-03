//! What a message quotes of a server's document (an issuer, an endpoint): a
//! server writes what it wants there, so it is quoted short and on one
//! line, its hidden characters spelled out, never as written.

use mimorya_core::plain_text::is_hidden;

const MAX_QUOTED_CHARS: usize = 120;

/// `value` as a message quotes it: its first characters, each hidden one
/// spelled out, the cut falling between two of them, followed by « […] »
/// when it held more.
pub(super) fn quoted(value: &str) -> String {
    let mut chars = value.chars();
    let mut shown = String::new();
    for c in chars.by_ref().take(MAX_QUOTED_CHARS) {
        if is_hidden(c) {
            shown.extend(c.escape_unicode());
        } else {
            shown.push(c);
        }
    }
    if chars.next().is_some() {
        shown.push_str(" […]");
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quoted_value_is_short_and_on_one_line() {
        assert_eq!(
            quoted("https://auth.example.com"),
            "https://auth.example.com"
        );
        assert_eq!(quoted("Appelez\nURGENT"), "Appelez\\u{a}URGENT");
        assert_eq!(quoted("a\u{202e}b"), "a\\u{202e}b");
        let long = quoted(&"a".repeat(100_000));
        assert_eq!(
            long.chars().count(),
            MAX_QUOTED_CHARS + " […]".chars().count()
        );
    }
}
