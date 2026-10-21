//! An account's identities at its company's providers (ADR-063): the
//! provider's immutable subject, tied at the first sign-in there. Deleted
//! with the provider or the account.

use sqlx::PgConnection;
use uuid::Uuid;

/// The account a provider's subject is tied to, and the tie.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LinkedIdentity {
    pub id: Uuid,
    pub user_id: Uuid,
}

pub async fn find_identity(
    conn: &mut PgConnection,
    provider_id: Uuid,
    subject: &str,
) -> sqlx::Result<Option<LinkedIdentity>> {
    sqlx::query_as(
        "select id, user_id from user_identities where provider_id = $1 and subject = $2",
    )
    .bind(provider_id)
    .bind(subject)
    .fetch_optional(conn)
    .await
}

/// Whether the account is tied to another subject of this provider.
pub async fn has_identity_at(
    conn: &mut PgConnection,
    provider_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "select exists (select 1 from user_identities where provider_id = $1 and user_id = $2)",
    )
    .bind(provider_id)
    .bind(user_id)
    .fetch_one(conn)
    .await
}

pub struct NewIdentity<'a> {
    pub tenant_id: Uuid,
    pub provider_id: Uuid,
    pub user_id: Uuid,
    pub subject: &'a str,
}

/// `None` when the subject or the account got tied meanwhile.
pub async fn link_identity(
    conn: &mut PgConnection,
    input: NewIdentity<'_>,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "insert into user_identities (tenant_id, provider_id, user_id, subject) \
         values ($1, $2, $3, $4) on conflict do nothing returning id",
    )
    .bind(input.tenant_id)
    .bind(input.provider_id)
    .bind(input.user_id)
    .bind(input.subject)
    .fetch_optional(conn)
    .await
}

pub async fn touch_identity(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<()> {
    sqlx::query("update user_identities set last_used_at = now() where id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

/// The account of this company with this address, if any.
pub async fn find_user_by_email(
    conn: &mut PgConnection,
    email: &str,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar("select id from users where email = $1::citext")
        .bind(email)
        .fetch_optional(conn)
        .await
}
