//! The seals the sealing remembers (ADR-178): for each company, the last
//! place and seal this process set, or found when it started. Kept outside
//! the database, they tell when the chain moved behind the sealing (seals
//! taken away, entries removed from its end): the sealing then refuses to
//! go on for that company, and says so, rather than seal again from what
//! the database now shows. A process remembers from its start only; the
//! seals it logs are the anchors that outlive it, checked by
//! `mimorya-admin verify-audit --anchor`.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{
    ChainHead, audit_chain_head, audit_log_tenants, audit_seal_at,
};
use sqlx::PgConnection;
use uuid::Uuid;

use super::hex;

/// The chain of a company no longer holds, at the place `seq`, the seal the
/// sealing set or found there.
#[derive(Debug)]
pub struct ChainMoved {
    pub seq: i64,
}

impl std::fmt::Display for ChainMoved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the audit chain no longer holds at place {} the seal the sealing set or found there",
            self.seq
        )
    }
}

impl std::error::Error for ChainMoved {}

/// The last seal of each company, as this process knows it.
#[derive(Default)]
pub struct SealedHeads(Mutex<HashMap<Uuid, ChainHead>>);

impl SealedHeads {
    fn get(&self, tenant_id: Uuid) -> Option<ChainHead> {
        let heads = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        heads.get(&tenant_id).cloned()
    }

    pub(super) fn remember(&self, tenant_id: Uuid, head: ChainHead) {
        let mut heads = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        heads.insert(tenant_id, head);
    }

    /// Remembers, and logs, the last seal of every company, as the process
    /// starts: each logged is an anchor. A company that cannot be read is
    /// left out, and logged.
    pub async fn load(&self, db: &Database) -> anyhow::Result<()> {
        for tenant_id in audit_log_tenants(db.pool(), false).await? {
            let mut tx = db.begin(tenant_id).await?;
            match audit_chain_head(&mut tx).await {
                Ok(Some(head)) => {
                    let (seq, seal) = (head.chain_seq, hex(&head.chain_hash));
                    tracing::info!(tenant_id = %tenant_id, seq, head = %seal, "Audit chain found");
                    self.remember(tenant_id, head);
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(
                    tenant_id = %tenant_id,
                    error = %error,
                    "Audit chain not read at startup"
                ),
            }
        }
        Ok(())
    }

    /// Fails with `ChainMoved` if the company's chain, read under its lock,
    /// no longer holds the seal remembered at its place.
    pub(super) async fn check(
        &self,
        conn: &mut PgConnection,
        tenant_id: Uuid,
    ) -> anyhow::Result<()> {
        let Some(known) = self.get(tenant_id) else {
            return Ok(());
        };
        let held = audit_seal_at(conn, known.chain_seq).await?;
        if held.as_deref() != Some(known.chain_hash.as_slice()) {
            return Err(ChainMoved {
                seq: known.chain_seq,
            }
            .into());
        }
        Ok(())
    }
}
