//! One visible line of text, as a company writes it in collaborator
//! attributes and in policy values: in canonical form (NFC) and without hidden
//! characters, so that two values that look the same compare equal.

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
