//! Reading a Word document (ADR-095): its paragraphs, titles marked for the
//! passages; an archive too large once inflated, of too many entries or
//! without a body refused; no entity expanded but XML's own.

use mimorya_core::documents::limits::{MAX_ARCHIVE_ENTRIES, MAX_COMPRESSION_RATIO};
use mimorya_core::documents::{DocumentKind, Refusal};
use mimorya_extract::read;
use mimorya_extract::testing::{archive, body, docx};

fn text(file: &[u8]) -> String {
    read(DocumentKind::Docx, file).unwrap().pages.concat()
}

#[test]
fn paragraphs_and_titles_are_read() {
    let file = docx(
        "<w:p><w:pPr><w:pStyle w:val=\"Titre1\"/></w:pPr><w:r><w:t>Procédure</w:t></w:r></w:p>\
         <w:p><w:r><w:t xml:space=\"preserve\">Couper </w:t></w:r><w:r><w:t>l’alimentation.</w:t></w:r></w:p>\
         <w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:t>Contrôle</w:t></w:r></w:p>\
         <w:p><w:r><w:t>Pression</w:t><w:tab/><w:t>2 bars</w:t><w:br/><w:t>Relevé.</w:t></w:r></w:p>",
    );
    let read = read(DocumentKind::Docx, &file).unwrap();
    assert!(!read.paged);
    assert_eq!(
        read.pages,
        ["# Procédure\n\nCouper l’alimentation.\n\n## Contrôle\n\nPression 2 bars\nRelevé."]
    );
}

#[test]
fn xml_s_own_entities_are_read_no_other_is_expanded() {
    let file = docx("<w:p><w:r><w:t>R&amp;D &lt;3 &#233;t&#xE9;</w:t></w:r></w:p>");
    assert_eq!(text(&file), "R&D <3 été");
    let declared = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE d [<!ENTITY x \"PIÈGE\">]>{}",
        body("<w:p><w:r><w:t>avant &x; après</w:t></w:r></w:p>")
            .trim_start_matches("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>")
    );
    let file = archive(&[("word/document.xml", declared.as_bytes())]);
    assert_eq!(text(&file), "avant  après");
}

#[test]
fn an_archive_inflating_beyond_its_ratio_is_refused() {
    let spaces = " ".repeat(5_000_000);
    let inflating = body(&format!("<w:p><w:r><w:t>{spaces}</w:t></w:r></w:p>"));
    let file = archive(&[("word/document.xml", inflating.as_bytes())]);
    // Spaces compress well beyond the ratio a document's text reaches.
    assert!(file.len() as u64 * MAX_COMPRESSION_RATIO < inflating.len() as u64);
    assert_eq!(read(DocumentKind::Docx, &file), Err(Refusal::Unreadable));
}

#[test]
fn an_archive_of_too_many_entries_or_without_a_body_is_refused() {
    let names: Vec<String> = (0..=MAX_ARCHIVE_ENTRIES)
        .map(|i| format!("media/{i}.txt"))
        .collect();
    let mut entries: Vec<(&str, &[u8])> = names
        .iter()
        .map(|name| (name.as_str(), &b"x"[..]))
        .collect();
    let content = body("<w:p><w:r><w:t>Texte</w:t></w:r></w:p>");
    entries.push(("word/document.xml", content.as_bytes()));
    assert_eq!(
        read(DocumentKind::Docx, &archive(&entries)),
        Err(Refusal::Unreadable)
    );

    let without_body = archive(&[("word/styles.xml", b"<styles/>")]);
    assert_eq!(
        read(DocumentKind::Docx, &without_body),
        Err(Refusal::Unreadable)
    );
    assert_eq!(
        read(DocumentKind::Docx, b"PK\x03\x04broken"),
        Err(Refusal::Unreadable)
    );
    let empty = docx("<w:p><w:r><w:t>  </w:t></w:r></w:p>");
    assert_eq!(read(DocumentKind::Docx, &empty), Err(Refusal::NoText));
}
