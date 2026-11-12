//! Sign-ins half done (ADR-200): the password checked, the second factor
//! still to prove. `find_login_challenge` is a read before sign-in (a
//! SECURITY DEFINER function), with the periodic purge of expired ones; the
//! rest runs in the account's company. A change of password or status
//! deletes the account's challenges (trigger of migration 0089).

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// What a challenge is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengePurpose {
    /// The account's second factor, on, to prove.
    Verify,
    /// One the company requires, to enrol before the session opens.
    Enrol,
}

impl ChallengePurpose {
    /// Every purpose, in the order the database constraint lists them.
    pub const ALL: [Self; 2] = [Self::Verify, Self::Enrol];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verify => "verify",
            Self::Enrol => "enrol",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "verify" => Some(Self::Verify),
            "enrol" => Some(Self::Enrol),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LoginChallengeRecord {
    pub challenge_id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub purpose: String,
    pub expires_at: DateTime<Utc>,
    pub user_status: String,
    pub tenant_status: String,
}

pub async fn find_login_challenge(
    pool: &PgPool,
    token_hash: &str,
) -> sqlx::Result<Option<LoginChallengeRecord>> {
    sqlx::query_as(
        "select challenge_id, tenant_id, user_id, purpose, expires_at, user_status, \
         tenant_status from auth_find_login_challenge($1)",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

pub struct NewLoginChallenge<'a> {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub token_hash: &'a str,
    pub purpose: ChallengePurpose,
    pub expires_at: DateTime<Utc>,
}

pub async fn put_login_challenge(
    conn: &mut PgConnection,
    input: NewLoginChallenge<'_>,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into login_challenges (tenant_id, user_id, token_hash, purpose, expires_at) \
         values ($1, $2, $3, $4, $5)",
    )
    .bind(input.tenant_id)
    .bind(input.user_id)
    .bind(input.token_hash)
    .bind(input.purpose.as_str())
    .bind(input.expires_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Takes the challenge for its single use: deleted, and its account
/// returned, only if it still holds this token and has not expired when
/// this statement runs.
pub async fn take_login_challenge(
    conn: &mut PgConnection,
    challenge_id: Uuid,
    token_hash: &str,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "delete from login_challenges where id = $1 and token_hash = $2 \
         and expires_at > statement_timestamp() returning user_id",
    )
    .bind(challenge_id)
    .bind(token_hash)
    .fetch_optional(conn)
    .await
}

/// Deletes, in every company, the expired challenges and the enrolments
/// left waiting a day; returns how many.
pub async fn purge_second_factor_leftovers(pool: &PgPool) -> sqlx::Result<i32> {
    sqlx::query_scalar("select system_purge_second_factor_leftovers()")
        .fetch_one(pool)
        .await
}
