//! JSON columns written by the platform (run usage, verification, message
//! metadata), read back as their contract types.

use anyhow::anyhow;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// A mismatch is an internal error. Its message names the column only:
/// the stored value may hold message content, which never reaches the logs.
pub fn stored<T: DeserializeOwned>(column: &str, value: &Value) -> anyhow::Result<T> {
    T::deserialize(value).map_err(|_| anyhow!("stored {column} does not match the API contract"))
}
