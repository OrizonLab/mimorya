//! Who takes up the runs a stop of the service left running, when, and in
//! which company (ADR-053). The sweep of stuck runs leaves
//! them alone at startup, while expired reset links are purged at once. A run
//! interrupted for more than an hour ends instead of going on, even taken
//! up meanwhile. A read whose
//! tool its administrator set since to write is not repeated, and every end
//! names it. Each run is taken up in its own company, and only those still
//! running. A second worker starts beside the first (ADR-086), and each run
//! is taken up by one of them alone.

mod support;

use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::runs::{INTERRUPTED_TOO_LONG, STOPPED_BEFORE_ITS_FIRST_STEP};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::run_leases::mark_taken_up;
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::json;
use support::recovery::{
    events, interrupted_ago, restart, restore_demo_policies, saved, set_demo_policies,
    status_and_error, stopped_run,
};
use support::resets::new_reset_token;
use support::{TestApi, email, owner_url};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_second_worker_starts_beside_the_first_and_takes_each_run_up_once() {
    let first = TestApi::start().await;
    restart(&first).await;
    let run_id = stopped_run(&first, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    first.script(vec![ScriptedStep::text("Semaine calme.")]);

    // Both start at once (ADR-086): neither is refused, one takes it up.
    // The first's periodic takeover, under way since its own start, may
    // be the one: the starts then find it taken.
    let second = TestApi::start().await;
    second.script(vec![ScriptedStep::text("Semaine calme.")]);
    let (one, two) = tokio::join!(restart(&first), restart(&second));
    assert!(one + two <= 1, "taken up {one} + {two} times at start");
    assert_eq!(
        status_and_error(&first, tenant::ID, run_id).await.0,
        "completed"
    );
    // Taken up once, whoever took it.
    let resumed = events(&first, tenant::ID, run_id).await;
    let resumed = resumed.iter().filter(|(kind, _)| kind == "run_resumed");
    assert_eq!(resumed.count(), 1);
    let asked = |api: &TestApi| api.model.as_ref().unwrap().requests().len();
    assert_eq!(asked(&first) + asked(&second), 1);
    // No connection holds the former run executor's lock (ADR-053).
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let holders: i64 = sqlx::query_scalar(
        "select count(*) from pg_stat_activity \
         where application_name = 'mimorya-run-executor' and datname = current_database()",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(holders, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_sweep_at_startup_leaves_interrupted_runs_alone() {
    // A link expired, to see the first pass of maintenance run.
    let before = TestApi::start_with_mailer().await;
    new_reset_token(&before, &email(user::JULIEN)).await;
    let mut tx = before.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update password_resets set expires_at = now() where user_id = $1")
        .bind(user::JULIEN)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    // Interrupted twenty minutes ago: stale for the sweep.
    let run_id = stopped_run(&before, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    interrupted_ago(run_id, 20).await;
    // Stopped: only the new process's maintenance can purge the link.
    let TestApi { app, .. } = before;
    app.shutdown().await;

    let api = TestApi::start().await;
    let mut purged = false;
    for _ in 0..100 {
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        let left: i64 =
            sqlx::query_scalar("select count(*) from password_resets where user_id = $1")
                .bind(user::JULIEN)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        drop(tx);
        if left == 0 {
            purged = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(purged, "the first pass of maintenance did not run");
    assert_eq!(
        status_and_error(&api, tenant::ID, run_id).await.0,
        "running"
    );

    api.script(vec![ScriptedStep::text("Semaine calme.")]);
    assert_eq!(restart(&api).await, 1);
    assert_eq!(
        status_and_error(&api, tenant::ID, run_id).await.0,
        "completed"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_interrupted_more_than_an_hour_ago_ends_naming_what_was_under_way() {
    let api = TestApi::start().await;
    let idle = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    let writing = stopped_run(
        &api,
        tenant::ID,
        user::MARC,
        None,
        Some(saved()),
        &["interventions_search", "planning_apply"],
    )
    .await;
    interrupted_ago(idle, 61).await;
    interrupted_ago(writing, 61).await;
    // Taken up a moment ago by a worker that vanished before its first
    // step: the hour still counts from the last step (ADR-086).
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    mark_taken_up(&mut tx, idle).await.unwrap();
    tx.commit().await.unwrap();
    let calls_before = api.model.as_ref().unwrap().requests().len();

    restart(&api).await;

    let (status, error) = status_and_error(&api, tenant::ID, idle).await;
    assert_eq!(status, "failed");
    assert_eq!(error.as_deref(), Some(INTERRUPTED_TOO_LONG));
    // Never « relancez » while an action may have had an effect.
    let error = status_and_error(&api, tenant::ID, writing).await.1.unwrap();
    assert!(
        error.contains("Lors de l’arrêt du service, l’action « Application d’un planning » : son effet n’est pas connu. Vérifiez-le"),
        "{error}"
    );
    assert!(!error.contains("Recherche"), "{error}");
    assert_eq!(api.model.as_ref().unwrap().requests().len(), calls_before);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_read_whose_tool_was_set_since_to_write_is_named_not_repeated() {
    let api = TestApi::start().await;
    // Recorded as a read; its administrator has set the tool to write since.
    let set = json!({ "interventions_search": { "risk": "write" } });
    let before = set_demo_policies(&api, set).await;
    let search = ["interventions_search"];
    let recent = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &search).await;
    let old = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &search).await;
    interrupted_ago(old, 61).await;
    let calls_before = api.model.as_ref().unwrap().requests().len();

    restart(&api).await;
    restore_demo_policies(&api, before).await;

    // The engine ends one, the API the other: both name it.
    for run in [recent, old] {
        let (status, error) = status_and_error(&api, tenant::ID, run).await;
        assert_eq!(status, "failed");
        let error = error.unwrap();
        assert!(
            error.contains("l’action « Recherche d’interventions » : son effet n’est pas connu"),
            "{error}"
        );
    }
    assert_eq!(api.model.as_ref().unwrap().requests().len(), calls_before);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_company_s_runs_are_taken_up_in_their_own_company_and_only_running_ones() {
    let api = TestApi::start().await;
    let foreign = stopped_run(&api, OTHER_TENANT_ID, OTHER_USER_ID, None, None, &[]).await;
    let paused = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update agent_runs set status = 'awaiting_approval' where id = $1")
        .bind(paused)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(restart(&api).await, 1);

    let (status, error) = status_and_error(&api, OTHER_TENANT_ID, foreign).await;
    assert_eq!(status, "failed");
    assert_eq!(error.as_deref(), Some(STOPPED_BEFORE_ITS_FIRST_STEP));
    let ended = events(&api, OTHER_TENANT_ID, foreign).await;
    assert_eq!(ended.last().unwrap().0, "run_finished");
    // Its end is nowhere in the demo company.
    assert!(events(&api, tenant::ID, foreign).await.is_empty());
    assert_eq!(
        status_and_error(&api, tenant::ID, paused).await.0,
        "awaiting_approval"
    );
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update agent_runs set status = 'failed' where id = $1")
        .bind(paused)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}
