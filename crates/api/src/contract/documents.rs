//! The company's documents, as listed to a reader, and what a deposit
//! sends and gets back (ADR-095). The file itself is the request's body;
//! what describes it travels in the query string, strict as a body.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use super::admin::Sensitivity;

/// The kind of a deposited file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Pdf,
    Docx,
    Markdown,
    Text,
}

/// Whose space a document is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DocumentScope {
    /// The whole company's.
    Tenant,
    /// A unit's of the organisation.
    OrgUnit,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentItem {
    pub id: Uuid,
    pub title: String,
    pub file_name: String,
    pub kind: DocumentKind,
    pub scope: DocumentScope,
    pub org_unit_id: Option<Uuid>,
    /// Name of the unit whose space it is in.
    pub org_unit: Option<String>,
    pub sensitivity: Sensitivity,
    pub byte_size: i32,
    /// Its pages, for a PDF.
    pub page_count: Option<i32>,
    pub created_at: String,
    /// The reader may delete it.
    pub can_delete: bool,
}

/// A space the reader may deposit into: the company's (no unit) or a unit's.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSpaceOption {
    pub unit_id: Option<Uuid>,
    pub name: String,
}

/// The company's documents held, against its quota: shown to those who
/// deposit.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentQuota {
    pub used_bytes: i64,
    pub quota_bytes: i64,
}

/// `GET /documents`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentList {
    /// The latest documents read, a bounded number of them.
    pub documents: Vec<DocumentItem>,
    /// Older documents are read too, beyond those listed.
    pub truncated: bool,
    /// Where the reader may deposit.
    pub spaces: Vec<DocumentSpaceOption>,
    pub quota: Option<DocumentQuota>,
    /// The largest file a deposit takes, in bytes (ADR-095).
    pub max_file_bytes: i64,
}

/// `POST /documents?name=…&unitId=…&sensitivity=…`, the file as body: its
/// name, the unit whose space receives it (the company's without one), its
/// sensitivity (`internal` without one).
#[derive(Debug, Clone, Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentUpload {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    #[serde(default)]
    #[ts(optional)]
    pub unit_id: Option<Uuid>,
    #[serde(default)]
    #[ts(optional)]
    pub sensitivity: Option<Sensitivity>,
}

/// A document deposited: its identifier and the passages its text gave.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentCreated {
    pub id: Uuid,
    pub page_count: Option<i32>,
    pub passages: i64,
}
