//! The rules of a REST API's administration (ADR-112): only connector
//! administrators add one; its declaration is checked as a whole; prefixes
//! never overlap among APIs; a change of address takes the key away, a
//! change of its header or operations is checked again; an operation's
//! description is changed in its declaration, not by a policy; an MCP
//! server has neither header nor operations; a public platform reaches
//! public HTTPS addresses only.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::serve_rest_http;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::mcp::{PRIVATE_DEPLOYMENT, add, last_update, listed, mcp_body};
use support::rest::{CREATE, KEY, READ, add_rest, operations, rest_body};

/// The message of a refusal, with its status.
async fn refused(
    admin: &support::Client<'_>,
    path: &str,
    body: Value,
    patch: bool,
) -> (StatusCode, String) {
    let answer = if patch {
        admin.patch(path, body).await
    } else {
        admin.post(path, body).await
    };
    (answer.status, answer.error_message())
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_well_formed_api_is_added_by_its_administrators() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let url = serve_rest_http().await;
    let forbidden = marc
        .post("/admin/connectors/rest", rest_body("CRM", &url, None))
        .await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);

    let mut escaping = rest_body("CRM", &url, None);
    escaping["operations"][0]["path"] = json!("/echo/../{id}");
    let (status, message) = refused(&paul, "/admin/connectors/rest", escaping, false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        message,
        "Opération get_item : segment . ou .. refusé dans le chemin"
    );
    let mut reserved = rest_body("CRM", &url, None);
    reserved["tokenHeader"] = json!("Cookie");
    let (_, message) = refused(&paul, "/admin/connectors/rest", reserved, false).await;
    assert!(message.starts_with("En-tête réservé"), "{message}");
    let (_, message) = refused(
        &paul,
        "/admin/connectors/rest",
        rest_body("CRM", &url, Some("clé à espaces")),
        false,
    )
    .await;
    assert!(message.starts_with("Jeton invalide"), "{message}");
    let mut unknown = rest_body("CRM", &url, None);
    unknown["operations"][0]["verb"] = json!("GET");
    let unread = paul.post("/admin/connectors/rest", unknown).await;
    assert_eq!(unread.status, StatusCode::BAD_REQUEST);

    let id = add_rest(&paul, rest_body("CRM", &url, Some(KEY))).await;
    // Another API under the same prefix: its tools would be named alike.
    let (status, message) = refused(
        &paul,
        "/admin/connectors/rest",
        rest_body("CRM bis", &url, None),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        message,
        "Ce préfixe est déjà utilisé par une autre API REST"
    );
    // An MCP server may share it: its tools are named mcp_crm_….
    let mcp = add(
        &paul,
        mcp_body("CRM MCP", "crm", "http://127.0.0.1:9/mcp", None),
    )
    .await;
    for id in [id, mcp] {
        assert_eq!(
            paul.delete(&format!("/admin/connectors/{id}")).await.status,
            StatusCode::OK
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_change_is_checked_and_the_key_never_follows_the_api_elsewhere() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let url = serve_rest_http().await;
    let id = add_rest(&paul, rest_body("CRM", &url, Some(KEY))).await;
    let path = format!("/admin/connectors/{id}");

    // The operations replaced, checked as a whole.
    let mut changed = operations();
    changed[1]["parameters"] = json!([]);
    changed[0]["path"] = json!("/echo/items/{id}/{other}");
    let (status, message) = refused(&paul, &path, json!({ "operations": changed }), true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        message,
        "Opération get_item : {other} figure dans le chemin sans être un paramètre de chemin"
    );
    let read = json!({ "risk": "read", "permissions": ["intervention:read"] });
    let mapped = json!({ "toolPolicies": { READ: read, CREATE: read } });
    assert_eq!(paul.patch(&path, mapped).await.status, StatusCode::OK);
    let one = json!([operations()[0]]);
    let saved = paul
        .patch(&path, json!({ "operations": one, "tokenHeader": null }))
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let instance = listed(&paul, &id).await;
    assert_eq!(instance["rest"]["operations"].as_array().unwrap().len(), 1);
    assert_eq!(instance["rest"]["tokenHeader"], Value::Null);
    // The key now goes elsewhere: what was known of the API no longer holds.
    assert_eq!(instance["status"], "untested");
    assert_eq!(instance["rest"]["tokenStored"], true);
    // The removed operation's policy went with it; the other one stays.
    let policies = instance["toolPolicies"].as_object().unwrap();
    assert!(policies.contains_key(READ) && !policies.contains_key(CREATE));
    // A read whose parameters change is no longer the read mapped.
    let mut filtered = json!([operations()[0]]);
    filtered[0]["parameters"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "status", "in": "query", "type": "string" }));
    let saved = paul.patch(&path, json!({ "operations": filtered })).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(listed(&paul, &id).await["toolPolicies"], json!({}));
    let remapped = json!({ "toolPolicies": { READ: read } });
    assert_eq!(paul.patch(&path, remapped).await.status, StatusCode::OK);
    // Nor one whose method or path changes.
    let mut deleting = filtered.clone();
    deleting[0]["method"] = json!("DELETE");
    let saved = paul.patch(&path, json!({ "operations": deleting })).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(listed(&paul, &id).await["toolPolicies"], json!({}));
    let audited = last_update(&api, &id).await;
    assert_eq!(audited["droppedPolicies"], json!([READ]));
    // The policies audited are those that changed: the one dropped (ADR-160).
    assert_eq!(audited["toolPolicies"], json!({ READ: null }));

    // A policy names a declared operation only, never one to come.
    let unknown = json!({ "toolPolicies": { "rest_crm_export_all": { "risk": "read" } } });
    let (status, message) = refused(&paul, &path, unknown, true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        message,
        "Opération inconnue de cette API : rest_crm_export_all"
    );
    let mut exporting = json!([operations()[0]]);
    exporting.as_array_mut().unwrap().push(json!({
        "name": "export_all", "title": "Exporter", "description": "Exporte tout.",
        "method": "GET", "path": "/export",
    }));
    let both = json!({
        "operations": exporting,
        "toolPolicies": { "rest_crm_export_all": { "risk": "read" } },
    });
    let saved = paul.patch(&path, both).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(
        listed(&paul, &id).await["toolPolicies"]["rest_crm_export_all"]["risk"],
        "read"
    );

    // A policy maps a tool; it does not rewrite what the model reads of it.
    let described = json!({ "toolPolicies": { READ: { "description": "Autre texte" } } });
    let (status, message) = refused(&paul, &path, described, true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        message,
        format!("Description de {READ} : elle se change dans la déclaration de l’opération")
    );

    // Elsewhere, without the key.
    let moved = paul
        .patch(&path, json!({ "url": format!("{url}/v2") }))
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    let instance = listed(&paul, &id).await;
    assert_eq!(instance["rest"]["url"], format!("{url}/v2"));
    assert_eq!(instance["rest"]["tokenStored"], false);

    // An MCP server has neither.
    let mcp = add(
        &paul,
        mcp_body("Docs", "docs", "http://127.0.0.1:9/mcp", None),
    )
    .await;
    let (status, message) = refused(
        &paul,
        &format!("/admin/connectors/{mcp}"),
        json!({ "operations": operations() }),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        message,
        "Seule une API REST a un en-tête de jeton et des opérations"
    );
    for id in [id, mcp] {
        assert_eq!(
            paul.delete(&format!("/admin/connectors/{id}")).await.status,
            StatusCode::OK
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_public_platform_reaches_public_https_apis_only() {
    let api = TestApi::start_with(&[("MIMORYA_MASTER_KEY", support::llm::MASTER_KEY)]).await;
    let paul = api.sign_in(user::PAUL).await;
    for (url, expected) in [
        ("http://api.example.com/v1", "Adresse en https:// attendue"),
        (
            "https://10.0.0.8/v1",
            "Adresse d’un réseau privé : refusée sur ce serveur",
        ),
        (
            "https://api.example.com/v1?cle=1",
            "L’adresse ne contient ni identifiants, ni paramètres, ni fragment : utilisez le jeton",
        ),
    ] {
        let (status, message) = refused(
            &paul,
            "/admin/connectors/rest",
            rest_body("CRM", url, None),
            false,
        )
        .await;
        assert_eq!(
            (status, message.as_str()),
            (StatusCode::BAD_REQUEST, expected),
            "{url}"
        );
    }
    let id = add_rest(&paul, rest_body("CRM", "https://api.example.com/v1", None)).await;
    let (status, message) = refused(
        &paul,
        &format!("/admin/connectors/{id}"),
        json!({ "url": "https://127.0.0.1/v1" }),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        message,
        "Adresse d’un réseau privé : refusée sur ce serveur"
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_unreadable_declaration_is_deleted_rather_than_changed() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = add_rest(&paul, rest_body("CRM", "http://127.0.0.1:9/api", None)).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update connector_instances set config = config - 'operations' where id = $1::uuid",
    )
    .bind(&id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let path = format!("/admin/connectors/{id}");
    assert_eq!(listed(&paul, &id).await["rest"], Value::Null);
    let (status, message) = refused(
        &paul,
        &path,
        json!({ "url": "http://127.0.0.1:9/v2" }),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        message,
        "Déclaration de cette API illisible : supprimez-la puis ajoutez-la de nouveau"
    );
    let renamed = paul.patch(&path, json!({ "name": "CRM ancien" })).await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.text());
    assert_eq!(paul.delete(&path).await.status, StatusCode::OK);
}
