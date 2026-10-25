//! Where a deny policy acts (ADR-022): a condition about the person or the
//! actor withdraws the permission wherever it is checked (routes, the agent's
//! tools and skills index, `/auth/me`); a condition on the resource is only accepted for
//! permissions checked resource by resource; permissions the platform does not
//! check yet are refused rather than restricted in name only.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::accounts::permissions;
use support::planning::{apply_proposal, propose};
use support::{Client, TestApi, week};

/// The heading of the skills index in the system prompt.
const SKILL_INDEX: &str = "Compétences de l’entreprise";

fn agents_only() -> Value {
    json!({ "op": "eq", "left": { "attr": "context.actorType" }, "right": { "value": "agent" } })
}

fn policy(permissions: &[&str], condition: Value) -> Value {
    json!({ "description": "Politique de test", "permissions": permissions, "condition": condition })
}

async fn create(paul: &Client<'_>, body: Value) -> String {
    let created = paul.post("/admin/policies", body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_deny_for_agents_blocks_the_tool_but_not_the_person() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    create(&paul, policy(&["planning:apply"], agents_only())).await;
    // Marc keeps the permission himself.
    let me = marc.get("/auth/me").await.json();
    assert!(permissions(&me).contains(&"planning:apply".to_string()));

    let mut search = week();
    search["status"] = json!(["in_progress"]);
    api.script(vec![
        ScriptedStep::calls("", vec![call("interventions_search", search)]),
        propose("INT-2049", "13:00"),
        apply_proposal("J’applique le planning."),
        ScriptedStep::text("Je n’ai pas pu appliquer le planning."),
    ]);
    let conversation = marc.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let started = marc
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": "Programme un suivi pour les interventions en cours" }),
        )
        .await;
    assert_eq!(started.status, StatusCode::ACCEPTED);
    let run_id = started.json()["runId"].as_str().unwrap().to_string();
    api.idle().await;

    // The agent could propose, not apply: nothing waits for validation.
    assert!(
        marc.get("/approvals").await.json()["approvals"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let detail = marc.get(&format!("/runs/{run_id}")).await.json();
    let tools: Vec<(String, String)> = detail["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["toolName"].as_str().unwrap().to_string(),
                t["status"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(tools.contains(&("planning_propose".into(), "succeeded".into())));
    let applied: Vec<&String> = tools
        .iter()
        .filter(|(name, _)| name == "planning_apply")
        .map(|(_, status)| status)
        .collect();
    assert!(!applied.is_empty(), "{tools:?}");
    assert!(
        applied
            .iter()
            .all(|s| *s != "succeeded" && *s != "awaiting_approval"),
        "{tools:?}"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_deny_about_a_person_withdraws_the_permission_from_routes() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let only_marc = json!({ "op": "eq", "left": { "attr": "subject.userId" }, "right": { "value": user::MARC } });
    let id = create(&paul, policy(&["agent:use"], only_marc)).await;

    assert_eq!(
        marc.post("/conversations", json!({})).await.status,
        StatusCode::FORBIDDEN
    );
    let me = marc.get("/auth/me").await.json();
    assert!(!permissions(&me).contains(&"agent:use".to_string()));
    assert_eq!(
        lucas.post("/conversations", json!({})).await.status,
        StatusCode::CREATED
    );

    let off = paul
        .patch(
            &format!("/admin/policies/{id}"),
            json!({ "enabled": false }),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK);
    assert_eq!(
        marc.post("/conversations", json!({})).await.status,
        StatusCode::CREATED
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_deny_on_reading_skills_leaves_them_out_of_the_prompt() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let ask = async || {
        api.script(vec![ScriptedStep::text("Bonjour.")]);
        let conversation = marc.post("/conversations", json!({})).await.json();
        let id = conversation["conversation"]["id"].as_str().unwrap();
        let sent = marc
            .post(
                &format!("/conversations/{id}/messages"),
                json!({ "content": "Bonjour" }),
            )
            .await;
        assert_eq!(sent.status, StatusCode::ACCEPTED);
        api.idle().await;
        let requests = api.model.as_ref().unwrap().requests();
        requests.last().unwrap().system.clone()
    };
    assert!(ask().await.contains(SKILL_INDEX));

    let id = create(&paul, policy(&["skill:read"], agents_only())).await;
    assert!(!ask().await.contains(SKILL_INDEX));
    assert_eq!(
        paul.delete(&format!("/admin/policies/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn accepts_only_what_the_platform_can_enforce() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let cancelled = json!({ "op": "eq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "cancelled" } });

    // Resource conditions: only where each resource is checked.
    create(&paul, policy(&["intervention:read"], cancelled.clone())).await;
    create(&paul, policy(&["intervention:*"], cancelled.clone())).await;
    let holder_checked = paul
        .post("/admin/policies", policy(&["planning:apply"], cancelled))
        .await;
    assert_eq!(holder_checked.status, StatusCode::BAD_REQUEST);
    assert!(
        holder_checked
            .error_message()
            .contains("ressource par ressource")
    );

    // Not checked anywhere yet, or read by the administration screens.
    for permission in ["intervention:write", "org:read", "user:read"] {
        let refused = paul
            .post("/admin/policies", policy(&[permission], agents_only()))
            .await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{permission}");
    }

    let listing = paul.get("/admin/policies").await.json();
    let mut listed: Vec<(&str, bool)> = listing["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["permission"].as_str().unwrap(),
                p["perResource"].as_bool().unwrap(),
            )
        })
        .collect();
    listed.sort_unstable();
    // How each permission is checked, as the code checks it: changing a
    // classification changes what policies may say, so it must show here.
    assert_eq!(
        listed,
        [
            ("agent:use", false),
            ("approval:decide", true),
            ("connector:use", false),
            // Checked since documents exist (ADR-095).
            ("document:read", false),
            ("document:write", false),
            ("intervention:read", true),
            ("memory:read", false),
            ("memory:write", false),
            // Checked since missions exist (ADR-089).
            ("mission:create", false),
            ("mission:read", false),
            ("planning:apply", false),
            ("planning:propose", false),
            ("planning:read", false),
            // Checked since the agent proposes skills (ADR-100).
            ("skill:propose", false),
            ("skill:read", false),
        ]
    );
}
