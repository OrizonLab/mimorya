//! A run reads its user's rights again before each model call and before the
//! tool calls it returns (ADR-035): an account disabled, or deprived of the
//! right to use the agent, or whose company is suspended, while the model
//! answers ends the run before any of those calls runs; business roles
//! withdrawn meanwhile no longer count for them.

mod support;

use mimorya_agent::runtime::ACCOUNT_INACTIVE;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call};
use mimorya_api::services::runs::{AGENT_USE_WITHDRAWN, COMPANY_SUSPENDED};
use mimorya_core::demo::{tenant, user};
use support::runs::start_run;
use support::{TestApi, owner_url, week};
use uuid::Uuid;

/// Runs `sql` (`$1`: the account) with the owner role, blocking: callable
/// from inside the scripted model's answer, as an administrator acting while
/// the model works.
fn as_owner(sql: &'static str, account: Uuid) {
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(async {
                let owner = sqlx::PgPool::connect(owner_url()).await.expect("owner");
                sqlx::query(sql)
                    .bind(account)
                    .execute(&owner)
                    .await
                    .expect("change");
            });
    })
    .join()
    .expect("change thread");
}

/// The model searches the interventions of the week once `sql` has changed
/// the account, then would conclude.
fn script_change_then_search(api: &TestApi, sql: &'static str, account: Uuid) {
    api.script(vec![
        ScriptedStep::dynamic(move |_| {
            as_owner(sql, account);
            ScriptedReply::calls("Je cherche.", vec![call("interventions_search", week())])
        }),
        ScriptedStep::text("Synthèse."),
    ]);
}

/// Status and error of the run, and its tool invocations (status, reason).
async fn outcome(run_id: Uuid) -> (String, Option<String>, Vec<(String, Option<String>)>) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let (status, error): (String, Option<String>) =
        sqlx::query_as("select status, error from agent_runs where id = $1")
            .bind(run_id)
            .fetch_one(&owner)
            .await
            .unwrap();
    let calls = sqlx::query_as(
        "select status, decision_reason from tool_invocations where run_id = $1 order by created_at",
    )
    .bind(run_id)
    .fetch_all(&owner)
    .await
    .unwrap();
    (status, error, calls)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_disabled_while_the_model_answers_runs_none_of_its_calls() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    script_change_then_search(
        &api,
        "update users set status = 'disabled' where id = $1",
        user::MARC,
    );

    let run_id = start_run(&marc).await;
    api.idle().await;

    let (status, error, calls) = outcome(run_id).await;
    assert_eq!(status, "failed");
    assert_eq!(error.as_deref(), Some(ACCOUNT_INACTIVE));
    assert!(calls.is_empty(), "{calls:?}");
    // The run stopped there: the model was not called again.
    assert_eq!(api.model.as_ref().unwrap().remaining(), 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_right_to_use_the_agent_withdrawn_while_the_model_answers_ends_the_run() {
    let api = TestApi::start().await;
    let isabelle = api.sign_in(user::ISABELLE).await;
    // Only the base role carries agent:use; her business role stays.
    script_change_then_search(
        &api,
        "delete from role_assignments where user_id = $1 \
         and role_id in (select id from roles where key = 'member')",
        user::ISABELLE,
    );

    let run_id = start_run(&isabelle).await;
    api.idle().await;

    let (status, error, calls) = outcome(run_id).await;
    assert_eq!(status, "failed");
    assert_eq!(error.as_deref(), Some(AGENT_USE_WITHDRAWN));
    assert!(calls.is_empty(), "{calls:?}");
    assert_eq!(api.model.as_ref().unwrap().remaining(), 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn business_roles_withdrawn_while_the_model_answers_no_longer_count() {
    let api = TestApi::start().await;
    let julien = api.sign_in(user::JULIEN).await;
    // His business roles go (team lead, technician); the base role
    // (agent:use) stays.
    script_change_then_search(
        &api,
        "delete from role_assignments where user_id = $1 \
         and role_id in (select id from roles where key <> 'member')",
        user::JULIEN,
    );

    let run_id = start_run(&julien).await;
    api.idle().await;

    let (status, _, calls) = outcome(run_id).await;
    assert_eq!(status, "completed");
    assert_eq!(
        calls,
        [(
            "denied".to_string(),
            Some("Permission intervention:read requise".to_string())
        )]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_suspended_while_the_model_answers_ends_the_run() {
    const SUSPEND: &str = "update tenants set status = 'suspended' where id = (select tenant_id from users where id = $1)";
    let api = TestApi::start().await;
    let claire = api.sign_in(user::CLAIRE).await;
    let access = api.app.services.access.clone();
    // The demo company is active again for the next tests, even on failure.
    let _resumed = ResumeOnDrop(api.app.services.access.clone());
    api.script(vec![
        ScriptedStep::dynamic(move |_| {
            as_owner(SUSPEND, user::CLAIRE);
            // As when the company's cached rules expire.
            access.invalidate(tenant::ID);
            ScriptedReply::calls("Je cherche.", vec![call("interventions_search", week())])
        }),
        ScriptedStep::text("Synthèse."),
    ]);

    let run_id = start_run(&claire).await;
    api.idle().await;

    let (status, error, calls) = outcome(run_id).await;
    assert_eq!(status, "failed");
    assert_eq!(error.as_deref(), Some(COMPANY_SUSPENDED));
    assert!(calls.is_empty(), "{calls:?}");
    assert_eq!(api.model.as_ref().unwrap().remaining(), 1);
}

/// Reactivates the demo company when dropped, the access cache with it.
struct ResumeOnDrop(std::sync::Arc<mimorya_api::services::AccessService>);

impl Drop for ResumeOnDrop {
    fn drop(&mut self) {
        as_owner(
            "update tenants set status = 'active' where id = (select tenant_id from users where id = $1)",
            user::CLAIRE,
        );
        self.0.invalidate(tenant::ID);
    }
}
