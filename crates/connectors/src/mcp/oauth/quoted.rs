//! What a message quotes of a server's document (an issuer, an endpoint): a
//! server writes what it wants there, so it is quoted on one line, its
//! hidden characters spelled out, never as written, and short: 120
//! characters as shown at most, a spelling never cut.

use mimorya_core::plain_text::is_hidden;

const MAX_QUOTED_CHARS: usize = 120;

/// `value` as a message quotes it: as much of it as 120 characters show,
/// each hidden character spelled out whole, followed by « […] » when it
/// held more. Only what is shown is read.
pub(super) fn quoted(value: &str) -> String {
    let mut shown = String::new();
    let mut length = 0;
    for c in value.chars() {
        let spelled: String = if is_hidden(c) {
            c.escape_unicode().collect()
        } else {
            c.to_string()
        };
        let width = spelled.chars().count();
        if length + width > MAX_QUOTED_CHARS {
            shown.push_str(" […]");
            return shown;
        }
        length += width;
        shown.push_str(&spelled);
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
        assert_eq!(long, format!("{} […]", "a".repeat(MAX_QUOTED_CHARS)));
        // Spellings count as shown, and are never cut.
        let hidden = quoted(&"\u{200b}".repeat(200));
        assert_eq!(hidden, format!("{} […]", "\\u{200b}".repeat(15)));
        let after = quoted(&format!("a{}", "\u{202e}".repeat(200)));
        assert_eq!(after, format!("a{} […]", "\\u{202e}".repeat(14)));
        // Exactly as long as the bound: whole, without the mark.
        assert_eq!(
            quoted(&"a".repeat(MAX_QUOTED_CHARS)),
            "a".repeat(MAX_QUOTED_CHARS)
        );
    }
}
