//! One visible line of text, as a company writes it in collaborator
//! attributes and in policy values: in canonical form (NFC) and without hidden
//! characters, so that two values that look the same compare equal. Texts the
//! model reads hide nothing either: refused when an administrator writes them,
//! taken out of what a third party declares (an MCP server) or returns (a
//! tool's result, counted). A long text handed to the model is cut at a
//! bound, and says so.

use unicode_normalization::UnicodeNormalization;
use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

/// The canonical (NFC) form: the same word typed on two systems is the same.
pub fn canonical(text: &str) -> String {
    text.nfc().collect()
}

/// Characters that break a single visible line or hide in it: controls,
/// invisible formatting (zero-width space, direction overrides…), line or
/// paragraph separators, and the other characters Unicode ignores by default
/// when drawing (grapheme joiner, variation selectors, Hangul fillers…). A
/// value shown « administratif » must be exactly that.
pub fn is_hidden(c: char) -> bool {
    matches!(
        c.general_category(),
        GeneralCategory::Control
            | GeneralCategory::Format
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator
    ) || is_default_ignorable(c)
}

/// Whether a text an administrator writes for the model shows all it holds:
/// no hidden character but line breaks and tabs. What the administrator reads
/// is what the model reads (a skill's content, ADR-052; a tool's
/// description, ADR-057).
pub fn shows_all_it_holds(text: &str) -> bool {
    !text.chars().any(|c| c != '\n' && c != '\t' && is_hidden(c))
}

/// The text without its hidden characters, line breaks and tabs aside: what
/// a third party writes for the model (an MCP server's declarations,
/// ADR-058) then shows all it holds.
pub fn without_hidden(text: &str) -> String {
    text.chars()
        .filter(|&c| c == '\n' || c == '\t' || !is_hidden(c))
        .collect()
}

/// What a third party returned, as the model reads it (a tool's result,
/// ADR-158): line breaks (`\r\n` read as `\n`) and tabs kept, every other
/// hidden character taken out, and how many were.
pub fn shown(text: &str) -> (String, usize) {
    let mut removed = 0;
    let shown = text
        .replace("\r\n", "\n")
        .chars()
        .filter(|&c| {
            let kept = c == '\n' || c == '\t' || !is_hidden(c);
            removed += usize::from(!kept);
            kept
        })
        .collect();
    (shown, removed)
}

/// The first `max` characters of `text`, followed by « […] » when it held
/// more: what the model rereads of a run (ADR-076) or of a message folded
/// into a conversation's summary (ADR-145).
pub fn capped(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{head} […]")
    } else {
        head
    }
}

/// Unicode's `Default_Ignorable_Code_Point` (DerivedCoreProperties).
fn is_default_ignorable(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F..=0x1160
            | 0x17B4..=0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0xFFF0..=0xFFF8
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_text_is_cut_at_its_bound_and_says_so() {
        assert_eq!(capped("été", 3), "été");
        assert_eq!(capped("étés", 3), "été […]");
        assert_eq!(capped("", 0), "");
    }

    #[test]
    fn takes_hidden_characters_out_but_line_breaks_and_tabs() {
        let hidden = "Cherche\u{200b} une\u{202e} procédure\u{e0041}\u{e0042}.\nRéponse :\tune liste\u{feff}";
        let kept = without_hidden(hidden);
        assert_eq!(kept, "Cherche une procédure.\nRéponse :\tune liste");
        assert!(shows_all_it_holds(&kept));
        assert!(!shows_all_it_holds(hidden));
    }

    #[test]
    fn what_a_third_party_returned_shows_all_it_holds_and_counts_the_rest() {
        // An instruction spelt in tag characters, a direction override and a
        // zero-width space: four characters taken out.
        let returned = "Rapport\u{e0049}\u{e0047}\u{202e} du\u{200b} jour.\r\nFin\t.\n";
        let (text, removed) = shown(returned);
        assert_eq!(text, "Rapport du jour.\nFin\t.\n");
        assert_eq!(removed, 4);
        assert!(shows_all_it_holds(&text));
        // A line ending is not counted; a lone carriage return is.
        assert_eq!(shown("a\r\nb"), ("a\nb".to_string(), 0));
        assert_eq!(shown("a\rb"), ("ab".to_string(), 1));
        assert_eq!(shown("Rien à cacher."), ("Rien à cacher.".to_string(), 0));
    }
}
