//! The kinds of file a company deposits (ADR-095): PDF, Word (.docx),
//! Markdown and plain text. A file's kind is named by its extension and
//! confirmed by its first bytes, never taken from what the browser says.

use serde::{Deserialize, Serialize};

use super::Refusal;

/// A PDF starts so.
const PDF_MAGIC: &[u8] = b"%PDF-";
/// A ZIP archive, as a .docx is, starts so.
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
/// An OLE compound file: a Word document saved in the old format (.doc),
/// or protected by a password, starts so.
const OLE_MAGIC: &[u8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Pdf,
    Docx,
    Markdown,
    Text,
}

impl DocumentKind {
    pub const ALL: [DocumentKind; 4] = [Self::Pdf, Self::Docx, Self::Markdown, Self::Text];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Docx => "docx",
            Self::Markdown => "markdown",
            Self::Text => "text",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// The media type a file of this kind is served with.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Markdown => "text/markdown; charset=utf-8",
            Self::Text => "text/plain; charset=utf-8",
        }
    }

    /// The kind of the file named `name` holding `bytes`: named by its
    /// extension, confirmed by its first bytes.
    pub fn recognise(name: &str, bytes: &[u8]) -> Result<Self, Refusal> {
        let extension = name
            .rsplit_once('.')
            .map(|(_, extension)| extension.to_lowercase())
            .ok_or(Refusal::NotAccepted)?;
        let kind = match extension.as_str() {
            "pdf" => Self::Pdf,
            "docx" => Self::Docx,
            "md" | "markdown" => Self::Markdown,
            "txt" => Self::Text,
            "doc" => return Err(Refusal::OldOrProtectedWord),
            _ => return Err(Refusal::NotAccepted),
        };
        let binary = [PDF_MAGIC, ZIP_MAGIC, OLE_MAGIC]
            .iter()
            .any(|magic| bytes.starts_with(magic));
        match kind {
            Self::Pdf if bytes.starts_with(PDF_MAGIC) => Ok(kind),
            Self::Docx if bytes.starts_with(ZIP_MAGIC) => Ok(kind),
            Self::Docx if bytes.starts_with(OLE_MAGIC) => Err(Refusal::OldOrProtectedWord),
            Self::Markdown | Self::Text if !binary => Ok(kind),
            _ => Err(Refusal::Mismatch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_taken_by_its_extension_confirmed_by_its_first_bytes() {
        let pdf = b"%PDF-1.7\n...";
        assert_eq!(
            DocumentKind::recognise("Rapport.PDF", pdf),
            Ok(DocumentKind::Pdf)
        );
        assert_eq!(
            DocumentKind::recognise("note.docx", b"PK\x03\x04rest"),
            Ok(DocumentKind::Docx)
        );
        assert_eq!(
            DocumentKind::recognise("a.md", b"# Titre"),
            Ok(DocumentKind::Markdown)
        );
        assert_eq!(
            DocumentKind::recognise("a.txt", b"texte"),
            Ok(DocumentKind::Text)
        );

        let html = b"<html><body>page</body></html>";
        assert_eq!(
            DocumentKind::recognise("faux.pdf", html),
            Err(Refusal::Mismatch)
        );
        assert_eq!(
            DocumentKind::recognise("faux.txt", pdf),
            Err(Refusal::Mismatch)
        );
        assert_eq!(
            DocumentKind::recognise("faux.docx", pdf),
            Err(Refusal::Mismatch)
        );
        let ole = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";
        for name in ["ancien.docx", "ancien.doc", "Rapport.DOC"] {
            assert_eq!(
                DocumentKind::recognise(name, ole),
                Err(Refusal::OldOrProtectedWord),
                "{name}"
            );
        }
        for name in ["photo.png", "page.html", "sans-extension", "archive.zip"] {
            assert_eq!(
                DocumentKind::recognise(name, b"texte"),
                Err(Refusal::NotAccepted),
                "{name}"
            );
        }
    }

    #[test]
    fn each_kind_is_named_back_as_it_is_parsed() {
        for kind in DocumentKind::ALL {
            assert_eq!(DocumentKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(DocumentKind::parse("exe"), None);
    }
}
