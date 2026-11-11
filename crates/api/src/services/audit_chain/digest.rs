//! An audit entry's seal (ADR-178): the HMAC-SHA256, under a key derived
//! from the server's master key, of the previous seal of its company's
//! chain, its place and its content. Each part is written with whether it
//! is there and, if so, its length, so that no two entries write the same
//! bytes: a value moved from one field to the next, or an empty one taken
//! for an absent one, changes the seal. The encoding is fixed: changing it
//! would break every chain sealed before (the test below pins it).
//!
//! Where a chain starts once its oldest places went (ADR-198) is sealed
//! too, apart: the company, its first place, the seal before it and how
//! many places went, under the same key, in another domain, so that the
//! database owner, without the key, can neither write a start nor move one.

use mimorya_db::repositories::audit_chain::{ChainEntry, ChainStart};
use ring::hmac;
use uuid::Uuid;

use crate::secrets::SecretCipher;

/// The purpose of the key derived for the audit chain.
const PURPOSE: &[u8] = b"audit-chain";
/// Opens what is sealed: a later encoding would be told apart.
const DOMAIN: &[u8] = b"mimorya.audit-chain.v1";
/// Opens a chain's start as sealed: never taken for an entry.
const START_DOMAIN: &[u8] = b"mimorya.audit-chain-start.v1";

/// The key that seals the audit log, derived from the master key.
#[derive(Clone)]
pub struct ChainKey(hmac::Key);

impl ChainKey {
    pub fn of(cipher: &SecretCipher) -> anyhow::Result<Self> {
        Ok(Self(cipher.derived_key(PURPOSE)?))
    }

    /// The seal of `entry` at the place `seq`, after the seal `previous`
    /// (none for the first place).
    pub fn seal(&self, previous: Option<&[u8]>, seq: i64, entry: &ChainEntry) -> [u8; 32] {
        let mut seal = [0; 32];
        seal.copy_from_slice(hmac::sign(&self.0, &encode(previous, seq, entry)).as_ref());
        seal
    }

    /// The seal of where the company `tenant_id`'s chain starts: its first
    /// place, the seal before it and how many places went (`start.seal` is
    /// not part of it).
    pub fn seal_start(&self, tenant_id: Uuid, start: &ChainStart) -> [u8; 32] {
        let (tenant, first, purged) = (
            tenant_id.to_string(),
            start.first_seq.to_be_bytes(),
            start.purged_entries.to_be_bytes(),
        );
        let mut bytes = START_DOMAIN.to_vec();
        write(
            &mut bytes,
            &[
                Some(tenant.as_bytes()),
                Some(&first),
                Some(&start.previous_hash),
                Some(&purged),
            ],
        );
        let mut seal = [0; 32];
        seal.copy_from_slice(hmac::sign(&self.0, &bytes).as_ref());
        seal
    }
}

/// An optional text as its bytes.
fn text(value: &Option<String>) -> Option<&[u8]> {
    value.as_deref().map(str::as_bytes)
}

/// What a seal signs, in a fixed order.
fn encode(previous: Option<&[u8]>, seq: i64, entry: &ChainEntry) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(256 + entry.metadata.len());
    bytes.extend_from_slice(DOMAIN);
    // Bound here: the parts below borrow them (Rust 1.88 drops temporaries
    // at the end of the statement).
    let (seq, id, occurred) = (
        seq.to_be_bytes(),
        entry.id.to_be_bytes(),
        entry.occurred_us.to_be_bytes(),
    );
    let parts: [Option<&[u8]>; 17] = [
        previous,
        Some(&seq),
        Some(&id),
        Some(entry.tenant_id.as_bytes()),
        Some(&occurred),
        Some(entry.actor_type.as_bytes()),
        text(&entry.actor_user_id),
        text(&entry.on_behalf_of_user_id),
        text(&entry.run_id),
        Some(entry.action.as_bytes()),
        text(&entry.resource_type),
        text(&entry.resource_id),
        Some(entry.outcome.as_bytes()),
        text(&entry.reason),
        Some(entry.metadata.as_bytes()),
        text(&entry.ip),
        text(&entry.request_id),
    ];
    write(&mut bytes, &parts);
    bytes
}

/// Writes each part with whether it is there and, if so, its length.
fn write(bytes: &mut Vec<u8>, parts: &[Option<&[u8]>]) {
    for part in parts {
        match part {
            None => bytes.push(0),
            Some(value) => {
                bytes.push(1);
                bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
                bytes.extend_from_slice(value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::PUBLIC_TEST_MASTER_KEYS;

    fn key(index: usize) -> ChainKey {
        ChainKey::of(&SecretCipher::from_base64(PUBLIC_TEST_MASTER_KEYS[index]).unwrap()).unwrap()
    }

    fn entry() -> ChainEntry {
        ChainEntry {
            id: 42,
            chain_seq: None,
            chain_hash: None,
            tenant_id: "00000000-0000-4000-8000-000000000001".into(),
            occurred_us: 1_793_000_000_000_000,
            actor_type: "user".into(),
            actor_user_id: Some("00000000-0000-4000-8000-000000000002".into()),
            on_behalf_of_user_id: None,
            run_id: None,
            action: "policy.updated".into(),
            resource_type: Some("policy".into()),
            resource_id: Some("p-1".into()),
            outcome: "success".into(),
            reason: None,
            metadata: r#"{"name": "Refus"}"#.into(),
            ip: Some("192.0.2.1/32".into()),
            request_id: Some("req-1".into()),
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn the_encoding_is_pinned() {
        // Sealed chains are verified with this encoding: a change here breaks
        // every one of them. Computed apart from this code (HKDF-SHA256 of
        // the master key, then HMAC-SHA256 of the parts as described above).
        let first = key(0).seal(None, 1, &entry());
        let second = key(0).seal(Some(&first), 2, &entry());
        assert_eq!(
            hex(&first),
            "0e211e833c1b977f3eb08feb430d5dc9181787390ad087774229440dbff46e15"
        );
        assert_eq!(
            hex(&second),
            "c4f5bcd4873c347aa92167890b4d84c19f8af684e5b69380a077b33b191430b5"
        );
    }

    fn start() -> ChainStart {
        ChainStart {
            first_seq: 10,
            previous_hash: vec![9; 32],
            purged_entries: 9,
            seal: Vec::new(),
        }
    }

    const COMPANY: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0001);

    #[test]
    fn a_start_s_encoding_is_pinned() {
        // Computed apart from this code, as the entry's above: the domain,
        // then the company as its text, the first place, the seal before
        // it and the count, each with its length.
        assert_eq!(
            hex(&key(0).seal_start(COMPANY, &start())),
            "a614c22996d008746a30b2718101d42e1a09a87e787128fe2e9e0376640a4fd0"
        );
    }

    #[test]
    fn every_part_of_a_start_changes_its_seal() {
        let chain = key(0);
        let sealed = chain.seal_start(COMPANY, &start());
        let other = Uuid::from_u128(2);
        assert_ne!(chain.seal_start(other, &start()), sealed);
        let changes: [fn(&mut ChainStart); 3] = [
            |s| s.first_seq += 1,
            |s| s.previous_hash[0] = 8,
            |s| s.purged_entries += 1,
        ];
        for change in changes {
            let mut moved = start();
            change(&mut moved);
            assert_ne!(chain.seal_start(COMPANY, &moved), sealed);
        }
        // Its own seal is not part of it; another key seals otherwise.
        let mut resealed = start();
        resealed.seal = sealed.to_vec();
        assert_eq!(chain.seal_start(COMPANY, &resealed), sealed);
        assert_ne!(key(1).seal_start(COMPANY, &start()), sealed);
    }

    #[test]
    fn the_same_entry_after_the_same_seal_is_sealed_alike() {
        let previous = [7; 32];
        assert_eq!(
            key(0).seal(Some(&previous), 3, &entry()),
            key(0).seal(Some(&previous), 3, &entry())
        );
    }

    #[test]
    fn another_key_seals_otherwise() {
        assert_ne!(
            key(0).seal(None, 1, &entry()),
            key(1).seal(None, 1, &entry())
        );
    }

    #[test]
    fn every_part_changes_the_seal() {
        let key = key(0);
        let sealed = key.seal(Some(&[7; 32]), 3, &entry());
        assert_ne!(key.seal(None, 3, &entry()), sealed);
        assert_ne!(key.seal(Some(&[8; 32]), 3, &entry()), sealed);
        assert_ne!(key.seal(Some(&[7; 32]), 4, &entry()), sealed);
        let changes: [fn(&mut ChainEntry); 16] = [
            |e| e.id += 1,
            |e| e.tenant_id = "00000000-0000-4000-8000-000000000003".into(),
            |e| e.occurred_us += 1,
            |e| e.actor_type = "agent".into(),
            |e| e.actor_user_id = None,
            |e| e.on_behalf_of_user_id = Some("00000000-0000-4000-8000-000000000002".into()),
            |e| e.run_id = Some("00000000-0000-4000-8000-000000000004".into()),
            |e| e.action = "policy.deleted".into(),
            |e| e.resource_type = None,
            |e| e.resource_id = Some("p-2".into()),
            |e| e.outcome = "denied".into(),
            |e| e.reason = Some("Raison".into()),
            |e| e.metadata = r#"{"name": "Accès"}"#.into(),
            |e| e.ip = None,
            |e| e.request_id = Some("req-2".into()),
            // The place and seal are not part of the content: changed, they
            // change nothing here; the verification compares them.
            |e| e.chain_seq = Some(9),
        ];
        let changed: Vec<bool> = changes
            .iter()
            .map(|change| {
                let mut altered = entry();
                change(&mut altered);
                key.seal(Some(&[7; 32]), 3, &altered) != sealed
            })
            .collect();
        assert_eq!(changed[..15], [true; 15]);
        assert!(!changed[15]);
    }

    #[test]
    fn neither_an_empty_value_nor_a_moved_one_passes_for_another() {
        let key = key(0);
        let mut empty = entry();
        empty.reason = Some(String::new());
        assert_ne!(key.seal(None, 1, &empty), key.seal(None, 1, &entry()));
        let (mut one, mut two) = (entry(), entry());
        one.resource_type = Some("polic".into());
        one.resource_id = Some("yp-1".into());
        two.resource_type = Some("policy".into());
        two.resource_id = Some("p-1".into());
        assert_ne!(key.seal(None, 1, &one), key.seal(None, 1, &two));
    }
}
