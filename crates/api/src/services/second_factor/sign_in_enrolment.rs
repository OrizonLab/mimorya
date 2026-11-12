//! A sign-in that waits for an enrolment (ADR-201): the company requires a
//! second factor of the account, which has none. Its challenge, opened once
//! the password is checked, starts the enrolment (no need to type the
//! password again: it was just checked), then takes its first code, which
//! turns the second factor on, gives the recovery codes, ends the account's
//! other sessions and opens this one. A wrong first code counts against the
//! account and is traced, as at any sign-in.

use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::auth::lock_login_state;
use mimorya_db::repositories::login_challenges::{
    ChallengePurpose, LoginChallengeRecord, take_login_challenge,
};
use mimorya_db::repositories::users::{get_user, revoke_user_sessions};
use serde_json::json;
use uuid::Uuid;

use super::activation::{Enrolment, WRONG_FIRST_CODE, activate, begin};
use super::challenge::{Completion, EXPIRED, pending};
use crate::auth::credentials::hash_token;
use crate::auth::sessions::open_session;
use crate::http::{ApiError, ApiResult};
use crate::plugins::RequestContext;
use crate::secrets::SecretCipher;

const NO_MASTER_KEY: &str = "Votre entreprise exige un second facteur, mais ce serveur ne peut \
     pas en conserver : l’exploitant n’a pas configuré MIMORYA_MASTER_KEY";

fn cipher_of(cipher: Option<&SecretCipher>) -> Result<&SecretCipher, DomainError> {
    cipher.ok_or_else(|| DomainError::unavailable(NO_MASTER_KEY))
}

/// Starts the enrolment the challenge of `token` waits for.
pub async fn start_sign_in_enrolment(
    db: &Database,
    cipher: Option<&SecretCipher>,
    token: &str,
) -> anyhow::Result<Enrolment> {
    // The challenge first: nothing is said of the server to whoever holds
    // none.
    let record = pending(db, &hash_token(token), Some(ChallengePurpose::Enrol)).await?;
    let cipher = cipher_of(cipher)?;
    let mut tx = db.begin(record.tenant_id).await?;
    let account = get_user(&mut tx, record.user_id)
        .await?
        .ok_or_else(|| DomainError::unauthenticated(EXPIRED))?;
    let enrolment = begin(&mut tx, cipher, record.user_id, &account.email).await?;
    tx.commit().await?;
    Ok(enrolment)
}

/// An enrolment completed at sign-in.
pub struct EnrolledSignIn {
    /// The recovery codes, shown this once.
    pub codes: Vec<String>,
    /// The session it opens.
    pub session: String,
}

fn entry(
    tenant_id: Uuid,
    user_id: Uuid,
    action: &str,
    outcome: &'static str,
    context: &RequestContext,
) -> AuditEntry {
    let mut entry = AuditEntry::new(tenant_id, "user", action, outcome)
        .by_user(user_id)
        .ip(context.ip_string());
    entry.request_id = Some(context.id.to_string());
    entry
}

/// Turns the enrolment on with its first `code` and opens the session.
/// Throttled per account, as any code at sign-in.
pub async fn complete_sign_in_enrolment(
    completion: Completion<'_>,
    token: &str,
    code: u32,
) -> ApiResult<EnrolledSignIn> {
    let token_hash = hash_token(token);
    let record = pending(completion.db, &token_hash, Some(ChallengePurpose::Enrol)).await?;
    let cipher = cipher_of(completion.cipher)?;
    let account = record.user_id.to_string();
    if !completion.refusals.allow(&account).await? {
        return Err(ApiError::RateLimited);
    }
    let outcome = enrol(&completion, cipher, &record, &token_hash, code).await;
    // Only a wrong first code counts.
    if !matches!(outcome, Ok(None)) {
        completion.refusals.release(&account).await;
    }
    match outcome? {
        Some(enrolled) => Ok(enrolled),
        None => Err(DomainError::unauthenticated(WRONG_FIRST_CODE).into()),
    }
}

/// The enrolment turned on and the session opened, or `None` when the code
/// is not the app's (the refusal traced).
async fn enrol(
    completion: &Completion<'_>,
    cipher: &SecretCipher,
    record: &LoginChallengeRecord,
    token_hash: &str,
    code: u32,
) -> ApiResult<Option<EnrolledSignIn>> {
    let (tenant_id, user_id) = (record.tenant_id, record.user_id);
    let context = completion.context;
    let mut tx = completion.db.begin(tenant_id).await?;
    // The account's row first, as any sign-in: a password changed or an
    // account disabled meanwhile wins.
    let active = lock_login_state(&mut tx, user_id)
        .await?
        .is_some_and(|(_, status)| status == "active");
    if !active {
        return Err(DomainError::unauthenticated(EXPIRED).into());
    }
    let Some(codes) = activate(&mut tx, cipher, user_id, code).await? else {
        let refused = entry(
            tenant_id,
            user_id,
            "auth.second_factor_failed",
            "denied",
            context,
        )
        .resource("user", user_id)
        .reason("Activation à la connexion : code refusé");
        append_audit(&mut tx, refused).await?;
        tx.commit().await?;
        return Ok(None);
    };
    // Once, with this very token, before it expires.
    if take_login_challenge(&mut tx, record.challenge_id, token_hash).await? != Some(user_id) {
        return Err(DomainError::unauthenticated(EXPIRED).into());
    }
    revoke_user_sessions(&mut tx, user_id, None).await?;
    let session = open_session(&mut tx, user_id, context, completion.headers).await?;
    let enabled = entry(
        tenant_id,
        user_id,
        "account.second_factor_enabled",
        "success",
        context,
    )
    .resource("user", user_id)
    .metadata(json!({ "atSignIn": true }));
    append_audit(&mut tx, enabled).await?;
    let signed_in = entry(tenant_id, user_id, "auth.login", "success", context)
        .metadata(json!({ "method": "password", "secondFactor": "enrolled" }));
    append_audit(&mut tx, signed_in).await?;
    tx.commit().await?;
    Ok(Some(EnrolledSignIn { codes, session }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_master_key_the_refusal_says_whom_to_ask() {
        let refused = cipher_of(None).unwrap_err();
        assert!(refused.message.contains("MIMORYA_MASTER_KEY"));
    }
}
