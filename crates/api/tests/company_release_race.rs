//! A suspended company the operator releases (ADR-193) while another
//! command holds its row: the release, which locks the row before reading
//! the status, waits for it. Reactivated meanwhile, the company is refused
//! its release and keeps what it declared at other services; deleted
//! meanwhile, it is unknown. Without the lock, the release would have read
//! it suspended and withdrawn the bots of a company back in use.

mod support;

use mimorya_api::services::provisioning::{CompanyStatusChange, release_company_with};
use mimorya_core::{DomainError, ErrorCode};
use mimorya_db::{Database, PoolOptions};
use sqlx::PgPool;
use support::TestApi;
use support::app_url;
use support::companies::create;
use support::llm;
use support::locks::named_waits;
use support::retention::owner;
use support::suspension::change;
use uuid::Uuid;

/// An application with its master key: the release takes nothing without.
async fn keyed() -> TestApi {
    TestApi::start_with(&[("MIMORYA_MASTER_KEY", llm::MASTER_KEY)]).await
}

/// The release's own connection, told from any other by its name.
const RELEASE: &str = "mimorya-release-race";

/// A suspended company of its own, with a Telegram bot (its token never
/// kept: nothing is asked of Telegram), and the bot's id.
async fn suspended_with_a_bot(api: &TestApi, owner: &PgPool, slug: &str) -> (Uuid, Uuid) {
    let created = create(api, slug, &format!("jeanne.martin@{slug}.example")).await;
    let bot = sqlx::query_scalar(
        "insert into gateways (tenant_id, kind, name, workspace_id, workspace_name, bot_user_id) \
         values ($1, 'telegram', 'bot', '7', 'bot', '7') returning id",
    )
    .bind(created.tenant_id)
    .fetch_one(owner)
    .await
    .unwrap();
    change(api, slug, CompanyStatusChange::Suspend)
        .await
        .unwrap();
    (created.tenant_id, bot)
}

/// Another command's transaction, doing `statements` to the company's row,
/// uncommitted: holding it with the locks they take, and no other.
async fn holding(
    owner: &PgPool,
    tenant_id: Uuid,
    statements: &[&str],
) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut other = owner.begin().await.unwrap();
    for statement in statements {
        sqlx::query(statement)
            .bind(tenant_id)
            .execute(&mut *other)
            .await
            .unwrap();
    }
    other
}

/// The release of the company `slug`, on a connection of its own, started.
async fn releasing(
    api: &TestApi,
    slug: &'static str,
) -> tokio::task::JoinHandle<anyhow::Result<mimorya_api::services::provisioning::Released>> {
    let db = Database::connect(
        app_url(),
        PoolOptions {
            max_connections: 1,
            application_name: RELEASE,
        },
    )
    .await
    .unwrap();
    let config = api.app.services.config.clone();
    tokio::spawn(async move { release_company_with(&db, &config, slug).await })
}

fn refusal(released: anyhow::Result<mimorya_api::services::provisioning::Released>) -> DomainError {
    released
        .expect_err("refused")
        .downcast::<DomainError>()
        .expect("domain error")
}

async fn bots(owner: &PgPool, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("select count(*) from gateways where tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(owner)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_reactivated_meanwhile_is_not_released() {
    const SLUG: &str = "acme-retenue";
    let api = keyed().await;
    let owner = owner().await;
    let (tenant_id, _) = suspended_with_a_bot(&api, &owner, SLUG).await;

    // As a reactivation does: its conditional update alone, which takes no
    // more than a no key update lock on the row.
    let other = holding(
        &owner,
        tenant_id,
        &["update tenants set status = 'active' where id = $1 and status = 'suspended'"],
    )
    .await;
    let release = releasing(&api, SLUG).await;
    // It waits on the company's row before reading its status.
    assert!(named_waits(&owner, RELEASE).await);
    other.commit().await.unwrap();

    let refused = refusal(release.await.unwrap());
    assert_eq!(refused.code, ErrorCode::Conflict);
    assert!(
        refused
            .message
            .starts_with("Seule une entreprise suspendue"),
        "{}",
        refused.message
    );
    assert_eq!(bots(&owner, tenant_id).await, 1);
    owner.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_deleted_meanwhile_is_not_found() {
    const SLUG: &str = "acme-effacee";
    let api = keyed().await;
    let owner = owner().await;
    let (tenant_id, _) = suspended_with_a_bot(&api, &owner, SLUG).await;

    // As the operator's deletion does: the row locked, then deleted.
    let deletion = holding(
        &owner,
        tenant_id,
        &[
            "select id from tenants where id = $1 for update",
            "delete from tenants where id = $1",
        ],
    )
    .await;
    let release = releasing(&api, SLUG).await;
    assert!(named_waits(&owner, RELEASE).await);
    deletion.commit().await.unwrap();

    let refused = refusal(release.await.unwrap());
    assert_eq!(refused.code, ErrorCode::NotFound);
    assert_eq!(refused.message, "Aucune entreprise ne porte ce sigle");
    assert_eq!(bots(&owner, tenant_id).await, 0);
    owner.close().await;
}
