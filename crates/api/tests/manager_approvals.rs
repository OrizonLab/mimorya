//! Validation by a manager (ADR-059). The risks a company names go to the
//! nearest lead above whom the agent acts for, on an active account, who
//! holds `approval:decide` over them: they alone approve, while they still
//! hold it, and the run goes on with the rights of whom it acts for, who may
//! withdraw the request while they may use the agent. Nobody eligible, the action is refused at
//! once. Only those who manage access policies set the risks.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::manager_approvals::{ROUTING, applying, audited, listed, route_to_managers, without};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_requester_s_manager_alone_decides_and_the_run_goes_on_for_the_requester() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let marc = api.sign_in(user::MARC).await;
    let julien = api.sign_in(user::JULIEN).await;
    route_to_managers(&paul, &["write"]).await;

    let (run_id, approval) = applying(
        &api,
        &nadia,
        ("INT-2049", "13:00"),
        "Le planning de INT-2049 est appliqué.",
    )
    .await;
    let approval = approval.expect("a request awaiting her manager");
    // Nadia leads her team: the lead above, Marc, validates.
    assert_eq!(approval["approver"]["id"], json!(user::MARC));
    assert_eq!(approval["requestedFor"]["id"], json!(user::NADIA));
    assert_eq!(approval["canDecide"], false);
    assert_eq!(approval["canWithdraw"], true);
    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    let approve = json!({ "approved": true });

    // Neither whom the agent acts for, nor another team lead, decides it.
    let mine = nadia.post(&decision, approve.clone()).await;
    assert_eq!(mine.status, StatusCode::FORBIDDEN, "{}", mine.text());
    assert!(mine.text().contains("votre responsable"), "{}", mine.text());
    for approved in [true, false] {
        let other_lead = julien
            .post(&decision, json!({ "approved": approved }))
            .await;
        assert_eq!(other_lead.status, StatusCode::NOT_FOUND, "{approved}");
    }
    assert!(listed(&julien, &run_id).await.is_none());

    // Marc sees it to decide, for Nadia.
    let to_decide = listed(&marc, &run_id).await.expect("listed to her manager");
    assert_eq!(to_decide["canDecide"], true);
    assert_eq!(to_decide["canWithdraw"], false);
    assert_eq!(to_decide["requestedFor"]["id"], json!(user::NADIA));
    let decided = marc.post(&decision, approve).await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    assert_eq!(
        decided.json()["approval"]["decidedBy"]["id"],
        json!(user::MARC)
    );
    api.idle().await;
    route_to_managers(&paul, &[]).await;

    // Decided by Marc on Nadia's behalf; the planning applied as Nadia.
    assert_eq!(
        audited(&api, &run_id, "approval.approved").await,
        [(Some(user::MARC), Some(user::NADIA))]
    );
    let applied = audited(&api, &run_id, "agent.tool.execute").await;
    assert!(!applied.is_empty());
    assert!(
        applied
            .iter()
            .all(|(_, on_behalf)| *on_behalf == Some(user::NADIA)),
        "{applied:?}"
    );
    let detail = nadia.get(&format!("/runs/{run_id}")).await.json();
    assert_eq!(detail["run"]["status"], "completed", "{detail}");
    // Decided, nobody withdraws it.
    let mine = listed(&nadia, &run_id).await.unwrap();
    assert_eq!(mine["canWithdraw"], false);

    // Her own to decide again: refused, not withdrawn.
    let (run_id, approval) = applying(
        &api,
        &nadia,
        ("INT-2052", "09:00"),
        "Le planning de INT-2052 est inchangé.",
    )
    .await;
    let approval = approval.expect("a request she decides");
    assert_eq!(approval["canDecide"], true);
    assert_eq!(approval["canWithdraw"], false);
    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    let refused = nadia.post(&decision, json!({ "approved": false })).await;
    assert_eq!(refused.status, StatusCode::OK, "{}", refused.text());
    api.idle().await;
    assert_eq!(listed(&nadia, &run_id).await.unwrap()["status"], "rejected");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_manager_who_no_longer_holds_the_right_cannot_decide() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let marc = api.sign_in(user::MARC).await;
    route_to_managers(&paul, &["write"]).await;
    let (run_id, approval) = applying(
        &api,
        &nadia,
        ("INT-2052", "13:00"),
        "Le planning de INT-2052 est appliqué.",
    )
    .await;
    route_to_managers(&paul, &[]).await;
    let approval = approval.expect("a request awaiting her manager");
    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());

    // Named when the request was made, Marc loses the right meanwhile: he
    // no longer sees it, nor decides it.
    let policy = without(&paul, user::MARC, "approval:decide").await;
    assert!(listed(&marc, &run_id).await.is_none());
    let refused = marc.post(&decision, json!({ "approved": true })).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    assert_eq!(listed(&nadia, &run_id).await.unwrap()["status"], "pending");

    // Given back, he decides.
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);
    let rejected = marc.post(&decision, json!({ "approved": false })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    api.idle().await;
    assert_eq!(listed(&nadia, &run_id).await.unwrap()["status"], "rejected");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn whom_the_agent_acts_for_withdraws_a_request_nobody_can_decide_any_more() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    route_to_managers(&paul, &["write"]).await;
    let (run_id, approval) = applying(
        &api,
        &nadia,
        ("INT-2052", "10:00"),
        "Le planning de INT-2052 est inchangé.",
    )
    .await;
    route_to_managers(&paul, &[]).await;
    let approval = approval.expect("a request awaiting her manager");
    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    let policy = without(&paul, user::MARC, "approval:decide").await;

    // Withdrawing resumes the agent: not without it.
    let no_agent = without(&paul, user::NADIA, "agent:use").await;
    assert_eq!(listed(&nadia, &run_id).await.unwrap()["canWithdraw"], false);
    let refused = nadia.post(&decision, json!({ "approved": false })).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    assert_eq!(paul.delete(&no_agent).await.status, StatusCode::OK);

    // Still not hers to approve, but hers to withdraw: the run goes on
    // without the action.
    let approved = nadia.post(&decision, json!({ "approved": true })).await;
    assert_eq!(
        approved.status,
        StatusCode::FORBIDDEN,
        "{}",
        approved.text()
    );
    let withdrawn = nadia.post(&decision, json!({ "approved": false })).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    api.idle().await;
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);
    let mine = listed(&nadia, &run_id).await.unwrap();
    assert_eq!(mine["status"], "rejected");
    assert_eq!(mine["decidedBy"]["id"], json!(user::NADIA));
    let detail = nadia.get(&format!("/runs/{run_id}")).await.json();
    assert_eq!(detail["run"]["status"], "completed", "{detail}");

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let audit: Vec<Value> = sqlx::query_scalar(
        "select metadata from audit_log where run_id = $1::uuid and action = 'approval.rejected'",
    )
    .bind(&run_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0]["withdrawn"], true, "{}", audit[0]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_lead_is_passed_over() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    route_to_managers(&paul, &["write"]).await;
    let marc = format!("/admin/users/{}", user::MARC);
    let disabled = paul.patch(&marc, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    // Marc passed over, only the director remains, who may not validate.
    let (run_id, approval) = applying(
        &api,
        &nadia,
        ("INT-2052", "15:00"),
        "Personne ne peut valider ce planning.",
    )
    .await;
    let enabled = paul.patch(&marc, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    route_to_managers(&paul, &[]).await;
    assert!(approval.is_none());
    assert_eq!(
        audited(&api, &run_id, "agent.approval.unroutable")
            .await
            .len(),
        1
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_nobody_may_validate_is_refused_at_once() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    route_to_managers(&paul, &["write"]).await;
    // Above Marc, only the director, who may not validate.
    let (run_id, approval) = applying(
        &api,
        &marc,
        ("INT-2049", "15:00"),
        "Personne ne peut valider ce planning.",
    )
    .await;
    route_to_managers(&paul, &[]).await;
    assert!(approval.is_none());
    let detail = marc.get(&format!("/runs/{run_id}")).await.json();
    assert_eq!(detail["run"]["status"], "completed", "{detail}");
    let refused = detail["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["toolName"] == "planning_apply")
        .cloned()
        .unwrap();
    assert_eq!(refused["status"], "denied", "{refused}");
    assert_eq!(
        audited(&api, &run_id, "agent.approval.unroutable")
            .await
            .len(),
        1
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_those_who_manage_access_policies_set_the_risks() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let shown = paul.get(ROUTING).await.json();
    assert_eq!(shown["managerRisks"], json!([]));
    // No mail relay here: nobody is e-mailed (ADR-060).
    assert_eq!(shown["notifiesByEmail"], false);
    let refused = marc
        .put(ROUTING, json!({ "managerRisks": ["write"] }))
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(marc.get(ROUTING).await.status, StatusCode::FORBIDDEN);

    // Once each, from the lightest to the heaviest; audited with the former.
    route_to_managers(&paul, &["irreversible", "write", "write"]).await;
    assert_eq!(
        paul.get(ROUTING).await.json()["managerRisks"],
        json!(["write", "irreversible"])
    );
    route_to_managers(&paul, &[]).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let changes: Vec<Value> = sqlx::query_scalar(
        "select metadata from audit_log where action = 'approval_routing.updated' \
         order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let last = changes.last().unwrap();
    assert_eq!(last["managerRisks"], json!([]));
    assert_eq!(
        last["previousManagerRisks"],
        json!(["write", "irreversible"])
    );
}
