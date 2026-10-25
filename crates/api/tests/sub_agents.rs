//! Sub-agents (ADR-098): a run hands a task to a child run, recorded beside
//! it for the same user, read-only, given the tools that only read, whose
//! sources the run's answer cites. A child is listed with its run, never on
//! its own, and only its user sees it. It is never taken up on its own: its
//! run taken up, it fails, as it does when its run ends; no company opens
//! one under another's run.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call, last_tool_results};
use mimorya_api::services::runs::SUB_AGENT_INTERRUPTED;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::run_leases::expired_run_leases;
use mimorya_db::repositories::runs::{RunPatch, RunStatus, update_active_run};
use mimorya_db::repositories::sub_runs::{NewSubRun, create_sub_run};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use support::TestApi;
use support::document_search::{sources, titled};
use support::recovery::{conversation_of, events, restart, saved, status_and_error, stopped_run};
use uuid::Uuid;

const CHARTER: &str = "Charte qualité des interventions";
const TASK: &str = "Trouver le délai de prise en charge d’une panne de chauffage";

/// A child of `parent`, in `tenant_id`, as its run would open it.
async fn sub_run_of(api: &TestApi, tenant_id: Uuid, parent: Uuid) -> Option<Uuid> {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let row = create_sub_run(
        &mut tx,
        NewSubRun {
            parent_run_id: parent,
            profile: "research",
            objective: TASK.into(),
            budget: json!({}),
            allowed_permissions: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    row.map(|r| r.id)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_sub_agent_reads_for_its_run_whose_answer_cites_what_it_read() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let charter = titled(&lucas, CHARTER).await;
    api.script(vec![
        ScriptedStep::calls(
            "Je confie la recherche.",
            vec![call(
                "delegate_task",
                json!({ "profile": "research", "task": TASK }),
            )],
        ),
        ScriptedStep::dynamic(|req| {
            assert!(req.system.contains("## Rôle : sous-agent « Recherche »"));
            let tools = req.tool_names();
            assert!(tools.contains(&"document_search".to_string()), "{tools:?}");
            for absent in [
                "delegate_task",
                "memory_remember",
                "planning_propose",
                "planning_apply",
            ] {
                assert!(!tools.contains(&absent.to_string()), "{tools:?}");
            }
            let search = call("document_search", json!({ "query": "panne de chauffage" }));
            ScriptedReply::calls("", vec![search])
        }),
        ScriptedStep::text("Charte qualité : prise en charge le jour même avant 12 h."),
        ScriptedStep::dynamic(|req| {
            let results = last_tool_results(req);
            assert!(results[0].content.contains("\"resume\""), "{results:?}");
            ScriptedReply::text("D’après la charte, une panne signalée avant 12 h est prise en charge le jour même.")
        }),
    ]);
    let conversation = conversation_of(&lucas).await;
    let sent = lucas
        .post(
            &format!("/conversations/{conversation}/messages"),
            json!({ "content": "Sous quel délai une panne de chauffage est-elle prise en charge ?" }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    let parent: Uuid = sent.json()["runId"].as_str().unwrap().parse().unwrap();
    api.idle().await;

    // The charter the sub-agent read is the answer's source.
    let cited = json!({ "type": "document", "id": charter.to_string(), "label": CHARTER });
    let shown = sources(&lucas, &conversation.to_string()).await;
    assert!(shown.contains(&cited), "{shown:?}");

    let detail = lucas.get(&format!("/runs/{parent}")).await.json();
    assert_eq!(detail["run"]["status"], "completed", "{detail}");
    assert_eq!(detail["run"]["agentProfile"], "main");
    assert_eq!(detail["run"]["parentRunId"], Value::Null);
    let delegated = detail["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["toolName"] == "delegate_task")
        .expect("the delegation's record");
    assert_eq!(delegated["status"], "succeeded");
    assert_eq!(
        delegated["outputSummary"],
        "Sous-agent « Recherche » : résumé rendu"
    );
    let children = detail["subRuns"].as_array().unwrap();
    assert_eq!(children.len(), 1, "{detail}");
    assert_eq!(children[0]["agentProfile"], "research");
    assert_eq!(children[0]["objective"], TASK);
    assert_eq!(children[0]["status"], "completed");
    let child = children[0]["id"].as_str().unwrap().to_string();

    let shown = lucas.get(&format!("/runs/{child}")).await.json();
    assert_eq!(shown["run"]["parentRunId"], json!(parent));
    assert_eq!(shown["run"]["conversationId"], Value::Null);
    assert_eq!(shown["run"]["autonomyLevel"], "read_only");
    let read: Vec<_> = shown["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| (t["toolName"].clone(), t["status"].clone()))
        .collect();
    assert_eq!(read, vec![(json!("document_search"), json!("succeeded"))]);
    assert!(shown["subRuns"].as_array().unwrap().is_empty());

    // Listed with its run only, and seen by its user only.
    let listed = lucas.get("/runs").await.json();
    let runs = listed["runs"].as_array().unwrap();
    assert!(runs.iter().any(|r| r["id"] == json!(parent)));
    assert!(runs.iter().all(|r| r["agentProfile"] == "main"), "{listed}");
    let julien = api.sign_in(user::JULIEN).await;
    let hidden = julien.get(&format!("/runs/{child}")).await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_sub_agent_is_never_taken_up_alone_and_fails_when_its_run_is() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let conversation = conversation_of(&marc).await;
    let parent = stopped_run(
        &api,
        tenant::ID,
        user::MARC,
        Some(conversation),
        Some(saved()),
        &["delegate_task"],
    )
    .await;
    let child = sub_run_of(&api, tenant::ID, parent).await.expect("opened");
    // Neither has a lease; only the run is to be taken up.
    let expired = expired_run_leases(api.app.services.db.pool())
        .await
        .unwrap();
    assert!(expired.iter().any(|r| r.run_id == parent));
    assert!(!expired.iter().any(|r| r.run_id == child));
    api.script(vec![ScriptedStep::text("Semaine calme : rien à signaler.")]);

    // The run only: its sub-agent went with its execution.
    assert_eq!(restart(&api).await, 1);
    assert_eq!(
        status_and_error(&api, tenant::ID, parent).await.0,
        "completed"
    );
    assert_eq!(
        status_and_error(&api, tenant::ID, child).await,
        (
            "failed".to_string(),
            Some(SUB_AGENT_INTERRUPTED.to_string())
        )
    );
    let announced = events(&api, tenant::ID, child).await;
    let (kind, data) = announced.last().expect("its end announced");
    assert_eq!(kind, "run_finished");
    assert_eq!(data["status"], "failed");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_that_ends_ends_its_sub_agents_still_running() {
    let api = TestApi::start().await;
    let parent = stopped_run(&api, tenant::ID, user::MARC, None, None, &[]).await;
    let done = sub_run_of(&api, tenant::ID, parent).await.unwrap();
    let running = sub_run_of(&api, tenant::ID, parent).await.unwrap();
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let completed = RunPatch {
        status: Some(RunStatus::Completed),
        ..RunPatch::default()
    };
    assert!(update_active_run(&mut tx, done, completed).await.unwrap());
    let cancelled = RunPatch {
        status: Some(RunStatus::Cancelled),
        ..RunPatch::default()
    };
    assert!(update_active_run(&mut tx, parent, cancelled).await.unwrap());
    tx.commit().await.unwrap();

    assert_eq!(
        status_and_error(&api, tenant::ID, done).await,
        ("completed".to_string(), None)
    );
    let (status, error) = status_and_error(&api, tenant::ID, running).await;
    assert_eq!(status, "failed");
    assert_eq!(
        error.as_deref(),
        Some("Sous-agent interrompu : la demande qui l’avait lancé s’est arrêtée.")
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn no_company_opens_a_sub_agent_under_another_s_run() {
    let api = TestApi::start().await;
    let theirs = stopped_run(&api, OTHER_TENANT_ID, OTHER_USER_ID, None, None, &[]).await;
    assert_eq!(sub_run_of(&api, tenant::ID, theirs).await, None);
    assert!(sub_run_of(&api, OTHER_TENANT_ID, theirs).await.is_some());

    // Ended, with its sub-agent: no later restart takes it up.
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    let cancelled = RunPatch {
        status: Some(RunStatus::Cancelled),
        ..RunPatch::default()
    };
    assert!(update_active_run(&mut tx, theirs, cancelled).await.unwrap());
    tx.commit().await.unwrap();
}
