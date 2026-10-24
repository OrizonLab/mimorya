//! Reading a PDF (ADR-095): its text page by page, accents kept; one that
//! needs a password, has too many pages, holds no text or is no PDF at all,
//! refused, never a panic.

use mimorya_core::documents::limits::MAX_PAGES;
use mimorya_core::documents::{DocumentKind, Refusal};
use mimorya_extract::read;
use mimorya_extract::testing::{pdf, protected_pdf};

#[test]
fn a_pdf_is_read_page_by_page() {
    let file = pdf(&[
        "Rapport d'intervention : la chaudière est réparée.",
        "Coût : 120 euros.",
    ]);
    let read = read(DocumentKind::Pdf, &file).unwrap();
    assert!(read.paged);
    assert_eq!(
        read.pages,
        [
            "Rapport d'intervention : la chaudière est réparée.",
            "Coût : 120 euros."
        ]
    );
}

#[test]
fn a_pdf_restricted_by_its_owner_only_is_read_one_needing_a_password_is_not() {
    let restricted = protected_pdf(&["Note de service."], "");
    assert_eq!(
        read(DocumentKind::Pdf, &restricted).unwrap().pages,
        ["Note de service."]
    );
    let locked = protected_pdf(&["Secret."], "motdepasse");
    assert_eq!(read(DocumentKind::Pdf, &locked), Err(Refusal::Encrypted));
}

#[test]
fn a_pdf_beyond_its_pages_or_without_text_is_refused() {
    let pages = vec!["p"; MAX_PAGES + 1];
    assert_eq!(
        read(DocumentKind::Pdf, &pdf(&pages)),
        Err(Refusal::TooManyPages)
    );
    assert_eq!(
        read(DocumentKind::Pdf, &pdf(&["", " "])),
        Err(Refusal::NoText)
    );
}

#[test]
fn what_is_no_pdf_is_unreadable() {
    for bytes in [&b"%PDF-1.7\nnothing else"[..], b"", b"<html></html>"] {
        assert_eq!(read(DocumentKind::Pdf, bytes), Err(Refusal::Unreadable));
    }
    let mut cut = pdf(&["Texte complet."]);
    cut.truncate(cut.len() / 2);
    assert!(read(DocumentKind::Pdf, &cut).is_err());
}
