//! REST APIs a company adds (ADR-112): added by its connector administrators
//! with a key sealed, listed without it, tested and mapped as an MCP server
//! is; then called in a run, a mapped read directly, an unmapped write
//! after its person validated what it sends, the key masked in what the
//! model reads.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_connectors::testing::serve_rest_http;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::rest::{CREATE, KEY, READ, add_rest, rest_body};

/// The audit metadata of the connector, oldest first.
async fn audited(api: &TestApi, id: &str) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar("select metadata from audit_log where resource_id = $1 order by id")
        .bind(id)
        .fetch_all(&mut *tx)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_rest_api_is_added_tested_and_mapped() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let url = serve_rest_http().await;
    let id = add_rest(&paul, rest_body("CRM", &url, Some(KEY))).await;
    let shown = paul.get("/admin/connectors").await;
    assert!(!shown.text().contains(KEY));
    let instance = listed(&paul, &id).await;
    assert_eq!(instance["type"], "rest");
    assert_eq!(instance["mcp"], Value::Null);
    assert_eq!(instance["rest"]["url"], url);
    assert_eq!(instance["rest"]["tokenHeader"], "X-Api-Key");
    assert_eq!(instance["rest"]["tokenStored"], true);
    let read = &instance["rest"]["operations"][0];
    assert_eq!(read["name"], "get_item");
    // A path parameter is required, said or not.
    assert_eq!(read["parameters"][0]["required"], true);

    // Unmapped: human validation and connector:use only.
    let tested = paul
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await
        .json();
    assert_eq!(tested["ok"], true, "{tested}");
    let names: Vec<&str> = tested["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, [READ, CREATE]);
    assert_eq!(tested["tools"][0]["risk"], "write");
    assert_eq!(tested["tools"][0]["permissions"], json!(["connector:use"]));
    assert_eq!(
        tested["tools"][0]["description"],
        "[API REST « crm »] Renvoie l’élément demandé."
    );

    let mapped = paul
        .patch(
            &format!("/admin/connectors/{id}"),
            json!({ "toolPolicies": { READ: { "risk": "read", "permissions": ["intervention:read"] } } }),
        )
        .await;
    assert_eq!(mapped.status, StatusCode::OK, "{}", mapped.text());
    let tested = paul
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await
        .json();
    assert_eq!(tested["tools"][0]["risk"], "read");
    assert_eq!(
        tested["tools"][0]["permissions"],
        json!(["connector:use", "intervention:read"])
    );

    let audited = audited(&api, &id).await;
    assert_eq!(audited[0]["type"], "rest");
    assert_eq!(audited[0]["operations"], json!(["get_item", "create_item"]));
    assert_eq!(audited[0]["tokenHeader"], "X-Api-Key");
    assert_eq!(audited[0]["tokenStored"], true);
    assert!(audited.iter().all(|m| !m.to_string().contains(KEY)));

    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
    assert_eq!(listed(&paul, &id).await, Value::Null);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let kept: i64 = sqlx::query_scalar("select count(*) from secrets where name = $1")
        .bind(format!("connector.{id}.token"))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(kept, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_reads_directly_and_writes_once_validated() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let url = serve_rest_http().await;
    let id = add_rest(&paul, rest_body("CRM", &url, Some(KEY))).await;
    let policies = json!({ READ: { "risk": "read", "permissions": ["intervention:read"] } });
    paul.patch(
        &format!("/admin/connectors/{id}"),
        json!({ "toolPolicies": policies }),
    )
    .await;

    api.script(vec![
        ScriptedStep::calls("", vec![call(READ, json!({ "id": "INT-42" }))]),
        ScriptedStep::calls("", vec![call(CREATE, json!({ "title": "Pompe P2" }))]),
    ]);
    let conversation = marc.post("/conversations", json!({})).await.json();
    let conversation_id = conversation["conversation"]["id"].as_str().unwrap();
    let started = marc
        .post(
            &format!("/conversations/{conversation_id}/messages"),
            json!({ "content": "Lis l’élément INT-42 puis crée la fiche de la pompe." }),
        )
        .await;
    let run_id = started.json()["runId"].as_str().unwrap().to_string();
    api.idle().await;

    // The read ran: what the model read has the key masked.
    let detail = marc.get(&format!("/runs/{run_id}")).await.json();
    let read = detail["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["toolName"] == READ)
        .cloned()
        .unwrap();
    assert_eq!(read["status"], "succeeded", "{detail}");
    let requests = api.model.as_ref().unwrap().requests();
    let answer = format!("{:?}", requests[1].messages.last());
    assert!(answer.contains("/api/echo/items/INT-42"), "{answer}");
    assert!(answer.contains("***") && !answer.contains(KEY), "{answer}");
    // The write waits, its request shown whole.
    let approvals = marc.get("/approvals?status=pending").await.json();
    let pending = &approvals["approvals"][0];
    assert_eq!(pending["toolName"], CREATE, "{approvals}");
    assert_eq!(
        pending["actionSummary"],
        format!(
            "Appeler « Créer un élément » (API REST « crm ») : POST {url}/echo/items avec le \
             corps {{\"title\":\"Pompe P2\"}}"
        )
    );
    api.script(vec![ScriptedStep::text("La fiche est créée.")]);
    let decision = format!("/approvals/{}/decision", pending["id"].as_str().unwrap());
    let approved = marc.post(&decision, json!({ "approved": true })).await;
    assert_eq!(approved.status, StatusCode::OK, "{}", approved.text());
    api.idle().await;
    let detail = marc.get(&format!("/runs/{run_id}")).await.json();
    assert_eq!(detail["run"]["status"], "completed", "{detail}");
    // Sent as validated: the request answered.
    let created = detail["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["toolName"] == CREATE && t["status"] == "succeeded");
    assert!(created, "{detail}");
    let requests = api.model.as_ref().unwrap().requests();
    let answer = format!("{:?}", requests[2].messages.last());
    assert!(
        answer.contains("POST") && answer.contains("Pompe P2"),
        "{answer}"
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
