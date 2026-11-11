//! The audit log chained by company (ADR-178): each entry sealed after the
//! fact, in its company's chain, under a key derived from the master key
//! (`digest.rs`), by a sweep of the maintenance (`sweep.rs`, `seal.rs`)
//! that refuses to go on where the chain moved behind it (`heads.rs`); a
//! chain verified from its first place, and against anchors logged by the
//! sealing (`anchor.rs`, `verify.rs`), by `mimorya-admin verify-audit` or
//! by an administrator of the company (`on_demand.rs`, ADR-179), two at
//! once in a process, one for a company (`places.rs`, ADR-180). Its oldest
//! places, five years old, go with the retention pass, their seals checked
//! first, and where it then starts is sealed too (`purge.rs`, `start.rs`,
//! ADR-198). Without a master key, nothing is sealed, nor purged.

mod anchor;
mod digest;
mod heads;
mod on_demand;
mod places;
mod purge;
mod seal;
mod start;
mod sweep;
mod verify;

pub use anchor::Anchor;
pub use digest::ChainKey;
pub use heads::{ChainMoved, SealedHeads};
pub use on_demand::{MAX_ANCHORS, OwnChainCheck, verify_own_chain};
pub use places::{CHECKS_AT_ONCE, ChainChecks, CheckPlace};
pub use purge::{MAX_PURGE_BATCH, purge_old_places};
pub use seal::{AUDIT_CHAIN_SWEEP, seal_audit_log, seal_company};
pub use start::{Place, Start, chain_start};
pub use sweep::AuditSealing;
pub use verify::{AnchorFound, ChainBreak, ChainReport, LISTED_BREAKS, verify_company};

use mimorya_db::repositories::audit_chain::ChainHead;
use uuid::Uuid;

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

/// An anchor as the sealing logs it, and as `mimorya-admin verify-audit
/// --anchor` takes it: the company, the place and the seal there.
pub fn anchor_text(tenant_id: Uuid, head: &ChainHead) -> String {
    format!("{tenant_id}:{}:{}", head.chain_seq, hex(&head.chain_hash))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::secrets::PUBLIC_TEST_MASTER_KEYS;

    fn config(vars: &[(&str, &str)]) -> AppConfig {
        let mut map: HashMap<&str, &str> =
            HashMap::from([("DATABASE_APP_URL", "postgres://app:app@localhost/mimorya")]);
        map.extend(vars.iter().copied());
        AppConfig::from_vars(|key| map.get(key).map(|value| (*value).to_string())).unwrap()
    }

    #[test]
    fn only_a_server_with_a_master_key_seals() {
        assert!(server_key(&config(&[])).is_none());
        let keyed = config(&[("MIMORYA_MASTER_KEY", PUBLIC_TEST_MASTER_KEYS[0])]);
        assert!(server_key(&keyed).is_some());
    }
}
