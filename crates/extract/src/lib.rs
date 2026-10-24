//! Reads the text of a document a company deposits (ADR-095): PDF, Word
//! (.docx), Markdown or plain text, as `mimorya_core::documents` names them.
//! It parses hostile input, so it runs in a process of its own
//! (`mimorya-extract`, `child`), confined by its limits, never in the API's:
//! a file that crashes it, loops or exhausts its memory loses that process
//! only. The text it gives is cleaned (`clean`), page by page for a PDF.

pub mod child;
mod clean;
mod docx;
mod pdf;
#[cfg(feature = "testing")]
pub mod testing;
mod text;

use mimorya_core::documents::{DocumentKind, Extracted, Refusal};

pub use clean::clean;

/// The text of `bytes`, a file of `kind`.
pub fn read(kind: DocumentKind, bytes: &[u8]) -> Result<Extracted, Refusal> {
    match kind {
        DocumentKind::Pdf => pdf::read_pdf(bytes),
        DocumentKind::Docx => docx::read_docx(bytes),
        DocumentKind::Markdown | DocumentKind::Text => text::read_text(bytes),
    }
}
