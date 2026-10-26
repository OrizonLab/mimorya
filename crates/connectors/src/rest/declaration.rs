//! A REST API as a company's administrators describe it (ADR-112): its
//! address, the header its token goes in, and its operations, each a method,
//! a path and typed parameters. Stored as the instance's configuration;
//! checked by `checks`, the same way when it is saved and at each start.

use serde::{Deserialize, Serialize};

/// Nothing secret: the token is in the secret store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestConfig {
    /// Its tools are named `rest_<prefix>_<operation>`.
    pub prefix: String,
    /// What every operation's path follows: `https://…`, without
    /// credentials, parameters nor fragment.
    pub url: String,
    /// The header the token is sent in; absent: `Authorization: Bearer …`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_header: Option<String>,
    pub operations: Vec<RestOperation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestOperation {
    /// Names its tool: `rest_<prefix>_<name>`.
    pub name: String,
    pub title: String,
    /// What the model reads of the tool, after the API's prefix.
    pub description: String,
    pub method: HttpMethod,
    /// Appended to the API's address; `{name}` stands for a path parameter.
    pub path: String,
    #[serde(default)]
    pub parameters: Vec<RestParameter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestParameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: ParameterLocation,
    #[serde(rename = "type")]
    pub kind: ParameterType,
    /// A path parameter always is.
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    /// Whether a request with this method carries a body.
    pub fn has_body(self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterLocation {
    Path,
    Query,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterType {
    String,
    Integer,
    Number,
    Boolean,
}

impl ParameterType {
    /// Its name in a JSON Schema.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::Boolean => "boolean",
        }
    }
}

/// What every tool of an API's prefix begins with.
pub fn tool_namespace(prefix: &str) -> String {
    format!("rest_{prefix}_")
}

/// The tool an operation becomes.
pub fn rest_tool_name(prefix: &str, operation: &str) -> String {
    format!("{}{operation}", tool_namespace(prefix))
}

/// What the model reads of an operation: the API it belongs to, then its
/// description as the administrators wrote it.
pub fn rest_tool_description(prefix: &str, description: &str) -> String {
    format!("[API REST « {prefix} »] {description}")
}
