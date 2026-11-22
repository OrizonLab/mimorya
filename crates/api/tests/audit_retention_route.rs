//! Where its company's audit chain begins, as the verification an
//! administrator asks for answers it (ADR-198, ADR-219): after the places
//! retention deleted, under the start it sealed; once that start is moved
//! without the key, at the first place, the places before it missing.

mod support;

use axum::http::StatusCode;
use mimorya_api::plugins::retention::apply_retention;
use mimorya_api::services::audit_chain::{SealedHeads, hex};
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept};
use support::audit_chain::{key, seal};
use support::old_audit::{entries, owner_runs, place, unsealed_gone};
use support::{Client, TestApi, companies, llm};

const VERIFY: &str = "/admin/audit/verifications";

async fn verified(client: &Client<'_>) -> Value {
    let verified = client.post(VERIFY, json!({ "anchors": [] })).await;
    assert_eq!(verified.status, StatusCode::OK, "{}", verified.text());
    verified.json()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_verification_answers_where_the_chain_begins() {
    // The server's key is the one the tests seal and purge with.
    let api = TestApi::start_with(&[
        ("MIMORYA_ROLE", "api"),
        ("MIMORYA_MASTER_KEY", llm::MASTER_KEY),
    ])
    .await;
    let db = &api.app.services.db;
    let address = "jeanne@acme-debut-lu.example";
    let acme = companies::create(&api, "acme-debut-lu", address).await;
    let accepted = accept(&api, &acme.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    let tenant_id = acme.tenant_id;
    unsealed_gone(tenant_id).await;

    // Nothing deleted yet: the chain begins at its first place.
    entries(db, tenant_id, "ancienne", 3, 6).await;
    entries(db, tenant_id, "recente", 2, 0).await;
    let heads = SealedHeads::default();
    seal(db, &heads, tenant_id).await.unwrap();
    let report = verified(&jeanne).await;
    assert_eq!(
        (&report["firstPlace"], &report["purged"]),
        (&json!(1), &json!(0))
    );
    assert_eq!(report["holds"], true, "{report}");

    // The three entries six years old gone: it begins after them.
    let retained = apply_retention(db, Some(&key(0))).await.unwrap().unwrap();
    assert!(
        retained
            .audit
            .iter()
            .any(|lost| lost.tenant_id == tenant_id)
    );
    seal(db, &heads, tenant_id).await.unwrap();
    let report = verified(&jeanne).await;
    assert_eq!(
        (&report["firstPlace"], &report["purged"]),
        (&json!(4), &json!(3))
    );
    assert_eq!(report["holds"], true, "{report}");

    // The start moved forward without the key, the place after it gone:
    // taken for none, the chain read from its first place, which is missing.
    let fourth = place(db, tenant_id, 4).await;
    owner_runs(&format!(
        "update audit_chain_starts set first_seq = 5, purged_entries = 4, \
         previous_hash = '\\x{}' where tenant_id = '{tenant_id}'",
        hex(&fourth.chain_hash)
    ))
    .await
    .unwrap();
    owner_runs(&format!(
        "delete from audit_log where tenant_id = '{tenant_id}' and chain_seq = 4"
    ))
    .await
    .unwrap();
    let report = verified(&jeanne).await;
    assert_eq!(
        (&report["firstPlace"], &report["purged"]),
        (&json!(1), &json!(0))
    );
    assert_eq!(report["holds"], false);
    assert_eq!(
        report["breaks"][0],
        json!({ "kind": "missing", "from": 1, "to": 4 })
    );
}
