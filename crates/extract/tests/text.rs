//! Reading a Markdown or text file (ADR-095): UTF-8 only, its byte order
//! mark dropped, what hides in it taken out; binary or empty, refused.

use mimorya_core::documents::limits::MAX_TEXT_CHARS;
use mimorya_core::documents::{DocumentKind, Refusal};
use mimorya_extract::read;

#[test]
fn utf8_text_is_read_cleaned() {
    let file = "\u{FEFF}# Consignes\r\n\r\nNe pas\u{200B} ouvrir\u{202E} la vanne.\r\n".as_bytes();
    let read = read(DocumentKind::Markdown, file).unwrap();
    assert!(!read.paged);
    assert_eq!(read.pages, ["# Consignes\n\nNe pas ouvrir la vanne."]);
}

#[test]
fn text_not_utf8_binary_empty_or_too_long_is_refused() {
    assert_eq!(read(DocumentKind::Text, b"caf\xe9"), Err(Refusal::NotUtf8));
    assert_eq!(read(DocumentKind::Text, b"a\0b"), Err(Refusal::Unreadable));
    assert_eq!(read(DocumentKind::Text, b" \n\t\n"), Err(Refusal::NoText));
    let long = "a".repeat(MAX_TEXT_CHARS + 1);
    assert_eq!(
        read(DocumentKind::Text, long.as_bytes()),
        Err(Refusal::TooLong)
    );
}
