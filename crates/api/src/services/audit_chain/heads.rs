//! The seals the sealing remembers (ADR-178): for each company, the last
//! place and seal this process set, or found when it started. Kept outside
//! the database, they tell when the chain moved behind the sealing (seals
//! taken away, entries removed from its end): the sealing then refuses to
//! go on for that company, and says so, rather than seal again from what
//! the database now shows. A process remembers from its start only; the
//! seals it logs are the anchors that outlive it, checked by
//! `mimorya-admin verify-audit --anchor`. A place remembered that went
//! with the chain's oldest places, five years old (ADR-198), is no move:
//! the seal the purge kept stands for it at the place before the start.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{
    ChainHead, ChainStart, audit_chain_head, audit_chain_start, audit_log_tenants, audit_seal_at,
};
use sqlx::PgConnection;
use uuid::Uuid;

use super::anchor_text;

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

    /// Remembers, and logs, the last seal of every company not remembered
    /// yet, as the process starts: each logged is an anchor. A company that
    /// cannot be read is logged and left for the next call, which the
    /// failure asks for; one remembered already is never read again, so
    /// that a chain changed meanwhile is not taken for its own.
    pub async fn load(&self, db: &Database) -> anyhow::Result<()> {
        let mut unread = 0;
        for tenant_id in audit_log_tenants(db.pool(), false).await? {
            if self.get(tenant_id).is_some() {
                continue;
            }
            match last_seal(db, tenant_id).await {
                Ok(Some(head)) => {
                    let anchor = anchor_text(tenant_id, &head);
                    tracing::info!(tenant_id = %tenant_id, anchor = %anchor, "Audit chain found");
                    self.remember(tenant_id, head);
                }
                Ok(None) => {}
                Err(error) => {
                    unread += 1;
                    tracing::warn!(
                        tenant_id = %tenant_id,
                        error = format!("{error:#}"),
                        "Audit chain not read at startup"
                    );
                }
            }
        }
        if unread > 0 {
            anyhow::bail!("{unread} audit chain(s) not read");
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
        if held.as_deref() == Some(known.chain_hash.as_slice()) {
            return Ok(());
        }
        let start = match held {
            Some(_) => None,
            None => audit_chain_start(conn).await?,
        };
        if purged_as_known(&known, start.as_ref()) {
            return Ok(());
        }
        Err(ChainMoved {
            seq: known.chain_seq,
        }
        .into())
    }
}

/// Whether the place remembered, no longer in the chain, went with its
/// oldest places: at the place before its start, with the seal the purge
/// kept; before that, gone with them.
fn purged_as_known(known: &ChainHead, start: Option<&ChainStart>) -> bool {
    let Some(start) = start else {
        return false;
    };
    if known.chain_seq == start.first_seq - 1 {
        known.chain_hash == start.previous_hash
    } else {
        known.chain_seq < start.first_seq
    }
}

async fn last_seal(db: &Database, tenant_id: Uuid) -> anyhow::Result<Option<ChainHead>> {
    let mut tx = db.begin(tenant_id).await?;
    Ok(audit_chain_head(&mut tx).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_gone_with_the_oldest_ones_is_no_move() {
        let start = ChainStart {
            first_seq: 10,
            previous_hash: vec![9; 32],
            purged_entries: 9,
        };
        let known = |seq, byte| ChainHead {
            chain_seq: seq,
            chain_hash: vec![byte; 32],
        };
        assert!(purged_as_known(&known(4, 4), Some(&start)));
        assert!(purged_as_known(&known(9, 9), Some(&start)));
        // At the start's eve with another seal, after it, or no purge: moved.
        assert!(!purged_as_known(&known(9, 1), Some(&start)));
        assert!(!purged_as_known(&known(12, 1), Some(&start)));
        assert!(!purged_as_known(&known(4, 4), None));
    }
}
