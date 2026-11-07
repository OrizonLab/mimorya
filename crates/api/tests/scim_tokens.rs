//! The tokens with which a company's identity provider provisions its
//! accounts through SCIM (ADR-183): made by who manages the company's
//! accounts on the whole company (`user:manage`), the secret shown once,
//! never listed, logged nor audited; listed with where the provider reaches
//! SCIM; five at most; removed, the token reaches nothing more. An account
//! administrator of a unit only, or who reads the audit log, manages none.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_partial_admin, holder_of};
use support::audit_exports::audit_log;
use support::mcp::capture_logs;
use support::scim::{TOKENS, get, made, removed};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn who_manages_the_company_s_accounts_makes_scim_tokens_shown_once() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let logs = capture_logs();
    let (id, secret) = made(&paul, "  Microsoft Entra ID ").await;
    assert!(secret.starts_with("msc_"), "{secret}");

    // Listed by its name, never its secret, with where the provider reaches
    // SCIM and the limits a new token meets.
    let listed = paul.get(TOKENS).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    assert!(!listed.text().contains(&secret));
    let tokens = listed.json();
    let address = tokens["scimUrl"].as_str().unwrap();
    assert!(address.ends_with("/api/scim/v2"), "{address}");
    assert_eq!(
        (&tokens["maxTokens"], &tokens["nameLength"]),
        (&json!(5), &json!(80))
    );
    let token = &tokens["tokens"][0];
    assert_eq!(
        (&token["id"], &token["name"], &token["createdBy"]),
        (&json!(id), &json!("Microsoft Entra ID"), &json!(user::PAUL))
    );
    assert_eq!(token["lastUsedAt"], Value::Null);

    // The provider reaches SCIM with it; its use is noted.
    let config = get(&api, Some(&secret), "/ServiceProviderConfig").await;
    assert_eq!(config.status, StatusCode::OK, "{}", config.text());
    let used = paul.get(TOKENS).await.json();
    assert!(used["tokens"][0]["lastUsedAt"].is_string());

    // Five at most.
    let mut others = Vec::new();
    for n in 2..=5 {
        others.push(made(&paul, &format!("Fournisseur {n}")).await.0);
    }
    let sixth = paul.post(TOKENS, json!({ "name": "Fournisseur 6" })).await;
    assert_eq!(sixth.status, StatusCode::CONFLICT, "{}", sixth.text());
    assert!(
        sixth.error_message().contains("5 jetons SCIM"),
        "{}",
        sixth.text()
    );
    for other in &others {
        removed(&paul, other).await;
    }

    // Removed, it reaches nothing more.
    removed(&paul, &id).await;
    let gone = get(&api, Some(&secret), "/Users").await;
    assert_eq!(gone.status, StatusCode::UNAUTHORIZED, "{}", gone.text());
    let again = paul.delete(&format!("{TOKENS}/{id}")).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND, "{}", again.text());

    // Its making and removal audited with its name only; its secret in no
    // log, no list and no audit entry.
    let entries = audit_log(&paul).await;
    for action in ["scim.token_created", "scim.token_deleted"] {
        let entry = entries
            .iter()
            .find(|e| e["action"] == action && e["resourceId"] == json!(id))
            .unwrap_or_else(|| panic!("{action} audited"));
        assert_eq!(entry["resourceType"], "scim_token");
        assert_eq!(entry["metadata"], json!({ "name": "Microsoft Entra ID" }));
        assert_eq!(entry["actorUserId"], json!(user::PAUL));
    }
    let random = &secret["msc_".len()..];
    assert!(!serde_json::to_string(&entries).unwrap().contains(random));
    assert!(!logs.text().contains(random));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_company_wide_account_manager_manages_scim_tokens() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (id, _) = made(&paul, "Okta").await;

    // An administrator of the north team only, and who reads the audit log:
    // neither lists, makes nor removes a token.
    let mut partial = create_partial_admin(&api, &paul, "scim.partiel").await;
    let partial = activate(&api, &mut partial).await;
    let reader = holder_of(&api, &paul, "scim.lecteur", &["audit:read"]).await;
    for client in [&partial, &reader] {
        assert_eq!(client.get(TOKENS).await.status, StatusCode::FORBIDDEN);
        let making = client.post(TOKENS, json!({ "name": "Détourné" })).await;
        assert_eq!(making.status, StatusCode::FORBIDDEN, "{}", making.text());
        let removal = client.delete(&format!("{TOKENS}/{id}")).await;
        assert_eq!(removal.status, StatusCode::FORBIDDEN, "{}", removal.text());
    }
    // Who manages accounts on the whole company does.
    let manager = holder_of(&api, &paul, "scim.gestion", &["user:manage"]).await;
    let listed = manager.get(TOKENS).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    assert_eq!(listed.json()["tokens"][0]["id"], json!(id));
    // A name is asked for, and kept short.
    for name in [json!("  "), json!("x".repeat(81))] {
        let refused = manager.post(TOKENS, json!({ "name": name })).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
    }
    removed(&manager, &id).await;
}
