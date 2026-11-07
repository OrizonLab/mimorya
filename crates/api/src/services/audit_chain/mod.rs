//! The audit log chained by company (ADR-178): each entry sealed after the
//! fact, in its company's chain, under a key derived from the master key
//! (`digest.rs`), by a sweep of the maintenance (`seal.rs`); a chain
//! verified from its first place (`verify.rs`), by `mimorya-admin
//! verify-audit`. Without a master key, nothing is sealed.

mod digest;
mod seal;
mod verify;

pub use digest::ChainKey;
pub use seal::{AUDIT_CHAIN_SWEEP, seal_audit_log, seal_company};
pub use verify::{ChainBreak, ChainReport, verify_company};

use crate::config::{AppConfig, Environment};

/// The key that seals this server's audit log: none without a master key,
/// which production is warned of.
pub fn server_key(config: &AppConfig) -> Option<ChainKey> {
    let Some(cipher) = config.secrets.as_deref() else {
        if config.env == Environment::Production {
            tracing::warn!("No MIMORYA_MASTER_KEY: the audit log is not sealed (ADR-178)");
        }
        return None;
    };
    match ChainKey::of(cipher) {
        Ok(key) => Some(key),
        Err(error) => {
            tracing::warn!(error = %error, "The audit log's key is not derived: it is not sealed");
            None
        }
    }
}

/// A seal as written in the logs and to the operator.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
