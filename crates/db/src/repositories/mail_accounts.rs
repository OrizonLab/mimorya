//! The accounts people link at their company's standard mail (ADR-205):
//! their identifier and their sealed password, one per connector and
//! person, read and written in the company's transaction. The connector's
//! deletion and the person's take theirs with them.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::secrets::StoredSecret;

/// What is kept of a person's account, its password sealed.
#[derive(Debug, Clone)]
pub struct MailAccountRow {
    pub connector_id: Uuid,
    pub username: String,
    pub password: StoredSecret,
    pub linked_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct Row {
    connector_id: Uuid,
    username: String,
    password_ciphertext: Vec<u8>,
    password_iv: Vec<u8>,
    password_tag: Vec<u8>,
    key_version: i32,
    linked_at: DateTime<Utc>,
}

impl From<Row> for MailAccountRow {
    fn from(row: Row) -> Self {
        Self {
            connector_id: row.connector_id,
            username: row.username,
            password: StoredSecret {
                ciphertext: row.password_ciphertext,
                iv: row.password_iv,
                auth_tag: row.password_tag,
                key_version: row.key_version,
            },
            linked_at: row.linked_at,
        }
    }
}

const COLUMNS: &str = "connector_id, username, password_ciphertext, password_iv, \
                       password_tag, key_version, linked_at";

/// The account `user_id` linked at the connector `connector_id`, if any.
pub async fn get_mail_account(
    conn: &mut PgConnection,
    connector_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<MailAccountRow>> {
    let row: Option<Row> = sqlx::query_as(&format!(
        "select {COLUMNS} from mail_accounts where connector_id = $1 and user_id = $2"
    ))
    .bind(connector_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(Into::into))
}

/// The configuration of the connector `connector_id`, of that type,
/// locked against any change until the end of the transaction: an
/// account is kept only for the servers it was checked against.
pub async fn lock_mail_connector(
    conn: &mut PgConnection,
    connector_id: Uuid,
    connector_type: &str,
) -> sqlx::Result<Option<(bool, Value)>> {
    sqlx::query_as(
        "select enabled, config from connector_instances \
         where id = $1 and type = $2 for share",
    )
    .bind(connector_id)
    .bind(connector_type)
    .fetch_optional(conn)
    .await
}

/// Keeps `user_id`'s account at `connector_id`, replacing the one they
/// had; whether one was replaced.
pub async fn put_mail_account(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    connector_id: Uuid,
    user_id: Uuid,
    username: &str,
    password: &StoredSecret,
) -> sqlx::Result<bool> {
    let replaced: bool = sqlx::query_scalar(
        "insert into mail_accounts (tenant_id, connector_id, user_id, username, \
           password_ciphertext, password_iv, password_tag, key_version) \
         values ($1, $2, $3, $4, $5, $6, $7, $8) \
         on conflict (tenant_id, connector_id, user_id) do update set \
           username = excluded.username, \
           password_ciphertext = excluded.password_ciphertext, \
           password_iv = excluded.password_iv, \
           password_tag = excluded.password_tag, \
           key_version = excluded.key_version, \
           linked_at = now() \
         returning (xmax <> 0)",
    )
    .bind(tenant_id)
    .bind(connector_id)
    .bind(user_id)
    .bind(username)
    .bind(&password.ciphertext)
    .bind(&password.iv)
    .bind(&password.auth_tag)
    .bind(password.key_version)
    .fetch_one(conn)
    .await?;
    Ok(replaced)
}

/// Removes `user_id`'s account at `connector_id`; whether there was one.
pub async fn delete_mail_account(
    conn: &mut PgConnection,
    connector_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<bool> {
    let deleted = sqlx::query("delete from mail_accounts where connector_id = $1 and user_id = $2")
        .bind(connector_id)
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

/// Removes the account `user_id` linked at `connector_id` on `linked_at`,
/// which the company's server refused: one linked again since stays.
pub async fn delete_refused_mail_account(
    conn: &mut PgConnection,
    connector_id: Uuid,
    user_id: Uuid,
    linked_at: DateTime<Utc>,
) -> sqlx::Result<bool> {
    let deleted = sqlx::query(
        "delete from mail_accounts where connector_id = $1 and user_id = $2 and linked_at = $3",
    )
    .bind(connector_id)
    .bind(user_id)
    .bind(linked_at)
    .execute(conn)
    .await?;
    Ok(deleted.rows_affected() > 0)
}

/// Removes every account `user_id` linked: their account is disabled.
pub async fn delete_mail_accounts_of_user(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<u64> {
    let deleted = sqlx::query("delete from mail_accounts where user_id = $1")
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected())
}

/// Removes every account linked at `connector_id`: one of its servers
/// moved to another host.
pub async fn delete_mail_accounts_of_connector(
    conn: &mut PgConnection,
    connector_id: Uuid,
) -> sqlx::Result<u64> {
    let deleted = sqlx::query("delete from mail_accounts where connector_id = $1")
        .bind(connector_id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected())
}

/// How many people linked an account at `connector_id`.
pub async fn count_mail_accounts(conn: &mut PgConnection, connector_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar("select count(*) from mail_accounts where connector_id = $1")
        .bind(connector_id)
        .fetch_one(conn)
        .await
}

/// An account the person linked, as their export says it: never its
/// password.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportedMailAccount {
    pub connector: String,
    pub username: String,
    pub linked_at: DateTime<Utc>,
}

pub async fn exported_mail_accounts(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<ExportedMailAccount>> {
    sqlx::query_as(
        "select c.name as connector, m.username, m.linked_at from mail_accounts m \
         join connector_instances c on c.tenant_id = m.tenant_id and c.id = m.connector_id \
         where m.user_id = $1 order by m.linked_at",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await
}
