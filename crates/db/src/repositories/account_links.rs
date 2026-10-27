//! Accounts each person links at a provider their company declared
//! (ADR-116): who the provider says they are, the permissions granted, and
//! the provider's refresh token, sealed by the API before it reaches the
//! database. One link per person and connector; a new link replaces the
//! previous one.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::secrets::StoredSecret;

/// A link as listed: never its sealed token.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AccountLinkRow {
    pub id: Uuid,
    pub connector_id: Uuid,
    pub user_id: Uuid,
    pub subject: String,
    pub address: String,
    pub scopes: String,
    pub created_at: DateTime<Utc>,
    pub refreshed_at: DateTime<Utc>,
}

const SELECT: &str = "select id, connector_id, user_id, subject, address, scopes, created_at, \
    refreshed_at from account_links";

pub struct NewAccountLink<'a> {
    pub tenant_id: Uuid,
    pub connector_id: Uuid,
    pub user_id: Uuid,
    pub subject: &'a str,
    pub address: &'a str,
    pub scopes: &'a str,
    pub refresh_token: &'a StoredSecret,
}

/// Creates the person's link at this connector, or replaces it; returns
/// whether one was replaced.
pub async fn put_account_link(
    conn: &mut PgConnection,
    link: NewAccountLink<'_>,
) -> sqlx::Result<bool> {
    let replaced = sqlx::query_scalar::<_, bool>(
        "select exists(select 1 from account_links where connector_id = $1 and user_id = $2)",
    )
    .bind(link.connector_id)
    .bind(link.user_id)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query(
        "insert into account_links (tenant_id, connector_id, user_id, subject, address, scopes, \
         refresh_ciphertext, refresh_iv, refresh_auth_tag, refresh_key_version) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         on conflict (connector_id, user_id) do update set subject = excluded.subject, \
         address = excluded.address, scopes = excluded.scopes, \
         refresh_ciphertext = excluded.refresh_ciphertext, refresh_iv = excluded.refresh_iv, \
         refresh_auth_tag = excluded.refresh_auth_tag, \
         refresh_key_version = excluded.refresh_key_version, created_at = now(), \
         refreshed_at = now()",
    )
    .bind(link.tenant_id)
    .bind(link.connector_id)
    .bind(link.user_id)
    .bind(link.subject)
    .bind(link.address)
    .bind(link.scopes)
    .bind(&link.refresh_token.ciphertext)
    .bind(&link.refresh_token.iv)
    .bind(&link.refresh_token.auth_tag)
    .bind(link.refresh_token.key_version)
    .execute(conn)
    .await?;
    Ok(replaced)
}

/// Whether the person's account is active, its row held until the
/// transaction ends: a deactivation, which removes their links, waits for
/// a link being kept, or is seen by it.
pub async fn lock_active_account(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<bool> {
    let status: Option<String> =
        sqlx::query_scalar("select status from users where id = $1 for share")
            .bind(user_id)
            .fetch_optional(conn)
            .await?;
    Ok(status.as_deref() == Some("active"))
}

/// The person's links, oldest connector first.
pub async fn list_account_links_of(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<AccountLinkRow>> {
    sqlx::query_as(&format!("{SELECT} where user_id = $1 order by created_at"))
        .bind(user_id)
        .fetch_all(conn)
        .await
}

/// How many people linked an account at this connector.
pub async fn count_account_links(conn: &mut PgConnection, connector_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar("select count(*) from account_links where connector_id = $1")
        .bind(connector_id)
        .fetch_one(conn)
        .await
}

#[derive(sqlx::FromRow)]
struct SealedLinkRow {
    #[sqlx(flatten)]
    link: AccountLinkRow,
    refresh_ciphertext: Vec<u8>,
    refresh_iv: Vec<u8>,
    refresh_auth_tag: Vec<u8>,
    refresh_key_version: i32,
}

/// The person's link at this connector and its sealed refresh token.
pub async fn get_account_link(
    conn: &mut PgConnection,
    connector_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<(AccountLinkRow, StoredSecret)>> {
    let row: Option<SealedLinkRow> = sqlx::query_as(
        "select id, connector_id, user_id, subject, address, scopes, created_at, refreshed_at, \
         refresh_ciphertext, refresh_iv, refresh_auth_tag, refresh_key_version \
         from account_links where connector_id = $1 and user_id = $2",
    )
    .bind(connector_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|row| {
        (
            row.link,
            StoredSecret {
                ciphertext: row.refresh_ciphertext,
                iv: row.refresh_iv,
                auth_tag: row.refresh_auth_tag,
                key_version: row.refresh_key_version,
            },
        )
    }))
}

/// Returns whether the person had a link at this connector.
pub async fn delete_account_link(
    conn: &mut PgConnection,
    connector_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from account_links where connector_id = $1 and user_id = $2")
        .bind(connector_id)
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Every link of the person (their account disabled); returns how many.
pub async fn delete_account_links_of(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<u64> {
    let result = sqlx::query("delete from account_links where user_id = $1")
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(result.rows_affected())
}
