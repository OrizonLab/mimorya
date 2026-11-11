//! A SIEM reading its company's audit log (ADR-181) once the chain's
//! oldest places went (ADR-198): after the place before the chain's start,
//! with the seal kept there, it reads on from the start; with another seal,
//! the chain is said to hold another; before it, the place is said gone
//! with the oldest ones, and where the chain starts given, from where the
//! SIEM resumes. A start no key sealed is none: a place before it is gone,
//! as from a chain restored behind the SIEM.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::audit_chain::{SealedHeads, hex};
use mimorya_api::services::audit_export::create_audit_export_token;
use support::audit_chain::seal;
use support::audit_exports::read;
use support::llm::MASTER_KEY;
use support::old_audit::{entries, owner_runs, place, purged_with, unsealed_gone};
use support::slack_companies::other_administrator;
use support::{TestApi, TestResponse};

/// The places a read gave.
fn places_of(response: &TestResponse) -> Vec<i64> {
    assert_eq!(response.status, StatusCode::OK, "{}", response.text());
    let body = response.json();
    let entries = body["entries"].as_array().unwrap();
    entries
        .iter()
        .map(|entry| entry["place"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_siem_reads_on_past_a_purge_from_the_chain_s_start() {
    let api =
        TestApi::start_with(&[("MIMORYA_ROLE", "api"), ("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let db = &api.app.services.db;
    // A company of its own, its administrator's invitation accepted.
    let (acme, access) = other_administrator(&api, "acme-export-purge").await;
    let (_, secret) = create_audit_export_token(db, &access, "SIEM")
        .await
        .unwrap();
    // The company's making and the token's leave the chain to the test.
    unsealed_gone(acme).await;
    let heads = SealedHeads::default();
    entries(db, acme, "ancienne", 4, 6).await;
    entries(db, acme, "recente", 1, 0).await;
    seal(db, &heads, acme).await.unwrap();
    let (first, eve) = (place(db, acme, 1).await, place(db, acme, 2).await);
    assert_eq!(purged_with(db, acme, 2).await, Some((2, 2)));
    seal(db, &heads, acme).await.unwrap();

    // After the place before the start, with the seal kept there: on from
    // the start.
    let after_eve = format!("?after=2&seal={}", hex(&eve.chain_hash));
    let read_on = read(&api, Some(&secret), &after_eve).await;
    assert_eq!(places_of(&read_on), [3, 4, 5, 6]);
    // With another seal there: the chain holds another.
    let other = format!("?after=2&seal={}", "00".repeat(32));
    let changed = read(&api, Some(&secret), &other).await;
    assert_eq!(changed.status, StatusCode::CONFLICT, "{}", changed.text());
    assert!(
        changed
            .text()
            .contains("ne porte plus ce sceau à la place 2")
    );

    // Before it: gone with the oldest places, the start given.
    let before = format!("?after=1&seal={}", hex(&first.chain_hash));
    let gone = read(&api, Some(&secret), &before).await;
    assert_eq!(gone.status, StatusCode::CONFLICT, "{}", gone.text());
    let said = gone.text();
    assert!(said.contains("ne garde plus la place 1"), "{said}");
    assert!(said.contains("commence aujourd’hui à la place 3"), "{said}");
    let details = &gone.json()["error"]["details"];
    assert_eq!(
        (
            details["firstPlace"].as_i64(),
            details["lastPlace"].as_i64()
        ),
        (Some(3), Some(6))
    );
    // From the start, the SIEM reads the chain's first place now.
    assert_eq!(
        places_of(&read(&api, Some(&secret), "").await),
        [3, 4, 5, 6]
    );

    // A start no key sealed: none, the place before it gone as from a
    // chain restored behind the SIEM.
    owner_runs(&format!(
        "delete from audit_chain_starts where tenant_id = '{acme}'"
    ))
    .await
    .unwrap();
    owner_runs(&format!(
        "insert into audit_chain_starts (tenant_id, first_seq, previous_hash, purged_entries, \
         seal) values ('{acme}', 3, '\\x{}', 2, '\\x{}')",
        hex(&eve.chain_hash),
        "00".repeat(32)
    ))
    .await
    .unwrap();
    let forged = read(&api, Some(&secret), &after_eve).await;
    assert_eq!(forged.status, StatusCode::CONFLICT, "{}", forged.text());
    let said = forged.text();
    assert!(said.contains("ne compte plus la place 2"), "{said}");
    assert!(said.contains("restauré en deçà"), "{said}");
}
