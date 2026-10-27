//! An OpenAPI description read for the operations it offers (ADR-114): the
//! administrators review them and keep those to declare; nothing is stored
//! nor called until they add or change the API.

use mimorya_connectors::rest::Imported;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use super::rest_connectors::RestOperation;

/// Body of `POST /admin/connectors/rest/import`: an OpenAPI 3 description
/// in JSON, as written.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenApiDescription {
    pub document: String,
}

/// The operations the description offers, each one a declaration accepts:
/// its paths in the order written, the methods of each in this order: GET,
/// PUT, POST, DELETE, PATCH.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RestImport {
    /// `null`: the description gives no complete address.
    pub url: Option<String>,
    pub operations: Vec<RestOperation>,
    /// What was left aside or changed, and why.
    pub notes: Vec<String>,
}

impl From<Imported> for RestImport {
    fn from(imported: Imported) -> Self {
        Self {
            url: imported.url,
            operations: imported.operations.iter().map(Into::into).collect(),
            notes: imported.notes,
        }
    }
}
