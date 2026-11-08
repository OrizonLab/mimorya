//! Retention applied (DATA_MODEL §5, ADR-185). The database dates an
//! account's disabling itself, whatever disables it, and forgets the date
//! when it is enabled again. A pass deletes the sessions ended thirty days
//! ago, the episodes recorded a year ago and the memories past their own
//! term, the personal memories of an account disabled thirty days ago; what
//! is more recent stays. Each company that lost memories finds it in its own
//! audit log, by the platform, counted, never read. A pass runs in one
//! process at a time.

mod support;

use mimorya_api::plugins::retention::{RETENTION_SWEEP, apply_retention};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::audit_exports::audit_log;
use support::{TestApi, companies, owner_url};
use uuid::Uuid;

async fn owner() -> PgPool {
    PgPool::connect(owner_url()).await.unwrap()
}

/// An account of the company `tenant_id` at `email`, `status`, as an
/// operator writes it.
async fn account(tenant_id: Uuid, email: &str, status: &str) -> Uuid {
    let owner = owner().await;
    let id = sqlx::query_scalar(
        "insert into users (tenant_id, email, display_name, status) \
         values ($1, $2, 'Compte de la rétention', $3) returning id",
    )
    .bind(tenant_id)
    .bind(email)
    .bind(status)
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    id
}

/// When the database says the account `id` was disabled.
async fn disabled_at(id: Uuid) -> Option<chrono::DateTime<chrono::Utc>> {
    let owner = owner().await;
    let at = sqlx::query_scalar("select disabled_at from users where id = $1")
        .bind(id)
        .fetch_one(&owner)
        .await
        .unwrap();
    owner.close().await;
    at
}

/// Moves the disabling of the account `id` `days` back, past the rule that
/// keeps it the database's: what time does.
async fn disabled_days_ago(id: Uuid, days: i32) {
    let owner = owner().await;
    let mut tx = owner.begin().await.unwrap();
    sqlx::query("alter table users disable trigger users_disabled_at")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("update users set disabled_at = now() - make_interval(days => $2) where id = $1")
        .bind(id)
        .bind(days)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("alter table users enable trigger users_disabled_at")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    owner.close().await;
}

/// A memory of the company `tenant_id` titled `title`: of `kind`, personal
/// to `owner_user_id` when it names one, recorded `days` ago, past its own
/// term when `expired`.
async fn memory(
    tenant_id: Uuid,
    title: &str,
    kind: &str,
    owner_user_id: Option<Uuid>,
    days: i32,
    expired: bool,
) {
    let owner = owner().await;
    sqlx::query(
        "insert into memory_items (tenant_id, scope, owner_user_id, kind, title, content, \
         source, created_at, expires_at) \
         values ($1, case when $2::uuid is null then 'tenant' else 'user' end, $2, $3, $4, \
         'contenu', '{}', now() - make_interval(days => $5), \
         case when $6 then now() - interval '1 minute' end)",
    )
    .bind(tenant_id)
    .bind(owner_user_id)
    .bind(kind)
    .bind(title)
    .bind(days)
    .bind(expired)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

/// The titles of the memories starting with `prefix`, sorted.
async fn titles(prefix: &str) -> Vec<String> {
    let owner = owner().await;
    let mut titles: Vec<String> =
        sqlx::query_scalar("select title from memory_items where title like $1 || '%'")
            .bind(prefix)
            .fetch_all(&owner)
            .await
            .unwrap();
    owner.close().await;
    titles.sort();
    titles
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_database_dates_an_account_s_disabling() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let created = paul
        .post(
            "/admin/users",
            json!({ "email": "date.retention@neo-maintenance.example", "displayName": "Date" }),
        )
        .await;
    assert_eq!(created.status.as_u16(), 201, "{}", created.text());
    let id: Uuid = created.json()["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(disabled_at(id).await, None);

    // Disabled by the administration: dated; disabled again: the same date.
    let path = format!("/admin/users/{id}");
    let disabled = paul.patch(&path, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status.as_u16(), 200, "{}", disabled.text());
    let first = disabled_at(id).await.expect("dated");
    paul.patch(&path, json!({ "status": "disabled" })).await;
    assert_eq!(disabled_at(id).await, Some(first));

    // A statement cannot set it; enabled again, it is forgotten.
    let owner = owner().await;
    sqlx::query("update users set disabled_at = now() - interval '90 days' where id = $1")
        .bind(id)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    assert_eq!(disabled_at(id).await, Some(first));
    paul.patch(&path, json!({ "status": "active" })).await;
    assert_eq!(disabled_at(id).await, None);

    // Made disabled (by SCIM, by an operator): dated from its making.
    let made = account(
        tenant::ID,
        "inactif.retention@neo-maintenance.example",
        "disabled",
    )
    .await;
    assert!(disabled_at(made).await.is_some());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn ended_sessions_are_forgotten_thirty_days_on() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let owner = owner().await;
    // Each session's (revoked, idle term, absolute term), in days from now.
    let sessions = [
        ("revoquee-31", Some(-31), 1, 30),
        ("inactive-31", None, -31, 30),
        ("absolue-31", None, 1, -31),
        ("revoquee-29", Some(-29), 1, 30),
        ("inactive-29", None, -29, 30),
    ];
    for (agent, revoked, expires, absolute) in sessions {
        sqlx::query(
            "insert into sessions (tenant_id, user_id, token_hash, expires_at, \
             absolute_expires_at, revoked_at, user_agent) \
             values ($1, $2, md5($3), now() + make_interval(days => $4), \
             now() + make_interval(days => $5), \
             now() + make_interval(days => $6), $3)",
        )
        .bind(tenant::ID)
        .bind(user::PAUL)
        .bind(format!("retention-{agent}"))
        .bind(expires)
        .bind(absolute)
        .bind(revoked)
        .execute(&owner)
        .await
        .unwrap();
    }

    let retained = apply_retention(&api.app.services.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retained.sessions, 3);
    let left: Vec<String> = sqlx::query_scalar(
        "select user_agent from sessions where user_agent like 'retention-%' order by 1",
    )
    .fetch_all(&owner)
    .await
    .unwrap();
    owner.close().await;
    assert_eq!(left, ["retention-inactive-29", "retention-revoquee-29"]);
    // The session in use is not touched.
    assert_eq!(paul.get("/auth/me").await.status.as_u16(), 200);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn memories_past_their_retention_go_each_company_told() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let paul = api.sign_in(user::PAUL).await;
    let neo = tenant::ID;
    let gone = account(neo, "parti.retention@neo-maintenance.example", "disabled").await;
    let leaving = account(neo, "partant.retention@neo-maintenance.example", "disabled").await;
    let back = account(neo, "revenu.retention@neo-maintenance.example", "disabled").await;
    disabled_days_ago(gone, 31).await;
    disabled_days_ago(leaving, 29).await;
    let owner = owner().await;
    sqlx::query("update users set status = 'active' where id = $1")
        .bind(back)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;

    memory(
        neo,
        "rétention épisode 13 mois",
        "episode",
        None,
        396,
        false,
    )
    .await;
    memory(
        neo,
        "rétention épisode 11 mois",
        "episode",
        None,
        335,
        false,
    )
    .await;
    memory(neo, "rétention fait expiré", "fact", None, 1, true).await;
    memory(neo, "rétention fait ancien", "fact", None, 900, false).await;
    memory(neo, "rétention parti", "preference", Some(gone), 1, false).await;
    memory(
        neo,
        "rétention partant",
        "preference",
        Some(leaving),
        1,
        false,
    )
    .await;
    memory(neo, "rétention revenu", "preference", Some(back), 1, false).await;
    memory(
        neo,
        "rétention Paul",
        "preference",
        Some(user::PAUL),
        400,
        false,
    )
    .await;
    let acme = companies::create(&api, "acme-retention", "admin@acme-retention.example").await;
    memory(
        acme.tenant_id,
        "rétention Acme",
        "episode",
        None,
        400,
        false,
    )
    .await;

    let retained = apply_retention(db).await.unwrap().unwrap();
    let lost = |tenant_id: Uuid| {
        retained
            .memories
            .iter()
            .find(|lost| lost.tenant_id == tenant_id)
            .map(|lost| (lost.aged, lost.departed))
    };
    assert_eq!(lost(neo), Some((2, 1)));
    assert_eq!(lost(acme.tenant_id), Some((1, 0)));
    assert_eq!(
        titles("rétention").await,
        [
            "rétention Paul",
            "rétention fait ancien",
            "rétention partant",
            "rétention revenu",
            "rétention épisode 11 mois",
        ]
    );

    // Each company told in its own log, by the platform, of the counts alone.
    let told: Vec<Value> = audit_log(&paul)
        .await
        .into_iter()
        .filter(|entry| entry["action"] == "memory.purged")
        .collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0]["actorType"], "system");
    assert_eq!(told[0]["metadata"], json!({ "aged": 2, "departed": 1 }));

    // Nothing more to delete: nothing more told.
    let again = apply_retention(db).await.unwrap().unwrap();
    assert!(again.memories.is_empty(), "{again:?}");

    // Another process running the pass: this one runs none.
    let held = db.try_sweep_lock(RETENTION_SWEEP).await.unwrap().unwrap();
    assert!(apply_retention(db).await.unwrap().is_none());
    held.release().await;
}
