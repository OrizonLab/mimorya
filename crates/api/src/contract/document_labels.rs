//! A document's type and confidentiality as the decision service classifies
//! them (ADR-213, D4): what a classification leaves for a person who may
//! write the document, and the labels that person sets or the proposal
//! they dismiss, each at the version they saw.

use mimorya_core::documents::DocumentType as CoreDocumentType;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use super::admin::Sensitivity;

/// A document's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    Contract,
    Invoice,
    Quote,
    Procedure,
    Minutes,
    Report,
    Letter,
    TechnicalSheet,
    Other,
}

impl From<DocumentType> for CoreDocumentType {
    fn from(kind: DocumentType) -> Self {
        match kind {
            DocumentType::Contract => Self::Contract,
            DocumentType::Invoice => Self::Invoice,
            DocumentType::Quote => Self::Quote,
            DocumentType::Procedure => Self::Procedure,
            DocumentType::Minutes => Self::Minutes,
            DocumentType::Report => Self::Report,
            DocumentType::Letter => Self::Letter,
            DocumentType::TechnicalSheet => Self::TechnicalSheet,
            DocumentType::Other => Self::Other,
        }
    }
}

/// What a classification leaves for a person who may write the document,
/// for its version `version`: a type and a looser level proposed, or the
/// level it held before the decision raised it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLabelProposal {
    pub version: i32,
    pub document_type: Option<DocumentType>,
    pub sensitivity: Option<Sensitivity>,
    pub raised_from: Option<Sensitivity>,
}

/// `PUT /documents/{id}/labels`: its type (none to clear it) and its level,
/// set by a person who may write it at its level and at the new one, at the
/// version they saw; what the classification proposed is dropped.
#[derive(Debug, Clone, Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentLabelsUpdate {
    #[validate(range(min = 1))]
    pub version: i32,
    pub document_type: Option<DocumentType>,
    pub sensitivity: Sensitivity,
}

/// `POST /documents/{id}/proposal/dismiss`: what the classification of the
/// version `version` proposed, dropped, the document left as it is.
#[derive(Debug, Clone, Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentProposalDismissal {
    #[validate(range(min = 1))]
    pub version: i32,
}
