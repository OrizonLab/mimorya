//! The company's secrets: sealed with the server's master key before they
//! reach the database, opened only where they are used, never returned by
//! the API nor written to logs or the audit (ADR-024).

mod cipher;
mod store;

pub use cipher::{MASTER_KEY_LEN, SecretCipher};
pub use store::{LLM_API_KEY, SecretStore};
