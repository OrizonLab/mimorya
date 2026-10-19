//! Columns written by the platform (run usage, verification, message
//! metadata, memory scopes…), read back as their contract types.

use anyhow::anyhow;
use serde::de::value::{Error as ValueError, StrDeserializer};
use serde::de::{DeserializeOwned, IntoDeserializer};
use serde_json::Value;

/// A JSON column. A mismatch is an internal error whose message names the
/// column only: the stored value may hold message content, which never
/// reaches the logs.
pub fn stored<T: DeserializeOwned>(column: &str, value: &Value) -> anyhow::Result<T> {
    T::deserialize(value).map_err(|_| off_contract(column))
}

/// A text column holding one of the contract's values (`snake_case`).
pub fn stored_text<T: DeserializeOwned>(column: &str, value: &str) -> anyhow::Result<T> {
    let text: StrDeserializer<'_, ValueError> = value.into_deserializer();
    T::deserialize(text).map_err(|_| off_contract(column))
}

fn off_contract(column: &str) -> anyhow::Error {
    anyhow!("stored {column} does not match the API contract")
}
