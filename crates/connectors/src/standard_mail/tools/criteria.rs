//! What `mail_search` asks the IMAP server (ADR-205): each word somewhere
//! in the e-mail (`TEXT`), received since a day (`SINCE`), as a command the
//! model cannot change. Its words lose quotes, backslashes and control
//! characters (a line break would end the command and start another). A
//! word beyond ASCII goes in UTF-8 (`CHARSET UTF-8`): as a literal the
//! server takes without a pause (`{n+}`, RFC 7888) where it offers one,
//! otherwise quoted, as IMAP4rev2 allows.

use chrono::NaiveDate;

/// The most words searched.
pub const MAX_WORDS: usize = 8;

/// The words and the day of a search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criteria {
    words: Vec<String>,
    since: Option<NaiveDate>,
}

impl Criteria {
    pub fn new(query: Option<&str>, since: Option<NaiveDate>) -> Self {
        let cleaned: String = query
            .unwrap_or_default()
            .chars()
            .map(|c| {
                if c == '"' || c == '\\' || c.is_control() {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        let words = cleaned
            .split_whitespace()
            .take(MAX_WORDS)
            .map(String::from)
            .collect();
        Self { words, since }
    }

    /// Nothing to search: the latest e-mails are listed.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.since.is_none()
    }

    /// A word goes beyond ASCII: the search is in UTF-8.
    pub fn in_utf8(&self) -> bool {
        self.words.iter().any(|word| !word.is_ascii())
    }

    /// The arguments of `UID SEARCH`; `literal_plus` when the server takes
    /// literals without a pause.
    pub fn command(&self, literal_plus: bool) -> String {
        let utf8 = self.in_utf8();
        let mut parts = Vec::new();
        if utf8 {
            parts.push("CHARSET UTF-8".to_string());
        }
        for word in &self.words {
            let written = if !word.is_ascii() && literal_plus {
                format!("{{{}+}}\r\n{word}", word.len())
            } else {
                format!("\"{word}\"")
            };
            parts.push(format!("TEXT {written}"));
        }
        if let Some(day) = self.since {
            parts.push(format!("SINCE {}", day.format("%-d-%b-%Y")));
        }
        if parts.is_empty() {
            parts.push("ALL".into());
        }
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn words_cannot_end_the_command() {
        let criteria = Criteria::new(
            Some("facture\r\nA9 DELETE INBOX \"x\\\" mars"),
            Some(day("2026-03-05")),
        );
        let command = criteria.command(true);
        assert_eq!(
            command,
            "TEXT \"facture\" TEXT \"A9\" TEXT \"DELETE\" TEXT \"INBOX\" TEXT \"x\" \
             TEXT \"mars\" SINCE 5-Mar-2026"
        );
        assert!(!command.contains('\n'));
    }

    #[test]
    fn a_word_beyond_ascii_goes_in_utf8() {
        let criteria = Criteria::new(Some("réunion lundi"), None);
        assert_eq!(
            criteria.command(true),
            "CHARSET UTF-8 TEXT {8+}\r\nréunion TEXT \"lundi\""
        );
        assert_eq!(
            criteria.command(false),
            "CHARSET UTF-8 TEXT \"réunion\" TEXT \"lundi\""
        );
    }

    #[test]
    fn words_are_bounded_and_nothing_searched_lists_all() {
        let many = (0..20).map(|n| format!("m{n}")).collect::<Vec<_>>();
        let criteria = Criteria::new(Some(&many.join(" ")), None);
        assert_eq!(criteria.command(false).matches("TEXT").count(), MAX_WORDS);
        let none = Criteria::new(Some(" \" \\ "), None);
        assert!(none.is_empty());
        assert_eq!(none.command(false), "ALL");
    }
}
