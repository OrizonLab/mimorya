//! A sign-in that waits for its second factor (ADR-200). Once a password
//! is checked, an account whose second factor is on gets no session but a
//! challenge, held in a cookie of the browser for five minutes; the code
//! of its app or a recovery code completes it, once, and opens the
//! session. An account the company requires one of, without one, gets a
//! challenge to enrol first (`sign_in_enrolment.rs`, ADR-201). A password
//! changed or an account disabled meanwhile ends it (trigger of migration
//! 0089); codes refused count against the account, however many challenges
//! are open.

use axum::http::HeaderMap;
use chrono::{Duration, Utc};
use mimorya_core::DomainError;
use mimorya_core::second_factor::SecondFactorCode;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::auth::lock_login_state;
use mimorya_db::repositories::login_challenges::{
    ChallengePurpose, LoginChallengeRecord, NewLoginChallenge, find_login_challenge,
    put_login_challenge, take_login_challenge,
};
use mimorya_db::repositories::second_factors::second_factor_state;
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use super::proof::{Attempt, prove};
use crate::auth::credentials::{hash_token, new_session_token};
use crate::auth::sessions::open_session;
use crate::http::{ApiError, ApiResult};
use crate::plugins::{RequestContext, SharedLimiter};
use crate::secrets::SecretCipher;

pub const CHALLENGE_COOKIE: &str = "mimorya_challenge";
/// Sent to the routes of the second factor at sign-in only.
const COOKIE_PATH: &str = "/api/auth/second-factor";
const CHALLENGE_LIFETIME: Duration = Duration::minutes(5);

pub const EXPIRED: &str = "Cette connexion a expiré : saisissez de nouveau votre mot de passe";

pub fn challenge_cookie(token: &str, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{CHALLENGE_COOKIE}={token}; Max-Age={}; Path={COOKIE_PATH}; HttpOnly; \
         SameSite=Strict{secure}",
        CHALLENGE_LIFETIME.num_seconds()
    )
}

pub fn cleared_challenge_cookie(secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{CHALLENGE_COOKIE}=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; \
         Path={COOKIE_PATH}; HttpOnly; SameSite=Strict{secure}"
    )
}

/// What a password checked opens.
#[derive(Debug)]
pub enum Opened {
    /// A session: its token, for the session cookie.
    Session(String),
    /// A challenge for the second factor, to prove or to enrol first: its
    /// token, for its own cookie.
    Challenge(String, ChallengePurpose),
}

/// After the password of `user_id` was checked (a login, a reset, an
/// accepted invitation), in its transaction, the account's row locked: a
/// session, or a challenge when its second factor is on, or when the
/// company requires one it has not (`required`, ADR-201).
pub async fn open_after_password(
    tx: &mut TenantTx,
    user_id: Uuid,
    required: bool,
    context: &RequestContext,
    headers: &HeaderMap,
) -> sqlx::Result<Opened> {
    let state = second_factor_state(tx, user_id).await?;
    let purpose = if state.is_some_and(|state| state.enabled_at.is_some()) {
        ChallengePurpose::Verify
    } else if required {
        ChallengePurpose::Enrol
    } else {
        return Ok(Opened::Session(
            open_session(tx, user_id, context, headers).await?,
        ));
    };
    let token = open_challenge(tx, user_id, purpose).await?;
    Ok(Opened::Challenge(token, purpose))
}

/// A challenge for `user_id` instead of a session: its token, for the
/// cookie.
async fn open_challenge(
    tx: &mut TenantTx,
    user_id: Uuid,
    purpose: ChallengePurpose,
) -> sqlx::Result<String> {
    let (token, token_hash) = new_session_token();
    let tenant_id = tx.tenant_id();
    let challenge = NewLoginChallenge {
        tenant_id,
        user_id,
        token_hash: &token_hash,
        purpose,
        expires_at: Utc::now() + CHALLENGE_LIFETIME,
    };
    put_login_challenge(tx, challenge).await?;
    Ok(token)
}

/// The challenge whose token's digest is `token_hash`, still pending, for
/// `purpose` if one is named.
pub(super) async fn pending(
    db: &Database,
    token_hash: &str,
    purpose: Option<ChallengePurpose>,
) -> anyhow::Result<LoginChallengeRecord> {
    match find_login_challenge(db.pool(), token_hash).await? {
        Some(record)
            if record.expires_at > Utc::now()
                && record.user_status == "active"
                && record.tenant_status == "active"
                && ChallengePurpose::parse(&record.purpose)
                    .is_some_and(|found| purpose.is_none_or(|wanted| wanted == found)) =>
        {
            Ok(record)
        }
        _ => Err(DomainError::unauthenticated(EXPIRED).into()),
    }
}

/// What the pending challenge of `token` asks for.
pub async fn challenge_purpose(db: &Database, token: &str) -> anyhow::Result<ChallengePurpose> {
    let record = pending(db, &hash_token(token), None).await?;
    Ok(ChallengePurpose::parse(&record.purpose).unwrap_or(ChallengePurpose::Verify))
}

/// What completing a challenge needs.
pub struct Completion<'a> {
    pub db: &'a Database,
    pub cipher: Option<&'a SecretCipher>,
    /// The account's refusals, shared by every process.
    pub refusals: &'a SharedLimiter,
    pub context: &'a RequestContext,
    pub headers: &'a HeaderMap,
}

/// Completes the challenge of `token` with `code`: the token of the
/// session it opens. Throttled per account: the refusal says so.
pub async fn complete_challenge(
    completion: Completion<'_>,
    token: &str,
    code: &SecondFactorCode,
) -> ApiResult<String> {
    let token_hash = hash_token(token);
    let record = pending(completion.db, &token_hash, Some(ChallengePurpose::Verify)).await?;
    let account = record.user_id.to_string();
    if !completion.refusals.allow(&account).await? {
        return Err(ApiError::RateLimited);
    }
    let mut tx = completion.db.begin(record.tenant_id).await?;
    // The account's row first, as a login or a reset: a password changed or
    // an account disabled meanwhile wins.
    let active = lock_login_state(&mut tx, record.user_id)
        .await?
        .is_some_and(|(_, status)| status == "active");
    if !active {
        // No code checked: nothing counts.
        completion.refusals.release(&account).await;
        return Err(DomainError::unauthenticated(EXPIRED).into());
    }
    let attempt = Attempt {
        cipher: completion.cipher,
        context: completion.context,
        place: "Connexion",
    };
    let proof = match prove(&mut tx, record.user_id, code, &attempt).await? {
        Ok(proof) => proof,
        Err(refusal) => {
            // Counted and traced before the refusal is said.
            tx.commit().await?;
            if !refusal.counts() {
                completion.refusals.release(&account).await;
            }
            return Err(refusal.error(true).into());
        }
    };
    // Once, with this very token, before it expires.
    let taken = take_login_challenge(&mut tx, record.challenge_id, &token_hash).await?;
    if taken != Some(record.user_id) {
        // Used or expired meanwhile: the code was right, nothing counts.
        drop(tx);
        completion.refusals.release(&account).await;
        return Err(DomainError::unauthenticated(EXPIRED).into());
    }
    let session = open_session(
        &mut tx,
        record.user_id,
        completion.context,
        completion.headers,
    )
    .await?;
    let mut entry = AuditEntry::new(record.tenant_id, "user", "auth.login", "success")
        .by_user(record.user_id)
        .ip(completion.context.ip_string())
        .metadata(json!({ "method": "password", "secondFactor": proof.as_str() }));
    entry.request_id = Some(completion.context.id.to_string());
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    completion.refusals.release(&account).await;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cookie_goes_to_the_second_factor_routes_alone() {
        let cookie = challenge_cookie("tok", true);
        assert!(cookie.starts_with("mimorya_challenge=tok; Max-Age=300;"));
        assert!(cookie.contains("Path=/api/auth/second-factor;"));
        assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
        assert!(cookie.ends_with("; Secure"));
        let cleared = cleared_challenge_cookie(false);
        assert!(cleared.contains("Max-Age=0") && !cleared.contains("Secure"));
    }
}
