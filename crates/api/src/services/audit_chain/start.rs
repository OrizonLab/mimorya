//! Where a company's chain starts once its oldest places went, five years
//! old (ADR-198), taken for the chain's only when its seal holds under the
//! chain's key: the verification, the sealing, the purge and the export
//! read it here. A start that does not hold its seal, written or moved by
//! whoever has the database but not the key, is taken for none, and
//! logged: the places before it are then missing, as they would be
//! without it. One rule says what the chain holds before its start: at the
//! place before it, the seal kept there; before that, nothing any more.

use mimorya_db::repositories::audit_chain::{ChainStart, audit_chain_start};
use sqlx::PgConnection;
use uuid::Uuid;

use super::ChainKey;

/// Where a company's chain starts, its seal checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    /// Its first place.
    pub first_seq: i64,
    /// The seal of the place before it, the last one deleted.
    pub previous_hash: Vec<u8>,
    /// How many places went in all.
    pub purged_entries: i64,
}

/// What the chain holds at a place, given where it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The place before the start: the seal kept there, as given or not.
    Eve { held: bool },
    /// Before that: gone with the oldest places.
    Purged,
    /// The start or after: in the chain still, or missing from it.
    InChain,
}

impl Start {
    /// What the chain holds at the place `seq`; at its eve, whether the
    /// seal kept there is the one `given` says it is.
    pub fn place(&self, seq: i64, given: impl FnOnce(&[u8]) -> bool) -> Place {
        if seq == self.first_seq - 1 {
            Place::Eve {
                held: given(&self.previous_hash),
            }
        } else if seq < self.first_seq {
            Place::Purged
        } else {
            Place::InChain
        }
    }
}

/// Where the company's chain starts, read in `conn`: none when its oldest
/// places never went, or when the start the database holds does not hold
/// its seal, which is logged.
pub async fn chain_start(
    conn: &mut PgConnection,
    key: &ChainKey,
    tenant_id: Uuid,
) -> anyhow::Result<Option<Start>> {
    let Some(found) = audit_chain_start(conn).await? else {
        return Ok(None);
    };
    let first_seq = found.first_seq;
    let start = checked(key, tenant_id, found);
    if start.is_none() {
        tracing::error!(
            tenant_id = %tenant_id,
            first_seq,
            "Audit chain start whose seal does not hold: taken for none"
        );
    }
    Ok(start)
}

/// `found`, if it holds its seal under `key`.
fn checked(key: &ChainKey, tenant_id: Uuid, found: ChainStart) -> Option<Start> {
    let sealed = key.seal_start(tenant_id, &found);
    (found.seal == sealed).then_some(Start {
        first_seq: found.first_seq,
        previous_hash: found.previous_hash,
        purged_entries: found.purged_entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{PUBLIC_TEST_MASTER_KEYS, SecretCipher};

    fn key(index: usize) -> ChainKey {
        ChainKey::of(&SecretCipher::from_base64(PUBLIC_TEST_MASTER_KEYS[index]).unwrap()).unwrap()
    }

    fn found(first_seq: i64) -> ChainStart {
        ChainStart {
            first_seq,
            previous_hash: vec![9; 32],
            purged_entries: first_seq - 1,
            seal: Vec::new(),
        }
    }

    #[test]
    fn a_start_is_the_chain_s_only_under_its_seal() {
        let company = Uuid::from_u128(1);
        let mut sealed = found(10);
        sealed.seal = key(0).seal_start(company, &sealed).to_vec();
        let start = checked(&key(0), company, sealed.clone()).unwrap();
        assert_eq!((start.first_seq, start.purged_entries), (10, 9));
        // Moved, written for another company, under another key, or with
        // a seal of the database owner's: none.
        let mut moved = sealed.clone();
        moved.first_seq = 20;
        assert_eq!(checked(&key(0), company, moved), None);
        assert_eq!(checked(&key(0), Uuid::from_u128(2), sealed.clone()), None);
        assert_eq!(checked(&key(1), company, sealed.clone()), None);
        let mut forged = sealed;
        forged.seal = vec![0; 32];
        assert_eq!(checked(&key(0), company, forged), None);
    }

    #[test]
    fn before_its_start_the_chain_holds_the_seal_kept_at_its_eve_alone() {
        let start = Start {
            first_seq: 10,
            previous_hash: vec![9; 32],
            purged_entries: 9,
        };
        let is = |byte: u8| move |kept: &[u8]| kept == [byte; 32];
        assert_eq!(start.place(9, is(9)), Place::Eve { held: true });
        assert_eq!(start.place(9, is(1)), Place::Eve { held: false });
        assert_eq!(start.place(3, is(3)), Place::Purged);
        assert_eq!(start.place(10, is(9)), Place::InChain);
        assert_eq!(start.place(12, is(9)), Place::InChain);
    }
}
