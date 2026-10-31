//! Conversations' summaries (ADR-145): once a conversation's messages not
//! covered by its summary grow past what is replayed word for word, the
//! oldest are folded into it by the company's model once a run ends; the
//! next turns start from that summary, handed over as data, then the latest
//! messages. A later pass folds the summary further, from the one it had.
//! What a pass costs is the run's; its text stays out of the audit. A failed
//! pass loses the summary, never the run, and a later one catches up. A
//! queued turn starts from the summary too.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::queue::{API, take_queued, worker};
use support::summaries::{
    SUMMARY, SUMMARY_SYSTEM, conversation, exchanges, given, last_system, stored, turn,
};
use uuid::Uuid;

fn answer(n: usize) -> ScriptedStep {
    ScriptedStep::text(format!("Réponse {n}"))
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn older_messages_are_folded_into_a_summary_the_next_turns_start_from() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let c = conversation(&marc).await;
    exchanges(c, 1, 8).await;

    // Sixteen messages are replayed as they are: nothing to fold.
    let summary = ScriptedStep::text(SUMMARY);
    let run = turn(&api, &marc, c, "Question 9", vec![answer(9), summary]).await;
    // Eighteen once answered: the oldest twelve are folded, the last three
    // exchanges kept.
    assert!(last_system(&api).starts_with(SUMMARY_SYSTEM));
    let reread = given(&api).join("\n");
    assert!(reread.contains("[Collaborateur]\nQuestion 1"), "{reread}");
    assert!(reread.contains("[Assistant]\nRéponse 6"), "{reread}");
    assert!(!reread.contains("Question 7"), "{reread}");
    assert!(!reread.contains("Résumé précédent"), "{reread}");
    let summary = stored(c).await;
    assert_eq!(summary, Some((SUMMARY.into(), "Réponse 6".into(), 12)));

    // Audited without its text; its cost counted in the run.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let audited: (Option<Uuid>, Option<Uuid>, Value) = sqlx::query_as(
        "select on_behalf_of_user_id, run_id, metadata from audit_log \
         where action = 'conversation.summarised' and resource_id = $1",
    )
    .bind(c.to_string())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        audited,
        (Some(user::MARC), Some(run), json!({ "foldedMessages": 12 }))
    );
    let calls: i64 =
        sqlx::query_scalar("select count(*) from usage_records where run_id = $1 and kind = 'llm'")
            .bind(run)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(calls, 2);
    drop(tx);

    // The next turn starts from the summary, as data, then the messages it
    // does not cover.
    turn(&api, &marc, c, "Question 10", vec![answer(10)]).await;
    let history = given(&api);
    assert_eq!(history.len(), 8, "{history:?}");
    assert!(history[0].starts_with("Résumé, établi par la plateforme"));
    assert!(history[0].contains("source=\"resume_conversation\" fiabilite=\"non_fiable\""));
    assert!(history[0].contains(SUMMARY));
    assert_eq!(
        history[1..7],
        [
            "Question 7",
            "Réponse 7",
            "Question 8",
            "Réponse 8",
            "Question 9",
            "Réponse 9"
        ]
    );
    assert!(history[7].contains("Question 10"));
    // Eight messages not covered: nothing folded, nothing asked.
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0);
    assert_eq!(stored(c).await.unwrap().2, 12);

    // Grown again, the summary is folded further from the one it had.
    exchanges(c, 11, 5).await;
    let further = ScriptedStep::text("Semaine 42 préparée ; INT-2049 et INT-2050 en cours.");
    turn(&api, &marc, c, "Question 16", vec![answer(16), further]).await;
    let reread = given(&api).join("\n");
    assert!(reread.contains("source=\"resume_precedent\""), "{reread}");
    assert!(reread.contains(SUMMARY), "{reread}");
    assert!(reread.contains("[Collaborateur]\nQuestion 7"), "{reread}");
    assert!(reread.contains("[Assistant]\nRéponse 13"), "{reread}");
    assert!(!reread.contains("Question 14"), "{reread}");
    let summary = stored(c).await.unwrap();
    assert_eq!(summary.1, "Réponse 13");
    assert_eq!(summary.2, 26);
    assert!(summary.0.starts_with("Semaine 42 préparée"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_failed_pass_loses_the_summary_never_the_run_and_a_later_one_catches_up() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let c = conversation(&marc).await;
    exchanges(c, 1, 8).await;

    // No answer scripted for the summary: the model fails it.
    let run = turn(&api, &marc, c, "Question 9", vec![answer(9)]).await;
    assert!(last_system(&api).starts_with(SUMMARY_SYSTEM));
    assert_eq!(stored(c).await, None);
    let shown = marc.get(&format!("/runs/{run}")).await;
    assert_eq!(shown.status, StatusCode::OK);
    assert_eq!(shown.json()["run"]["status"], "completed");

    // Without a summary, the latest messages as before; the next pass folds.
    let summary = ScriptedStep::text(SUMMARY);
    turn(&api, &marc, c, "Question 10", vec![answer(10), summary]).await;
    let requests = api.model.as_ref().unwrap().requests();
    let asked = &requests[requests.len() - 2];
    assert_eq!(asked.messages.len(), 19);
    assert_eq!(given(&api).len(), 1);
    assert_eq!(
        stored(c).await,
        Some((SUMMARY.into(), "Réponse 7".into(), 14))
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_queued_turn_starts_from_the_summary() {
    let api = TestApi::start_with(API).await;
    let worker = worker().await;
    let marc = api.sign_in(user::MARC).await;
    let c = conversation(&marc).await;
    exchanges(c, 1, 8).await;

    // Queued by the api process, executed and folded by the worker.
    turn(&api, &marc, c, "Question 9", vec![]).await;
    worker.script(vec![answer(9), ScriptedStep::text(SUMMARY)]);
    assert_eq!(take_queued(&worker).await, 1);
    assert_eq!(
        stored(c).await,
        Some((SUMMARY.into(), "Réponse 6".into(), 12))
    );

    turn(&api, &marc, c, "Question 10", vec![]).await;
    worker.script(vec![answer(10)]);
    assert_eq!(take_queued(&worker).await, 1);
    let history = given(&worker);
    assert_eq!(history.len(), 8, "{history:?}");
    assert!(history[0].contains(SUMMARY));
    assert_eq!(history[1], "Question 7");
    assert!(history[7].contains("Question 10"));
}
