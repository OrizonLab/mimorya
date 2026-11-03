//! What a message quotes of a server's document (an issuer, an endpoint): a
//! server writes what it wants there, so it is quoted short and on one
//! line, its hidden characters spelled out, never as written.

use mimorya_core::plain_text::{capped, is_hidden};

const MAX_QUOTED_CHARS: usize = 120;

/// `value` as a message quotes it.
pub(super) fn quoted(value: &str) -> String {
    let visible: String = value
        .chars()
        .flat_map(|c| {
            let spelled: Vec<char> = if is_hidden(c) {
                c.escape_unicode().collect()
            } else {
                vec![c]
            };
            spelled
        })
        .collect();
    capped(&visible, MAX_QUOTED_CHARS)
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
