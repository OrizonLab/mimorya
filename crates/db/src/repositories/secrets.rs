//! Encrypted secrets of a company (API keys…), as bytes: the API seals and
//! opens them; the database never sees a value in clear.

use sqlx::PgConnection;
use uuid::Uuid;

/// A sealed value: AES-256-GCM output and the version of the key that sealed it.
#[derive(Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredSecret {
    pub ciphertext: Vec<u8>,
    pub iv: Vec<u8>,
    pub auth_tag: Vec<u8>,
    pub key_version: i32,
}

/// Never printed: even sealed, a secret stays out of logs.
impl std::fmt::Debug for StoredSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredSecret")
            .field("key_version", &self.key_version)
            .finish_non_exhaustive()
    }
}

/// Creates or replaces the secret of that name.
pub async fn put_secret(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    name: &str,
    secret: &StoredSecret,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into secrets (tenant_id, name, ciphertext, iv, auth_tag, key_version) \
         values ($1, $2, $3, $4, $5, $6) \
         on conflict (tenant_id, name) do update set ciphertext = excluded.ciphertext, \
         iv = excluded.iv, auth_tag = excluded.auth_tag, key_version = excluded.key_version, \
         updated_at = now()",
    )
    .bind(tenant_id)
    .bind(name)
    .bind(&secret.ciphertext)
    .bind(&secret.iv)
    .bind(&secret.auth_tag)
    .bind(secret.key_version)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get_secret(conn: &mut PgConnection, name: &str) -> sqlx::Result<Option<StoredSecret>> {
    sqlx::query_as("select ciphertext, iv, auth_tag, key_version from secrets where name = $1")
        .bind(name)
        .fetch_optional(conn)
        .await
}

/// Returns whether a secret was deleted.
pub async fn delete_secret(conn: &mut PgConnection, name: &str) -> sqlx::Result<bool> {
    let result = sqlx::query("delete from secrets where name = $1")
        .bind(name)
        .execute(conn)
        .await?;
    Ok(result.rows_affected() > 0)
}
