//! What reading a deposited file gives (ADR-095): its text, page by page
//! when it has pages; or why it is refused, in words its depositor reads.
//! The reader (`mimorya-extract`) answers so, as JSON, on its output.

use serde::{Deserialize, Serialize};

use super::limits::{MAX_FILE_BYTES, MAX_PAGES, MAX_TEXT_CHARS};

/// Why a file is not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    #[error("Ce type de fichier n’est pas accepté : PDF, Word (.docx), Markdown ou texte.")]
    NotAccepted,
    #[error("Le contenu du fichier ne correspond pas à son extension.")]
    Mismatch,
    #[error(
        "Ce document Word est protégé par un mot de passe ou enregistré dans l’ancien format \
         (.doc) : enregistrez-le en .docx, sans mot de passe."
    )]
    OldOrProtectedWord,
    #[error("Le fichier dépasse {} Mo.", MAX_FILE_BYTES / (1024 * 1024))]
    TooLarge,
    #[error("Le fichier est illisible ou endommagé.")]
    Unreadable,
    #[error("Le fichier est protégé par un mot de passe.")]
    Encrypted,
    #[error("Le fichier texte n’est pas encodé en UTF-8.")]
    NotUtf8,
    #[error("Le document dépasse {MAX_PAGES} pages.")]
    TooManyPages,
    #[error("Le texte du document dépasse {MAX_TEXT_CHARS} caractères.")]
    TooLong,
    #[error("Le document ne contient aucun texte lisible : un document numérisé n’est pas lu.")]
    NoText,
}

/// A file's text: one entry per page of a PDF, a single one otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extracted {
    pub pages: Vec<String>,
    /// Whether the entries are pages, numbered from 1.
    pub paged: bool,
}

impl Extracted {
    /// `pages`, read: refused when they hold no text, or too much.
    pub fn checked(pages: Vec<String>, paged: bool) -> Result<Self, Refusal> {
        let chars: usize = pages.iter().map(|page| page.chars().count()).sum();
        if chars > MAX_TEXT_CHARS {
            return Err(Refusal::TooLong);
        }
        if pages.iter().all(|page| page.trim().is_empty()) {
            return Err(Refusal::NoText);
        }
        Ok(Self { pages, paged })
    }
}

/// The reader's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Reading {
    Read(Extracted),
    Refused { reason: Refusal },
}

impl From<Result<Extracted, Refusal>> for Reading {
    fn from(result: Result<Extracted, Refusal>) -> Self {
        match result {
            Ok(extracted) => Self::Read(extracted),
            Err(reason) => Self::Refused { reason },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_refused_when_empty_or_too_long() {
        let blank = vec![" \n".to_string(), String::new()];
        assert_eq!(Extracted::checked(blank, true), Err(Refusal::NoText));
        let long = vec![
            "é".repeat(MAX_TEXT_CHARS / 2),
            "a".repeat(MAX_TEXT_CHARS / 2 + 1),
        ];
        assert_eq!(Extracted::checked(long, true), Err(Refusal::TooLong));
        let read = Extracted::checked(vec!["Bonjour".into()], false).unwrap();
        assert_eq!(read.pages, ["Bonjour"]);
    }

    #[test]
    fn the_reading_travels_as_json() {
        let refused = Reading::Refused {
            reason: Refusal::Encrypted,
        };
        let json = serde_json::to_string(&refused).unwrap();
        assert_eq!(json, r#"{"outcome":"refused","reason":"encrypted"}"#);
        assert_eq!(serde_json::from_str::<Reading>(&json).unwrap(), refused);
        let read = Reading::Read(Extracted {
            pages: vec!["Texte".into()],
            paged: false,
        });
        let json = serde_json::to_string(&read).unwrap();
        assert_eq!(
            json,
            r#"{"outcome":"read","pages":["Texte"],"paged":false}"#
        );
        assert_eq!(serde_json::from_str::<Reading>(&json).unwrap(), read);
    }

    #[test]
    fn each_refusal_says_why_in_french() {
        assert_eq!(Refusal::TooLarge.to_string(), "Le fichier dépasse 10 Mo.");
        assert_eq!(
            Refusal::TooManyPages.to_string(),
            "Le document dépasse 500 pages."
        );
    }
}
