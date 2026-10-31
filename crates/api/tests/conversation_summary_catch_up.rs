//! A summary caught up at a run's end (ADR-149): while the messages it does
//! not cover lag behind what one pass folds, passes are chained, each its
//! own model call and audited apart, at most a few per run's end; what is
//! left waits for the next run's end. Until then the conversation says how
//! many messages after the summary are neither summarised nor reread. A
//! failed pass stops the chain, the passes before it kept.

mod support;

use mimorya_agent::testing::ScriptedStep;
use mimorya_core::conversation_summary::MAX_PASSES;
use mimorya_core::demo::user;
use support::TestApi;
use support::summaries::{audited_passes, conversation, exchanges, stored, turn};

fn answer(n: usize) -> ScriptedStep {
    ScriptedStep::text(format!("Réponse {n}"))
}

/// The summary pass `n` writes.
fn summary(n: usize) -> String {
    format!("Résumé après la passe {n} : le chantier avance.")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_lagging_summary_is_caught_up_pass_after_pass_a_few_at_a_time() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let c = conversation(&marc).await;
    // A long conversation, never summarised: 220 messages.
    exchanges(c, 1, 110).await;

    // Its next run's end chains passes of 24 messages, eight at most.
    let mut steps = vec![answer(111)];
    steps.extend((1..=MAX_PASSES).map(|n| ScriptedStep::text(summary(n))));
    turn(&api, &marc, c, "Question 111", steps).await;
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0);
    let (text, through, folded) = stored(c).await.expect("a summary");
    assert_eq!(text, summary(MAX_PASSES));
    assert_eq!((through.as_str(), folded), ("Réponse 96", 192));
    assert_eq!(audited_passes(c).await, 8);
    // Thirty messages after it, of which the latest twenty are reread: the
    // conversation says ten are left out until the next run's end.
    let detail = marc.get(&format!("/conversations/{c}")).await.json();
    assert_eq!(detail["historySummary"]["foldedMessages"], 192);
    assert_eq!(detail["historySummary"]["notReplayed"], 10);

    // The next run's end catches up: one pass, nothing left out.
    let steps = vec![answer(112), ScriptedStep::text(summary(9))];
    turn(&api, &marc, c, "Question 112", steps).await;
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0);
    let (text, through, folded) = stored(c).await.expect("a summary");
    assert_eq!(text, summary(9));
    assert_eq!((through.as_str(), folded), ("Réponse 108", 216));
    assert_eq!(audited_passes(c).await, 9);
    let detail = marc.get(&format!("/conversations/{c}")).await.json();
    assert_eq!(detail["historySummary"]["notReplayed"], 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_failed_pass_stops_the_chain_keeping_the_passes_before() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let c = conversation(&marc).await;
    exchanges(c, 1, 40).await;

    // Three passes needed, one answered: the second fails, the chain stops.
    let steps = vec![answer(41), ScriptedStep::text(summary(1))];
    let run = turn(&api, &marc, c, "Question 41", steps).await;
    let (text, through, folded) = stored(c).await.expect("a summary");
    assert_eq!(text, summary(1));
    assert_eq!((through.as_str(), folded), ("Réponse 12", 24));
    assert_eq!(audited_passes(c).await, 1);
    // The run kept its answer.
    let shown = marc.get(&format!("/runs/{run}")).await.json();
    assert_eq!(shown["run"]["status"], "completed");
    // 58 messages after the summary, 20 reread.
    let detail = marc.get(&format!("/conversations/{c}")).await.json();
    assert_eq!(detail["historySummary"]["notReplayed"], 38);
}
