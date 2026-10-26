//! REST APIs as the administration writes and reads them (`connector:manage`,
//! ADR-112): an address, the header the token goes in, and the operations,
//! each a method, a path and typed parameters. The platform checks them as
//! a whole (`mimorya_connectors::rest`); the token is never returned.

use mimorya_connectors::rest;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use crate::http::{trimmed, trimmed_opt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// Where a parameter goes: the path (`{name}`), the query, the JSON body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ParameterLocation {
    Path,
    Query,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ParameterType {
    String,
    Integer,
    Number,
    Boolean,
}

/// What the model may give an operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestParameter {
    #[serde(deserialize_with = "trimmed")]
    pub name: String,
    #[serde(rename = "in")]
    pub location: ParameterLocation,
    #[serde(rename = "type")]
    pub kind: ParameterType,
    /// A path parameter always is.
    #[serde(default)]
    #[ts(optional)]
    pub required: Option<bool>,
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub description: Option<String>,
}

/// One operation of the API, which becomes the tool `rest_<prefix>_<name>`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestOperation {
    #[serde(deserialize_with = "trimmed")]
    pub name: String,
    #[serde(deserialize_with = "trimmed")]
    pub title: String,
    /// What the model reads of the tool, after the API's prefix.
    #[serde(deserialize_with = "trimmed")]
    pub description: String,
    pub method: HttpMethod,
    /// Follows the API's address; `{name}` stands for a path parameter.
    #[serde(deserialize_with = "trimmed")]
    pub path: String,
    #[serde(default)]
    pub parameters: Vec<RestParameter>,
}

/// A REST API as stored, and whether a token is.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RestApi {
    /// Its tools are named `rest_<prefix>_<operation>`.
    pub prefix: String,
    pub url: String,
    /// `null`: the token goes as `Authorization: Bearer`.
    pub token_header: Option<String>,
    pub token_stored: bool,
    pub operations: Vec<RestOperation>,
}

/// Body of `POST /admin/connectors/rest`: a REST API and its operations,
/// checked as a whole by the platform (ADR-112).
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRestConnector {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 2, max = 80))]
    pub name: String,
    #[serde(deserialize_with = "trimmed")]
    pub prefix: String,
    #[serde(deserialize_with = "trimmed")]
    pub url: String,
    /// Absent or blank: `Authorization: Bearer`.
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub token_header: Option<String>,
    /// Absent or blank: none.
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub token: Option<String>,
    /// Absent: enabled.
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
    pub operations: Vec<RestOperation>,
}

impl From<RestOperation> for rest::RestOperation {
    fn from(operation: RestOperation) -> Self {
        Self {
            name: operation.name,
            title: operation.title,
            // As a form's text area may send it.
            description: operation.description.replace("\r\n", "\n"),
            method: match operation.method {
                HttpMethod::Get => rest::HttpMethod::Get,
                HttpMethod::Post => rest::HttpMethod::Post,
                HttpMethod::Put => rest::HttpMethod::Put,
                HttpMethod::Patch => rest::HttpMethod::Patch,
                HttpMethod::Delete => rest::HttpMethod::Delete,
            },
            path: operation.path,
            parameters: operation
                .parameters
                .into_iter()
                .map(|p| rest::RestParameter {
                    name: p.name,
                    location: match p.location {
                        ParameterLocation::Path => rest::ParameterLocation::Path,
                        ParameterLocation::Query => rest::ParameterLocation::Query,
                        ParameterLocation::Body => rest::ParameterLocation::Body,
                    },
                    kind: match p.kind {
                        ParameterType::String => rest::ParameterType::String,
                        ParameterType::Integer => rest::ParameterType::Integer,
                        ParameterType::Number => rest::ParameterType::Number,
                        ParameterType::Boolean => rest::ParameterType::Boolean,
                    },
                    // A path parameter always is, said or not.
                    required: p.required.unwrap_or(false) || p.location == ParameterLocation::Path,
                    description: p.description.unwrap_or_default(),
                })
                .collect(),
        }
    }
}

impl From<&rest::RestOperation> for RestOperation {
    fn from(operation: &rest::RestOperation) -> Self {
        Self {
            name: operation.name.clone(),
            title: operation.title.clone(),
            description: operation.description.clone(),
            method: match operation.method {
                rest::HttpMethod::Get => HttpMethod::Get,
                rest::HttpMethod::Post => HttpMethod::Post,
                rest::HttpMethod::Put => HttpMethod::Put,
                rest::HttpMethod::Patch => HttpMethod::Patch,
                rest::HttpMethod::Delete => HttpMethod::Delete,
            },
            path: operation.path.clone(),
            parameters: operation
                .parameters
                .iter()
                .map(|p| RestParameter {
                    name: p.name.clone(),
                    location: match p.location {
                        rest::ParameterLocation::Path => ParameterLocation::Path,
                        rest::ParameterLocation::Query => ParameterLocation::Query,
                        rest::ParameterLocation::Body => ParameterLocation::Body,
                    },
                    kind: match p.kind {
                        rest::ParameterType::String => ParameterType::String,
                        rest::ParameterType::Integer => ParameterType::Integer,
                        rest::ParameterType::Number => ParameterType::Number,
                        rest::ParameterType::Boolean => ParameterType::Boolean,
                    },
                    required: Some(p.required),
                    description: (!p.description.is_empty()).then(|| p.description.clone()),
                })
                .collect(),
        }
    }
}
