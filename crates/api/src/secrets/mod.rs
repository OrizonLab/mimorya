//! The company's secrets: sealed with the server's master key before they
//! reach the database, opened only where they are used, never returned by
//! the API nor written to logs or the audit (ADR-024).

mod cipher;
mod store;

pub use cipher::{KEY_VERSION, MASTER_KEY_LEN, SecretCipher};
pub use store::{LLM_API_KEY, SecretRead, SecretStore};

/// Master keys published by this repository's tests (integration and end to
/// end): anyone can read them, so production refuses them.
pub const PUBLIC_TEST_MASTER_KEYS: [&str; 2] = [
    "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
    "Hx4dHBsaGRgXFhUUExIREA8ODQwLCgkIBwYFBAMCAQA=",
];
