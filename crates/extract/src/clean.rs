//! A document's text as the agent reads it: in canonical form (NFC), each
//! line without the characters that hide in it (controls, invisible
//! formatting, direction overrides), tabs as spaces, at most one blank line
//! in a row. What a document hides, it cannot slip to the model.

use mimorya_core::plain_text::{canonical, is_hidden};

pub fn clean(text: &str) -> String {
    let text = canonical(text)
        .replace("\r\n", "\n")
        .replace(['\r', '\u{2028}', '\u{2029}', '\u{000B}', '\u{000C}'], "\n");
    let mut cleaned = String::with_capacity(text.len());
    let mut blank = true;
    for line in text.split('\n') {
        let line: String = line
            .chars()
            .map(|c| if c == '\t' { ' ' } else { c })
            .filter(|c| !is_hidden(*c))
            .collect();
        let line = line.trim_end();
        if line.trim().is_empty() {
            if !blank {
                cleaned.push('\n');
            }
            blank = true;
            continue;
        }
        cleaned.push_str(line);
        cleaned.push('\n');
        blank = false;
    }
    cleaned.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_characters_go_lines_and_paragraphs_stay() {
        let text =
            "Ligne\u{200B} une\r\nLigne\tdeux\u{202E}\r\n\r\n\r\n\r\nParagraphe\u{2028}suite  ";
        assert_eq!(clean(text), "Ligne une\nLigne deux\n\nParagraphe\nsuite");
        // Canonical: « é » typed as e and its accent is one character.
        assert_eq!(clean("e\u{301}t\u{e9}"), "\u{e9}t\u{e9}");
        assert_eq!(clean("\n\n  \n"), "");
    }
}
