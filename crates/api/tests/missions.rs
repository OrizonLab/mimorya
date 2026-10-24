//! Missions (ADR-089): entrusted, a mission is carried out in stages, each a
//! run of its conversation, queued. A stage that spends its budget ends with
//! a progress note and queues the next one, which starts from the mission
//! and the notes, with the sources found so far; the final answer, reviewed
//! by the model against the criteria, ends it. Past its limits it ends
//! unfinished; a stage that fails ends it, failed.

mod support;

use mimorya_agent::runtime::{ACCOUNT_INACTIVE, MISSION_STAGE_BUDGET};
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use support::TestApi;
use support::missions::{
    CRITERION, OBJECTIVE, approving_review, cost_budget, entrust, executing, limit, messages, read,
    spend, spending_stage, stages, stop,
};
use support::queue::take_queued;

const NOTE: &str = "Fait : INT-2049 relue, toujours en cours. Reste : proposer son suivi.";
const ANSWER: &str = "Suivi proposé : INT-2049, à reprendre lundi.";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mission_goes_on_stage_after_stage_until_its_answer() {
    let api = TestApi::start().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    let id = entrust(&marc, "Suivi de la semaine").await;
    let mission = read(&marc, id).await;
    assert_eq!(mission["status"], "in_progress");
    assert_eq!(mission["objective"], OBJECTIVE);
    assert_eq!(mission["successCriteria"], serde_json::json!([CRITERION]));
    assert_eq!(
        (mission["stages"].clone(), mission["maxStages"].clone()),
        (1.into(), 6.into())
    );
    let first = stages(id).await;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].1, "queued");

    // Its budget spent, the first stage hands over: the next one is queued.
    api.script(spending_stage(NOTE));
    assert_eq!(take_queued(&api).await, 1);
    let both = stages(id).await;
    let statuses: Vec<&str> = both.iter().map(|(_, s)| s.as_str()).collect();
    assert_eq!(statuses, ["completed", "queued"]);
    let mission = read(&marc, id).await;
    assert_eq!(
        (mission["status"].clone(), mission["stages"].clone()),
        ("in_progress".into(), 2.into())
    );
    let parts: Vec<(String, String)> = messages(&marc, &mission)
        .await
        .iter()
        .map(|m| {
            let part = m["metadata"]["mission"].as_str().unwrap_or_default();
            (m["role"].as_str().unwrap().to_owned(), part.to_owned())
        })
        .collect();
    let expected = [
        ("user", "brief"),
        ("assistant", "progress"),
        ("user", "resumption"),
    ];
    assert_eq!(parts, expected.map(|(r, p)| (r.to_owned(), p.to_owned())));

    // The next stage starts from the mission and the note; the sources the
    // first one found ground its answer.
    api.script(vec![ScriptedStep::text(ANSWER), approving_review()]);
    assert_eq!(take_queued(&api).await, 1);
    let requests = api.model.as_ref().unwrap().requests();
    let resumed = format!("{:?}", requests[requests.len() - 2].messages);
    assert!(
        resumed.contains(OBJECTIVE) && resumed.contains(NOTE),
        "{resumed}"
    );
    let mission = read(&marc, id).await;
    assert_eq!(mission["status"], "done");
    assert_eq!(mission["result"], ANSWER);
    assert_eq!(mission["verification"]["passed"], true);
    assert_eq!(mission["verification"]["judged"], true);
    assert!(mission["finishedAt"].is_string());
    assert_eq!(take_queued(&api).await, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mission_past_its_limits_ends_unfinished() {
    let api = TestApi::start().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    for (max_stages, max_cost) in [(1, 12_000_000), (6, 0)] {
        let id = entrust(&marc, "Mission bornée").await;
        limit(id, max_stages, max_cost).await;
        api.script(spending_stage(NOTE));
        assert_eq!(take_queued(&api).await, 1);
        let mission = read(&marc, id).await;
        assert_eq!(mission["status"], "failed");
        let error = mission["error"].as_str().unwrap();
        assert!(error.contains("limites de la mission"), "{error}");
        assert_eq!(stages(id).await.len(), 1);
        // Its note stays its last word.
        let said = messages(&marc, &mission).await;
        assert_eq!(said.last().unwrap()["content"], NOTE);
    }
    assert_eq!(take_queued(&api).await, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_stage_spends_no_more_than_its_mission_has_left() {
    let api = TestApi::start().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    let id = entrust(&marc, "Mission économe").await;
    let first = stages(id).await[0].0;
    assert_eq!(
        cost_budget(first).await,
        MISSION_STAGE_BUDGET.max_cost_micros
    );
    // Less left than a stage's budget: the next stage gets what is left,
    // once what its stages spent (an earlier one's 0.40 USD) is counted.
    limit(id, 6, 1_000_000).await;
    spend(id, 400_000).await;
    api.script(spending_stage(NOTE));
    assert_eq!(take_queued(&api).await, 1);
    let next = stages(id).await.last().unwrap().0;
    assert_eq!(cost_budget(next).await, 600_000);
    assert_eq!(stop(&marc, id).await, axum::http::StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_stage_that_fails_ends_its_mission() {
    let api = TestApi::start().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;

    // Its user's account disabled before it runs.
    let id = entrust(&marc, "Mission d’un compte désactivé").await;
    as_owner("update users set status = 'disabled' where id = $1").await;
    api.app.services.access.invalidate(tenant::ID);
    assert_eq!(take_queued(&api).await, 1);
    as_owner("update users set status = 'active' where id = $1").await;
    api.app.services.access.invalidate(tenant::ID);
    let mission = read(&marc, id).await;
    assert_eq!(mission["status"], "failed");
    assert_eq!(mission["error"], ACCOUNT_INACTIVE);
    assert!(mission["finishedAt"].is_string());

    // Its executor crashing: the model has nothing to answer.
    let id = entrust(&marc, "Mission sans réponse").await;
    assert_eq!(take_queued(&api).await, 1);
    let mission = read(&marc, id).await;
    assert_eq!(mission["status"], "failed");
    assert_eq!(mission["error"], "Erreur interne");
    assert_eq!(stages(id).await[0].1, "failed");
}

/// Runs `sql` as the owner, on Marc's account.
async fn as_owner(sql: &str) {
    let owner = sqlx::PgPool::connect(support::owner_url()).await.unwrap();
    sqlx::query(sql)
        .bind(user::MARC)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}
