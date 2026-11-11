//! The sealing as the maintenance runs it each minute (ADR-178): this
//! server's key and the seals it remembers, each company's chain read at
//! the first pass, and at each one after until every chain was read, then
//! the entries still to seal sealed. Apart from the maintenance's loop, so
//! that a test runs its passes as the maintenance does.

use mimorya_db::Database;

use super::{ChainKey, SealedHeads, seal_audit_log, server_key};
use crate::config::AppConfig;

/// The sealing of one process, from its start.
pub struct AuditSealing {
    key: ChainKey,
    heads: SealedHeads,
    read: bool,
}

impl AuditSealing {
    /// The sealing of a server with a master key; none without, which
    /// production is warned of.
    pub fn of(config: &AppConfig) -> Option<Self> {
        server_key(config).map(Self::new)
    }

    pub fn new(key: ChainKey) -> Self {
        Self {
            key,
            heads: SealedHeads::default(),
            read: false,
        }
    }

    /// The key it seals with, which the retention pass purges with too
    /// (ADR-198).
    pub fn key(&self) -> ChainKey {
        self.key.clone()
    }

    /// Whether every company's chain was read since the process started.
    pub fn has_read_the_chains(&self) -> bool {
        self.read
    }

    /// One pass: the chains read, if they were not all, then the entries
    /// still to seal sealed; how many. What fails is logged, for the next
    /// pass to try again.
    pub async fn pass(&mut self, db: &Database) -> usize {
        if !self.read {
            match self.heads.load(db).await {
                Ok(()) => self.read = true,
                Err(error) => tracing::warn!(error = %error, "Audit chains not all read"),
            }
        }
        match seal_audit_log(db, &self.key, &self.heads).await {
            Ok(sealed) => sealed,
            Err(error) => {
                tracing::warn!(error = %error, "Audit log sealing failed");
                0
            }
        }
    }
}
