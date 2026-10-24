//! A PDF: its text page by page, by pdf-extract over lopdf. One that opens
//! without a password is read, permissions or not; one that needs a
//! password is refused, as one beyond `MAX_PAGES`.

use lopdf::Document;
use mimorya_core::documents::limits::MAX_PAGES;
use mimorya_core::documents::{Extracted, Refusal};
use pdf_extract::{PlainTextOutput, output_doc_page};

use crate::clean;

pub fn read_pdf(bytes: &[u8]) -> Result<Extracted, Refusal> {
    let mut document = Document::load_mem(bytes).map_err(refusal)?;
    // Encrypted with an empty password (restrictions only): readable.
    if document.is_encrypted() && document.decrypt("").is_err() {
        return Err(Refusal::Encrypted);
    }
    let pages = document.get_pages();
    if pages.len() > MAX_PAGES {
        return Err(Refusal::TooManyPages);
    }
    let mut texts = Vec::with_capacity(pages.len());
    for number in pages.keys() {
        let mut text = String::new();
        output_doc_page(&document, &mut PlainTextOutput::new(&mut text), *number)
            .map_err(|_| Refusal::Unreadable)?;
        texts.push(clean(&text));
    }
    Extracted::checked(texts, true)
}

fn refusal(error: lopdf::Error) -> Refusal {
    match error {
        lopdf::Error::Decryption(_) | lopdf::Error::InvalidPassword => Refusal::Encrypted,
        _ => Refusal::Unreadable,
    }
}
