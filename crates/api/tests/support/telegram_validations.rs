//! Validations given in Telegram, as the tests ask for them: a request
//! written in Telegram that pauses for its author's own validation, and the
//! approval it awaits.

use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::FakeTelegram;
use mimorya_core::demo::tenant;
use uuid::Uuid;

use super::TestApi;
use super::planning::{apply_proposal, propose};
use super::telegram_conversations::{talk, telegram_runs};

/// The request `writer` writes to plan INT-2052, ending with `end` once
/// validated: its run and its approval.
pub async fn asked(
    api: &TestApi,
    telegram: &FakeTelegram,
    (id, writer): (Uuid, (i64, &str)),
    end: &str,
) -> (String, Uuid) {
    api.script(vec![
        propose("INT-2052", "15:00"),
        apply_proposal(""),
        ScriptedStep::text(end),
    ]);
    talk(api, telegram, id, 10, writer, "Planifie INT-2052").await;
    let run = telegram_runs(api).await.remove(0).0;
    (run.to_string(), approval_of(api, run).await)
}

/// The approval the run `run` awaits.
pub async fn approval_of(api: &TestApi, run: Uuid) -> Uuid {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let approval = sqlx::query_scalar("select id from approvals where run_id = $1")
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    approval
}
