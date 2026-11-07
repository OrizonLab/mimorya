//! The company's audit log read by its SIEM with a token (ADR-181): the
//! token shown once, never listed, logged nor audited; the sealed entries in
//! the chain's order, page after page, each with its place and its seal, an
//! anchor the company's verification holds. A token reads its company only,
//! nothing once removed or while its company is suspended; a session proves
//! nothing there. Only who may export the log on the whole company manages
//! the tokens (the refusals traced), five at most. A server without a master
//! key exports nothing.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mimorya_api::services::audit_chain::SealedHeads;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept, activate, create_partial_admin, holder_of};
use support::audit_chain::{append, chain, seal};
use support::mcp::capture_logs;
use support::queue::as_owner;
use support::{Client, TestApi, TestResponse, companies, llm};

const TOKENS: &str = "/admin/audit/export-tokens";

async fn keyed() -> TestApi {
    TestApi::start_with(&[("MIMORYA_MASTER_KEY", llm::MASTER_KEY)]).await
}

/// What a SIEM holding `token` reads at `/audit/export{query}`.
async fn read(api: &TestApi, token: Option<&str>, query: &str) -> TestResponse {
    let mut request = Request::get(format!("/audit/export{query}"));
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    api.send(request.body(Body::empty()).unwrap()).await
}

/// A token named `name`: its identifier and its secret.
async fn made(client: &Client<'_>, name: &str) -> (String, String) {
    let created = client.post(TOKENS, json!({ "name": name })).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let body = created.json();
    let id = body["token"]["id"].as_str().unwrap().to_string();
    (id, body["secret"].as_str().unwrap().to_string())
}

async fn removed(client: &Client<'_>, id: &str) {
    let deleted = client.delete(&format!("{TOKENS}/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}

/// The company's audit log, newest first, as its administration reads it.
async fn audit_log(client: &Client<'_>) -> Vec<Value> {
    let log = client.get("/admin/audit?limit=100").await.json();
    log["entries"].as_array().unwrap().clone()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_siem_reads_the_sealed_log_page_after_page_with_its_token() {
    let api = keyed().await;
    let db = &api.app.services.db;
    let paul = api.sign_in(user::PAUL).await;
    let logs = capture_logs();
    let (id, secret) = made(&paul, "  SIEM de la DSI ").await;
    assert!(secret.starts_with("mae_"), "{secret}");
    // Listed by its name, never its secret; whether this server seals.
    let listed = paul.get(TOKENS).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    assert!(!listed.text().contains(&secret));
    let tokens = listed.json();
    assert_eq!(tokens["sealing"], true);
    let token = &tokens["tokens"][0];
    assert_eq!(
        (&token["id"], &token["name"], &token["createdBy"]),
        (&json!(id), &json!("SIEM de la DSI"), &json!(user::PAUL))
    );
    assert_eq!(token["lastUsedAt"], Value::Null);
    // Its making audited with its name only.
    let made_entry = audit_log(&paul).await.into_iter().find(|entry| {
        entry["action"] == "audit.export_token_created" && entry["resourceId"] == json!(id)
    });
    let made_entry = made_entry.expect("token making audited");
    assert_eq!(made_entry["metadata"], json!({ "name": "SIEM de la DSI" }));
    assert_eq!(made_entry["actorUserId"], json!(user::PAUL));

    let first = append(db, tenant::ID, "test.exported").await;
    for action in ["test.exported.b", "test.exported.c"] {
        append(db, tenant::ID, action).await;
    }
    seal(db, &SealedHeads::default(), tenant::ID).await.unwrap();
    let start = chain(db, tenant::ID)
        .await
        .iter()
        .find(|(entry, _)| *entry == first)
        .unwrap()
        .1
        - 1;

    // Two at a time from the place before the first, until none follow.
    let mut read_back = Vec::new();
    let mut after = start;
    loop {
        let page = read(&api, Some(&secret), &format!("?after={after}&limit=2")).await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.text());
        let page = page.json();
        assert_eq!(page["company"], json!(tenant::ID));
        let entries = page["entries"].as_array().unwrap();
        assert!(entries.len() <= 2);
        for entry in entries {
            let seal_text = entry["seal"].as_str().unwrap();
            assert_eq!(seal_text.len(), 64, "{entry}");
            assert!(seal_text.chars().all(|c| c.is_ascii_hexdigit()));
            read_back.push((
                entry["id"].as_i64().unwrap(),
                entry["place"].as_i64().unwrap(),
            ));
            if entry["id"] == json!(first) {
                assert_eq!(entry["action"], "test.exported");
                assert_eq!(
                    (&entry["actorType"], &entry["outcome"]),
                    (&json!("user"), &json!("success"))
                );
                assert_eq!(entry["metadata"], json!({ "détail": [1, 2.5, null] }));
                assert_eq!(entry["ip"], "192.0.2.7");
            }
        }
        if page["more"] == false {
            break;
        }
        assert_eq!(entries.len(), 2);
        after = read_back.last().unwrap().1;
    }
    // Every place after the start, in order, as the chain holds them (the
    // maintenance may seal more meanwhile: read up to the last place read).
    let reached = read_back.last().unwrap().1;
    let expected: Vec<(i64, i64)> = chain(db, tenant::ID)
        .await
        .into_iter()
        .filter(|(_, at)| *at > start && *at <= reached)
        .collect();
    assert!(expected.len() >= 3, "{expected:?}");
    assert_eq!(read_back, expected);

    // A read is noted on the token, not in the audit log.
    let newest = audit_log(&paul).await[0]["id"].clone();
    let last = read(&api, Some(&secret), &format!("?after={}", reached - 1)).await;
    let last = last.json();
    assert_eq!(audit_log(&paul).await[0]["id"], newest);
    // The last seal read is an anchor the company's verification holds.
    let entry = last["entries"].as_array().unwrap().last().unwrap().clone();
    let anchor = format!(
        "{}:{}:{}",
        tenant::ID,
        entry["place"],
        entry["seal"].as_str().unwrap()
    );
    let verified = paul
        .post("/admin/audit/verifications", json!({ "anchors": [anchor] }))
        .await;
    assert_eq!(
        verified.json()["anchors"][0]["found"],
        "held",
        "{}",
        verified.text()
    );
    let tokens = paul.get(TOKENS).await.json();
    assert!(tokens["tokens"][0]["lastUsedAt"].is_string(), "{tokens}");
    // Its secret neither logged nor audited.
    let audited = serde_json::to_string(&audit_log(&paul).await).unwrap();
    assert!(!audited.contains(&secret[4..]));
    assert!(!logs.text().contains(&secret[4..]));
    removed(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_token_reads_its_company_only_and_nothing_once_removed_or_suspended() {
    let api = keyed().await;
    let db = &api.app.services.db;
    let address = "jeanne@export-siem.example";
    let company = companies::create(&api, "export-siem", address).await;
    let accepted = accept(&api, &company.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    let paul = api.sign_in(user::PAUL).await;
    let (theirs, their_secret) = made(&jeanne, "SIEM Acme").await;
    let (ours, our_secret) = made(&paul, "SIEM Néo").await;
    for tenant_id in [company.tenant_id, tenant::ID] {
        append(db, tenant_id, "test.isolated").await;
        seal(db, &SealedHeads::default(), tenant_id).await.unwrap();
    }

    // Each token its company's chain, from its first place.
    for (secret, tenant_id) in [
        (&their_secret, company.tenant_id),
        (&our_secret, tenant::ID),
    ] {
        let page = read(&api, Some(secret), "?limit=1000").await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.text());
        let page = page.json();
        assert_eq!(page["company"], json!(tenant_id));
        let ids: Vec<i64> = page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["id"].as_i64().unwrap())
            .collect();
        let own: Vec<i64> = chain(db, tenant_id)
            .await
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(ids, own[..ids.len()]);
        assert_eq!(page["entries"][0]["place"], 1);
    }
    // Another company's token is not listed, nor removed.
    assert!(!paul.get(TOKENS).await.text().contains(&theirs));
    let elsewhere = paul.delete(&format!("{TOKENS}/{theirs}")).await;
    assert_eq!(elsewhere.status, StatusCode::NOT_FOUND);
    assert_eq!(
        read(&api, Some(&their_secret), "").await.status,
        StatusCode::OK
    );

    // Unknown, missing, a session's cookie only: refused alike.
    let unknown = read(&api, Some("mae_inconnu"), "").await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    let refusal = unknown.text();
    assert_eq!(read(&api, None, "").await.text(), refusal);
    let by_session = paul.get("/audit/export").await;
    assert_eq!(by_session.status, StatusCode::UNAUTHORIZED);
    assert_eq!(by_session.text(), refusal);

    // Their company suspended, then active again.
    let status = "update tenants set status = $2 where id = $1";
    as_owner(&status.replace("$2", "'suspended'"), company.tenant_id).await;
    let suspended = read(&api, Some(&their_secret), "").await;
    assert_eq!(suspended.status, StatusCode::UNAUTHORIZED);
    assert_eq!(suspended.text(), refusal);
    as_owner(&status.replace("$2", "'active'"), company.tenant_id).await;
    assert_eq!(
        read(&api, Some(&their_secret), "").await.status,
        StatusCode::OK
    );

    // Removed, the token reads nothing more; its removal audited.
    removed(&jeanne, &theirs).await;
    let gone = read(&api, Some(&their_secret), "").await;
    assert_eq!(
        (gone.status, gone.text()),
        (StatusCode::UNAUTHORIZED, refusal)
    );
    let deleted = audit_log(&jeanne).await.into_iter().find(|entry| {
        entry["action"] == "audit.export_token_deleted" && entry["resourceId"] == json!(theirs)
    });
    assert_eq!(
        deleted.expect("removal audited")["metadata"]["name"],
        "SIEM Acme"
    );
    removed(&paul, &ours).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_company_wide_exporter_manages_the_tokens_five_at_most() {
    let api = keyed().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let mut partial = create_partial_admin(&api, &paul, "admin.export.nord").await;
    let north = activate(&api, &mut partial).await;
    let reader = holder_of(&api, &paul, "lecteur.journal", &["audit:read"]).await;
    let (id, _) = made(&paul, "SIEM").await;
    for (refused, who) in [
        (&lucas, "Lucas"),
        (&north, "team admin"),
        (&reader, "reader"),
    ] {
        let listed = refused.get(TOKENS).await;
        assert_eq!(listed.status, StatusCode::FORBIDDEN, "{who}");
        let created = refused.post(TOKENS, json!({ "name": "SIEM" })).await;
        assert_eq!(created.status, StatusCode::FORBIDDEN, "{who}");
        let deleted = refused.delete(&format!("{TOKENS}/{id}")).await;
        assert_eq!(deleted.status, StatusCode::FORBIDDEN, "{who}");
    }
    // Their attempts to make one traced, as the administration's are.
    for refused in [user::LUCAS, partial.id] {
        let traced = audit_log(&paul).await.into_iter().any(|entry| {
            entry["action"] == "admin.refused"
                && entry["actorUserId"] == json!(refused)
                && entry["metadata"]["method"] == "POST"
                && entry["metadata"]["route"]
                    .as_str()
                    .is_some_and(|r| r.ends_with(TOKENS))
        });
        assert!(traced, "{refused}'s refusal is not traced");
    }
    removed(&paul, &id).await;

    // Exporting the log on the whole company is enough.
    let exporter = holder_of(&api, &paul, "export.journal", &["audit:export"]).await;
    let mut kept = Vec::new();
    while exporter.get(TOKENS).await.json()["tokens"]
        .as_array()
        .unwrap()
        .len()
        < 5
    {
        kept.push(made(&exporter, &format!("SIEM {}", kept.len())).await.0);
    }
    let sixth = exporter
        .post(TOKENS, json!({ "name": "SIEM de trop" }))
        .await;
    assert_eq!(sixth.status, StatusCode::CONFLICT, "{}", sixth.text());
    for body in [
        json!({ "name": "   " }),
        json!({ "name": "x".repeat(81) }),
        json!({ "name": "SIEM", "scope": "all" }),
    ] {
        let refused = paul.post(TOKENS, body).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
    }
    // An unknown token answered as absent, and traced as such (ADR-047).
    let absent = uuid::Uuid::new_v4();
    assert_eq!(
        exporter.delete(&format!("{TOKENS}/{absent}")).await.status,
        StatusCode::NOT_FOUND
    );
    let traced = audit_log(&paul).await.into_iter().any(|entry| {
        entry["action"] == "admin.not_found"
            && entry["resourceType"] == "audit_export_token"
            && entry["resourceId"] == json!(absent)
    });
    assert!(traced, "the absent token is not traced");
    for id in &kept {
        removed(&exporter, id).await;
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_page_asked_amiss_or_a_server_without_a_master_key_is_refused() {
    let api = keyed().await;
    let paul = api.sign_in(user::PAUL).await;
    let (id, secret) = made(&paul, "SIEM").await;
    for query in [
        "?limit=0",
        "?limit=1001",
        "?limit=beaucoup",
        "?after=-1",
        "?depuis=3",
    ] {
        let refused = read(&api, Some(&secret), query).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{query}");
    }
    let most = read(&api, Some(&secret), "?limit=1000").await;
    assert_eq!(most.status, StatusCode::OK, "{}", most.text());
    removed(&paul, &id).await;
    drop(paul);
    drop(api);

    // Tokens are made where nothing is sealed, but read nothing there.
    let unsealed = TestApi::start().await;
    let paul = unsealed.sign_in(user::PAUL).await;
    let (id, secret) = made(&paul, "SIEM").await;
    assert_eq!(paul.get(TOKENS).await.json()["sealing"], false);
    let refused = read(&unsealed, Some(&secret), "").await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        refused.text().contains("MIMORYA_MASTER_KEY"),
        "{}",
        refused.text()
    );
    // An unknown token is refused first.
    let unknown = read(&unsealed, Some("mae_inconnu"), "").await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    removed(&paul, &id).await;
}
