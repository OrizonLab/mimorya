//! A scheduled task a stop of the service made miss its occurrence
//! (ADR-080), for the end-to-end tests to read what became of it
//! (`scheduled-tasks.spec.ts`): Léa's evening round-up, due two hours
//! before the server starts. Its first pass, once the interrupted runs are
//! taken up, records it missed without asking the model anything, and runs
//! it next at its time the day after.

use chrono::{Duration, Utc};
use chrono_tz::Europe::Paris;
use mimorya_api::app::App;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::scheduled_tasks::{TaskDefinition, create_scheduled_task};

/// The task's title, which the test looks for.
const TITLE: &str = "Relevé du soir";

/// Leaves Léa's task due two hours ago, every day at that time in Paris.
pub async fn leave(app: &App) -> anyhow::Result<()> {
    let due = Utc::now() - Duration::hours(2);
    let time = due.with_timezone(&Paris).format("%H:%M").to_string();
    let mut tx = app.services.db.begin(tenant::ID).await?;
    let task = TaskDefinition {
        title: TITLE,
        instruction: "Relève les interventions de la journée restées sans compte rendu.",
        weekdays: &[1, 2, 3, 4, 5, 6, 7],
        time_of_day: &time,
        time_zone: "Europe/Paris",
        next_run_at: Some(due),
    };
    create_scheduled_task(&mut tx, tenant::ID, user::LEA, task).await?;
    tx.commit().await?;
    Ok(())
}
