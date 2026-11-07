//! The verification of its company's audit chain an administrator asks for
//! (ADR-179): the whole chain read under the server's key, against the
//! anchors they kept, their company's only; each verification traced, as a
//! failure when the chain does not hold. Who may not ask, an anchor naming
//! another company, a server without a master key and a process already
//! running its verifications are refused.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::audit_chain::{CHECKS_AT_ONCE, SealedHeads, anchor_text, hex};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::audit_chain::ChainHead;
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept, activate, create_partial_admin, holder_of};
use support::audit_chain::{append, as_owner, chain, seal};
use support::mcp::capture_logs;
use support::{Client, TestApi, TestResponse, companies, llm};

const VERIFY: &str = "/admin/audit/verifications";

async fn keyed() -> TestApi {
    TestApi::start_with(&[("MIMORYA_MASTER_KEY", llm::MASTER_KEY)]).await
}

async fn verify(client: &Client<'_>, anchors: &[String]) -> TestResponse {
    client.post(VERIFY, json!({ "anchors": anchors })).await
}

/// The last verification the company's audit log traces.
async fn last_traced(client: &Client<'_>) -> Value {
    let log = client.get("/admin/audit?limit=20").await.json();
    log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["action"] == "audit.chain_verified")
        .cloned()
        .expect("verification traced")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_verifies_the_chain_against_the_anchors_they_kept() {
    let api = keyed().await;
    let db = &api.app.services.db;
    append(db, tenant::ID, "test.verified").await;
    seal(db, &SealedHeads::default(), tenant::ID).await.unwrap();
    let paul = api.sign_in(user::PAUL).await;

    let first = verify(&paul, &[]).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    let report = first.json();
    assert_eq!(report["holds"], true, "{report}");
    assert_eq!(report["broken"], 0);
    let place = report["last"]["place"].as_i64().unwrap();
    assert!(
        place >= 1 && report["verified"].as_i64() == Some(place),
        "{report}"
    );
    let seal_text = report["last"]["seal"].as_str().unwrap().to_string();
    let head = ChainHead {
        chain_seq: place,
        chain_hash: (0..32)
            .map(|at| u8::from_str_radix(&seal_text[2 * at..2 * at + 2], 16).unwrap())
            .collect(),
    };
    // The anchor shown is the one the sealing logs.
    let anchor = anchor_text(tenant::ID, &head);
    assert_eq!(report["last"]["anchor"], json!(anchor));
    assert_eq!(hex(&head.chain_hash), seal_text);
    let traced = last_traced(&paul).await;
    assert_eq!(traced["outcome"], "success");
    assert_eq!(traced["actorUserId"], json!(user::PAUL));
    assert_eq!(traced["metadata"]["lastPlace"], json!(place));

    // Kept, by the company's identifier or slug, the anchor holds; a seal
    // that is not the chain's, or a place it does not reach, does not.
    let anchors = [
        anchor.clone(),
        format!("{}:{place}:{seal_text}", tenant::SLUG),
        format!("{}:{place}:{}", tenant::ID, "00".repeat(32)),
        format!("{}:{}:{seal_text}", tenant::ID, place + 1_000_000),
    ];
    let anchored = verify(&paul, &anchors).await;
    assert_eq!(anchored.status, StatusCode::OK, "{}", anchored.text());
    let report = anchored.json();
    let found: Vec<&str> = report["anchors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|anchor| anchor["found"].as_str().unwrap())
        .collect();
    assert_eq!(found, ["held", "held", "differs", "missing"]);
    assert_eq!(report["anchors"][3]["place"], json!(place + 1_000_000));
    assert_eq!(
        (&report["holds"], &report["broken"]),
        (&json!(false), &json!(0))
    );
    let traced = last_traced(&paul).await;
    assert_eq!(traced["outcome"], "failure");
    assert_eq!(traced["metadata"]["anchorsMissed"], 2);
    assert!(traced["reason"].as_str().unwrap().contains("ancre"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_broken_chain_is_told_and_traced_as_a_failure() {
    let api = keyed().await;
    let db = &api.app.services.db;
    let address = "jeanne@chaine-verifiee.example";
    let company = companies::create(&api, "chaine-verifiee", address).await;
    let accepted = accept(&api, &company.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    for action in ["test.a", "test.b", "test.c"] {
        append(db, company.tenant_id, action).await;
    }
    seal(db, &SealedHeads::default(), company.tenant_id)
        .await
        .unwrap();
    let places = chain(db, company.tenant_id).await;
    let (altered, place) = places[places.len() - 2];
    as_owner(
        "update audit_log set action = 'test.forged' where id = $1",
        altered,
    )
    .await
    .unwrap();

    let logs = capture_logs();
    let broken = verify(&jeanne, &[]).await;
    assert_eq!(broken.status, StatusCode::OK, "{}", broken.text());
    let report = broken.json();
    // The operator is told too, of the company and its count, never a seal.
    let logged = logs.text();
    assert!(
        logged.contains("Audit chain verified on demand does not hold")
            && logged.contains(&company.tenant_id.to_string()),
        "{logged}"
    );
    let seal_text = report["last"]["seal"].as_str().unwrap();
    assert!(!logged.contains(seal_text), "{logged}");
    assert_eq!(
        (&report["holds"], &report["broken"]),
        (&json!(false), &json!(1))
    );
    assert_eq!(
        report["breaks"],
        json!([{ "kind": "altered", "place": place, "entryId": altered }])
    );
    let traced = last_traced(&jeanne).await;
    assert_eq!(traced["outcome"], "failure");
    assert_eq!(traced["reason"], "La chaîne du journal d’audit est rompue");
    assert_eq!(traced["metadata"]["broken"], 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_company_wide_audit_reader_verifies_and_only_with_their_company_s_anchors() {
    let api = keyed().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    assert_eq!(verify(&lucas, &[]).await.status, StatusCode::FORBIDDEN);
    // An administrator of a team only: its temporary password replaced.
    let mut partial = create_partial_admin(&api, &paul, "admin.audit.nord").await;
    let north = activate(&api, &mut partial).await;
    assert_eq!(verify(&north, &[]).await.status, StatusCode::FORBIDDEN);
    // Reading the audit log company-wide is enough.
    let officer = holder_of(&api, &paul, "verif.journal", &["audit:read"]).await;
    assert_eq!(verify(&officer, &[]).await.status, StatusCode::OK);

    // Another company's anchor, by its identifier or its slug, is refused
    // alike whether that company exists or not.
    let seal_text = "ab".repeat(32);
    let unknown = uuid::Uuid::new_v4();
    for (named, absent) in [
        (
            mimorya_db::testing::OTHER_TENANT_ID.to_string(),
            unknown.to_string(),
        ),
        (
            "autre-entreprise".to_string(),
            "aucune-entreprise".to_string(),
        ),
    ] {
        let existing = verify(&paul, &[format!("{named}:4:{seal_text}")]).await;
        let missing = verify(&paul, &[format!("{absent}:4:{seal_text}")]).await;
        assert_eq!(existing.status, StatusCode::BAD_REQUEST);
        assert_eq!(missing.status, StatusCode::BAD_REQUEST);
        let said = |response: &TestResponse, name: &str| {
            response.json()["error"]["message"]
                .as_str()
                .unwrap()
                .replace(name, "…")
        };
        assert_eq!(said(&existing, &named), said(&missing, &absent));
        assert!(said(&existing, &named).contains("n’est pas votre entreprise"));
    }
    let malformed = verify(&paul, &["neo-maintenance:4".to_string()]).await;
    assert_eq!(malformed.status, StatusCode::BAD_REQUEST);
    assert!(malformed.text().contains("<entreprise>:<place>:<sceau>"));
    let many: Vec<String> = (1..=21)
        .map(|place| format!("{}:{place}:{seal_text}", tenant::SLUG))
        .collect();
    assert_eq!(verify(&paul, &many).await.status, StatusCode::BAD_REQUEST);
    // Twenty are taken, each told.
    let twenty = verify(&paul, &many[..20]).await;
    assert_eq!(twenty.status, StatusCode::OK, "{}", twenty.text());
    assert_eq!(twenty.json()["anchors"].as_array().map(Vec::len), Some(20));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_server_without_a_master_key_or_already_verifying_refuses() {
    let unsealed = TestApi::start().await;
    let paul = unsealed.sign_in(user::PAUL).await;
    let refused = verify(&paul, &[]).await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        refused.text().contains("MIMORYA_MASTER_KEY"),
        "{}",
        refused.text()
    );
    drop(paul);

    let api = keyed().await;
    let paul = api.sign_in(user::PAUL).await;
    let checks = &api.app.services.audit_checks;
    let places: Vec<_> = (0..CHECKS_AT_ONCE).map_while(|_| checks.take()).collect();
    assert_eq!(places.len(), CHECKS_AT_ONCE);
    let busy = verify(&paul, &[]).await;
    assert_eq!(busy.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(busy.text().contains("en cours"), "{}", busy.text());
    drop(places);
    assert_eq!(verify(&paul, &[]).await.status, StatusCode::OK);
}
