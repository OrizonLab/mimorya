//! The purge applies each company's own retention durations (ADR-195): a
//! shortening once its week has passed, a lengthening at once, another
//! company's durations never. The application cannot skip the week nor the
//! bounds: it writes a duration and who chose it, the database dates the
//! change and keeps what applied before it. The tests start a process
//! without maintenance: they run the passes themselves.

mod support;

use mimorya_api::plugins::retention::{Retained, apply_retention};
use mimorya_core::demo::{tenant, user};
use mimorya_core::retention::RetentionKind;
use mimorya_db::Database;
use mimorya_db::repositories::retention_durations::{retention_durations, set_retention_duration};
use support::idle_conversations::{conversation, conversations_left};
use support::queue::API;
use support::retention::{
    account, clear_retention, disabled_days_ago, memory, owner, retention_changed_days_ago, titles,
};
use support::{TestApi, companies};
use uuid::Uuid;

async fn set(db: &Database, tenant_id: Uuid, kind: RetentionKind, duration: i32) {
    let mut tx = db.begin(tenant_id).await.unwrap();
    set_retention_duration(&mut tx, tenant_id, kind.key(), duration, user::PAUL)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

fn memories_lost(retained: &Retained, tenant_id: Uuid) -> Option<(i32, i32)> {
    (retained.memories.iter())
        .find(|lost| lost.tenant_id == tenant_id)
        .map(|lost| (lost.aged, lost.departed))
}

fn conversations_lost(retained: &Retained, tenant_id: Uuid) -> Option<i32> {
    (retained.conversations.iter())
        .find(|lost| lost.tenant_id == tenant_id)
        .map(|lost| lost.conversations)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_shortening_applies_to_memories_once_its_week_has_passed() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let neo = tenant::ID;
    let acme = companies::create(&api, "acme-durees", "admin@acme-durees.example").await;
    let gone = account(neo, "parti.durees@neo-maintenance.example", "disabled").await;
    let acme_gone = account(acme.tenant_id, "parti@acme-durees.example", "disabled").await;
    disabled_days_ago(gone, 10).await;
    disabled_days_ago(acme_gone, 10).await;
    memory(neo, "durée épisode Néo", "episode", None, 120, false).await;
    memory(neo, "durée partie Néo", "preference", Some(gone), 1, false).await;
    memory(
        acme.tenant_id,
        "durée épisode Acme",
        "episode",
        None,
        120,
        false,
    )
    .await;
    memory(
        acme.tenant_id,
        "durée partie Acme",
        "preference",
        Some(acme_gone),
        1,
        false,
    )
    .await;

    // Néo keeps its episodes 3 months and its departed memories 7 days:
    // for a week, the platform's still apply.
    set(db, neo, RetentionKind::Episodes, 3).await;
    set(db, neo, RetentionKind::Departed, 7).await;
    let retained = apply_retention(db).await.unwrap().unwrap();
    assert_eq!(memories_lost(&retained, neo), None);
    assert_eq!(titles("durée").await.len(), 4);

    // A week later, Néo's go; Acme's, under the platform's, stay.
    retention_changed_days_ago(neo, "episodes", 8).await;
    retention_changed_days_ago(neo, "departed", 8).await;
    let retained = apply_retention(db).await.unwrap().unwrap();
    assert_eq!(memories_lost(&retained, neo), Some((1, 1)));
    assert_eq!(memories_lost(&retained, acme.tenant_id), None);
    assert_eq!(
        titles("durée").await,
        ["durée partie Acme", "durée épisode Acme"]
    );
    clear_retention(neo).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn conversations_go_by_their_company_s_own_duration() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let neo = tenant::ID;
    let acme =
        companies::create(&api, "acme-conservation", "admin@acme-conservation.example").await;
    conversation(neo, user::MARC, "conservation Néo 30 mois", 30).await;
    conversation(neo, user::MARC, "conservation Néo 8 mois", 8).await;
    conversation(
        acme.tenant_id,
        acme.admin_id,
        "conservation Acme 30 mois",
        30,
    )
    .await;
    conversation(acme.tenant_id, acme.admin_id, "conservation Acme 8 mois", 8).await;

    // Néo keeps them 36 months, at once: its 30 months old one stays,
    // Acme's goes under the platform's 24.
    set(db, neo, RetentionKind::Conversations, 36).await;
    let retained = apply_retention(db).await.unwrap().unwrap();
    assert_eq!(conversations_lost(&retained, neo), None);
    assert_eq!(conversations_lost(&retained, acme.tenant_id), Some(1));

    // Shortened to 6 months: the longer still applies for a week, then the
    // shorter, to Néo's alone.
    set(db, neo, RetentionKind::Conversations, 6).await;
    let retained = apply_retention(db).await.unwrap().unwrap();
    assert_eq!(conversations_lost(&retained, neo), None);
    retention_changed_days_ago(neo, "conversations", 8).await;
    let retained = apply_retention(db).await.unwrap().unwrap();
    assert_eq!(conversations_lost(&retained, neo), Some(2));
    assert_eq!(
        conversations_left("conservation").await.0,
        ["conservation Acme 8 mois"]
    );
    clear_retention(neo).await;
}

/// The SQL state of a statement of the application in its company's
/// transaction.
async fn refused(db: &Database, statement: &str) -> String {
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let error = sqlx::query(statement).execute(&mut *tx).await.unwrap_err();
    error
        .as_database_error()
        .and_then(|error| error.code())
        .map(|code| code.to_string())
        .unwrap_or_default()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_application_skips_neither_the_week_nor_the_bounds() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let neo = tenant::ID;
    let acme = companies::create(
        &api,
        "acme-contournement",
        "admin@acme-contournement.example",
    )
    .await;
    set(db, neo, RetentionKind::Episodes, 6).await;

    // Nor the date of a change, nor what applied before it, nor removing
    // one: not the application's to write.
    const DENIED: &str = "42501";
    for statement in [
        "update tenant_retention_durations set changed_at = now() - interval '30 days'",
        "update tenant_retention_durations set before_duration = 6",
        "delete from tenant_retention_durations",
        "insert into tenant_retention_durations (tenant_id, kind, duration, before_duration) \
         values (current_setting('app.tenant_id')::uuid, 'departed', 7, 7)",
        "select * from retention_horizons()",
    ] {
        assert_eq!(refused(db, statement).await, DENIED, "{statement}");
    }
    // Nor a duration out of the bounds, nor another company's.
    let out = format!(
        "insert into tenant_retention_durations (tenant_id, kind, duration) \
         values ('{neo}', 'departed', 6)"
    );
    assert_eq!(refused(db, &out).await, "23514");
    let other = format!(
        "insert into tenant_retention_durations (tenant_id, kind, duration) \
         values ('{}', 'departed', 7)",
        acme.tenant_id
    );
    assert_eq!(refused(db, &other).await, DENIED);

    // A change the application writes waits its week, whatever it sets.
    let moved = "update tenant_retention_durations set duration = 1, kind = 'departed'";
    assert_eq!(refused(db, moved).await, DENIED);
    set(db, neo, RetentionKind::Episodes, 1).await;
    let mut tx = db.begin(neo).await.unwrap();
    let stored = retention_durations(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        (
            stored[0].kind.as_str(),
            stored[0].duration,
            stored[0].in_effect
        ),
        ("episodes", 1, 12)
    );
    let mut acme_tx = db.begin(acme.tenant_id).await.unwrap();
    assert!(retention_durations(&mut acme_tx).await.unwrap().is_empty());
    acme_tx.commit().await.unwrap();

    // The database's durations and bounds are the platform's rules.
    let owner = owner().await;
    for kind in RetentionKind::ALL {
        let (platform, below, low, high, above): (i32, bool, bool, bool, bool) = sqlx::query_as(
            "select retention_platform($1), retention_allowed($1, $2 - 1), \
                 retention_allowed($1, $2), retention_allowed($1, $3), \
                 retention_allowed($1, $3 + 1)",
        )
        .bind(kind.key())
        .bind(kind.min())
        .bind(kind.max())
        .fetch_one(&owner)
        .await
        .unwrap();
        assert_eq!(platform, kind.platform(), "{kind:?}");
        assert_eq!(
            (below, low, high, above),
            (false, true, true, false),
            "{kind:?}"
        );
    }
    owner.close().await;
    clear_retention(neo).await;
}
