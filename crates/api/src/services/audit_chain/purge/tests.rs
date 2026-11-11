//! Which of a chain's oldest places may go (ADR-198): a prefix of it from
//! its start, each place five years old, following the one before, held
//! once, holding its seal, before its last sealed place.

use mimorya_db::repositories::audit_chain::ChainEntry;

use super::*;
use crate::secrets::{PUBLIC_TEST_MASTER_KEYS, SecretCipher};

const COMPANY: &str = "00000000-0000-4000-8000-000000000001";
/// Six years before the horizon, and one second after it.
const OLD: i64 = 1_000_000_000_000;
const HORIZON: i64 = OLD + 6 * 365 * 86_400 * 1_000_000;
const RECENT: i64 = HORIZON + 1_000_000;

fn key() -> ChainKey {
    ChainKey::of(&SecretCipher::from_base64(PUBLIC_TEST_MASTER_KEYS[0]).unwrap()).unwrap()
}

fn entry(id: i64, occurred_us: i64) -> ChainEntry {
    ChainEntry {
        id,
        chain_seq: None,
        chain_hash: None,
        tenant_id: COMPANY.into(),
        occurred_us,
        actor_type: "user".into(),
        actor_user_id: None,
        on_behalf_of_user_id: None,
        run_id: None,
        action: "ancienne.action".into(),
        resource_type: None,
        resource_id: None,
        outcome: "success".into(),
        reason: None,
        metadata: "{}".into(),
        ip: None,
        request_id: None,
    }
}

/// `times` sealed one after the other from the place `first`, after the
/// seal `previous`.
fn chain(first: i64, previous: Option<Vec<u8>>, times: &[i64]) -> Vec<ChainEntry> {
    let key = key();
    let mut previous = previous;
    times
        .iter()
        .enumerate()
        .map(|(index, &time)| {
            let seq = first + index as i64;
            let mut sealed = entry(seq * 10, time);
            let hash = key.seal(previous.as_deref(), seq, &sealed).to_vec();
            sealed.chain_seq = Some(seq);
            sealed.chain_hash = Some(hash.clone());
            previous = Some(hash);
            sealed
        })
        .collect()
}

fn bounds(head: i64, most: i64) -> Bounds {
    Bounds {
        head,
        horizon_us: HORIZON,
        most,
    }
}

/// Through which place, and how many, go.
fn gone(start: Option<&Start>, entries: &[ChainEntry], bounds: Bounds) -> Option<(i64, i64)> {
    old_prefix(&key(), start, entries, bounds).map(|prefix| (prefix.through, prefix.count))
}

#[test]
fn old_places_go_never_the_last_sealed_one() {
    let entries = chain(1, None, &[OLD; 5]);
    assert_eq!(gone(None, &entries, bounds(5, 5000)), Some((4, 4)));
    let prefix = old_prefix(&key(), None, &entries, bounds(5, 5000)).unwrap();
    assert_eq!(Some(prefix.seal), entries[3].chain_hash);
    // Within what may go.
    assert_eq!(gone(None, &entries, bounds(5, 2)), Some((2, 2)));
    // A chain of one sealed place keeps it.
    assert_eq!(gone(None, &entries[..1], bounds(1, 5000)), None);
}

#[test]
fn a_recent_place_a_gap_a_place_held_twice_or_an_altered_one_stops_it() {
    let entries = chain(1, None, &[OLD, OLD, RECENT, OLD, OLD]);
    assert_eq!(gone(None, &entries, bounds(5, 5000)), Some((2, 2)));

    let mut gap = chain(1, None, &[OLD; 5]);
    gap.remove(2);
    assert_eq!(gone(None, &gap, bounds(5, 5000)), Some((2, 2)));

    let mut twice = chain(1, None, &[OLD; 5]);
    let mut copy = twice[1].clone();
    copy.id += 1;
    twice.insert(2, copy);
    assert_eq!(gone(None, &twice, bounds(5, 5000)), Some((1, 1)));

    // Its content or its time changed: its seal no longer holds.
    let mut altered = chain(1, None, &[OLD; 5]);
    altered[2].metadata = r#"{"changed": true}"#.into();
    assert_eq!(gone(None, &altered, bounds(5, 5000)), Some((2, 2)));
    let mut dated = chain(1, None, &[OLD, OLD, RECENT, OLD, OLD]);
    dated[2].occurred_us = OLD;
    assert_eq!(gone(None, &dated, bounds(5, 5000)), Some((2, 2)));

    // The first place not where the chain starts: none.
    assert_eq!(gone(None, &entries[1..], bounds(5, 5000)), None);
}

#[test]
fn from_a_start_the_places_follow_its_seal() {
    let before = chain(1, None, &[OLD; 3]);
    let kept = before[2].chain_hash.clone().unwrap();
    let entries = chain(4, Some(kept.clone()), &[OLD; 4]);
    let start = Start {
        first_seq: 4,
        previous_hash: kept,
        purged_entries: 3,
    };
    assert_eq!(gone(Some(&start), &entries, bounds(7, 5000)), Some((6, 3)));
    // Another seal kept before it: the first place does not hold.
    let moved = Start {
        previous_hash: vec![0; 32],
        ..start.clone()
    };
    assert_eq!(gone(Some(&moved), &entries, bounds(7, 5000)), None);
    // Read from a start where it is not: none.
    assert_eq!(gone(None, &entries, bounds(7, 5000)), None);
}
