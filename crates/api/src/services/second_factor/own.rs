//! One's own second factor, from « Mon compte » (ADR-200): its state; its
//! enrolment, started with a new secret once the password is typed again,
//! and turned on by the first code of the app, which gives the recovery
//! codes and ends the account's other sessions; new recovery codes and its
//! disabling, each proven by a current code. An account that signs in
//! through its company's identity provider has none here: its second
//! factor is the provider's. Without a master key, nothing is enrolled.

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_core::second_factor::{SecondFactorCode, accepted_step, base32, otpauth_uri, step_at};
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::second_factors::{
    delete_second_factor, enable_second_factor, lock_second_factor, put_pending_second_factor,
    replace_recovery_codes, second_factor_state,
};
use mimorya_db::repositories::users::{get_password_hash, revoke_user_sessions};
use uuid::Uuid;

use super::proof::{Attempt, Proof, Refusal, prove};
use super::secret::{new_recovery_codes, new_secret, open_secret};
use crate::auth::credentials::verify_password_async;
use crate::plugins::RequestContext;
use crate::secrets::SecretStore;
use crate::services::AccessContext;

const NO_PASSWORD: &str = "Votre compte se connecte par le fournisseur d’identité de votre \
     entreprise : c’est lui qui demande un second facteur.";
const ALREADY_ON: &str = "Le second facteur est déjà actif : désactivez-le d’abord pour en \
     changer";
const NO_ENROLMENT: &str = "Aucun enrôlement n’est en cours : recommencez depuis le début";

/// One's own second factor, as « Mon compte » shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnSecondFactor {
    pub applicable: bool,
    pub available: bool,
    pub enabled_at: Option<chrono::DateTime<Utc>>,
    pub recovery_codes_left: i64,
}

/// What the authenticator app is given.
#[derive(Debug, Clone)]
pub struct Enrolment {
    pub secret: String,
    pub uri: String,
}

fn entry(
    access: &AccessContext,
    action: &str,
    outcome: &'static str,
    context: &RequestContext,
) -> AuditEntry {
    let user_id = access.subject.user_id;
    let mut entry = AuditEntry::new(access.subject.tenant_id, "user", action, outcome)
        .by_user(user_id)
        .resource("user", user_id)
        .ip(context.ip_string());
    entry.request_id = Some(context.id.to_string());
    entry
}

pub async fn own_second_factor(
    db: &Database,
    secrets: &SecretStore,
    access: &AccessContext,
) -> anyhow::Result<OwnSecondFactor> {
    let mut tx = db.begin(access.subject.tenant_id).await?;
    let user_id = access.subject.user_id;
    let has_password = get_password_hash(&mut tx, user_id).await?.is_some();
    let state = second_factor_state(&mut tx, user_id).await?;
    tx.commit().await?;
    let enabled_at = state.and_then(|state| state.enabled_at);
    Ok(OwnSecondFactor {
        applicable: has_password,
        available: secrets.available(),
        enabled_at,
        recovery_codes_left: match (enabled_at, state) {
            (Some(_), Some(state)) => state.recovery_codes_left,
            _ => 0,
        },
    })
}

/// Starts an enrolment with a new secret, replacing one that waited, once
/// the account's password is typed again: a stolen session alone cannot
/// tie the account to another app. `None` when the password is wrong, the
/// only outcome that counts as a guess of it.
pub async fn start_enrolment(
    db: &Database,
    secrets: &SecretStore,
    access: &AccessContext,
    password: String,
) -> anyhow::Result<Option<Enrolment>> {
    let cipher = secrets.cipher()?;
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = db.begin(tenant_id).await?;
    let hash = get_password_hash(&mut tx, user_id).await?;
    tx.commit().await?;
    if hash.is_none() {
        return Err(DomainError::conflict(NO_PASSWORD).into());
    }
    if !verify_password_async(hash, password).await {
        return Ok(None);
    }
    let (secret, sealed) = new_secret(cipher, tenant_id, user_id)?;
    let mut tx = db.begin(tenant_id).await?;
    if !put_pending_second_factor(&mut tx, tenant_id, user_id, &sealed).await? {
        return Err(DomainError::conflict(ALREADY_ON).into());
    }
    tx.commit().await?;
    Ok(Some(Enrolment {
        secret: base32(&secret),
        uri: otpauth_uri(&secret, &access.profile.email),
    }))
}

/// Turns the enrolment on with the first code of the app: the recovery
/// codes, shown this once, the account's other sessions ended (one opened
/// before may be someone else's); `None` when the code is not the app's,
/// the refusal traced.
pub async fn confirm_enrolment(
    db: &Database,
    secrets: &SecretStore,
    access: &AccessContext,
    session_id: Uuid,
    code: u32,
    context: &RequestContext,
) -> anyhow::Result<Option<Vec<String>>> {
    let cipher = secrets.cipher()?;
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = db.begin(tenant_id).await?;
    let pending = lock_second_factor(&mut tx, user_id)
        .await?
        .filter(|factor| factor.enabled_at.is_none())
        .ok_or_else(|| DomainError::conflict(NO_ENROLMENT))?;
    let secret = open_secret(cipher, tenant_id, user_id, &pending.secret)
        .map_err(|_| DomainError::conflict(NO_ENROLMENT))?;
    let now = step_at(Utc::now().timestamp());
    let Some(step) = accepted_step(&secret, code, now, pending.last_step) else {
        let refused = entry(access, "auth.second_factor_failed", "denied", context)
            .reason("Activation : code refusé");
        append_audit(&mut tx, refused).await?;
        tx.commit().await?;
        return Ok(None);
    };
    if !enable_second_factor(&mut tx, user_id, step).await? {
        return Err(DomainError::conflict(NO_ENROLMENT).into());
    }
    let (codes, digests) = new_recovery_codes();
    replace_recovery_codes(&mut tx, tenant_id, user_id, &digests).await?;
    revoke_user_sessions(&mut tx, user_id, Some(session_id)).await?;
    append_audit(
        &mut tx,
        entry(access, "account.second_factor_enabled", "success", context),
    )
    .await?;
    tx.commit().await?;
    Ok(Some(codes))
}

/// New recovery codes, the former ones dropped, proven by a current code:
/// the codes, shown this once, or why the code was refused.
pub async fn renew_recovery_codes(
    db: &Database,
    secrets: &SecretStore,
    access: &AccessContext,
    code: &SecondFactorCode,
    context: &RequestContext,
) -> anyhow::Result<Result<Vec<String>, Refusal>> {
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = db.begin(tenant_id).await?;
    let attempt = Attempt {
        cipher: secrets.cipher().ok(),
        context,
        place: "Nouveaux codes de secours",
    };
    if let Err(refusal) = prove(&mut tx, user_id, code, &attempt).await? {
        tx.commit().await?;
        return Ok(Err(refusal));
    }
    let (codes, digests) = new_recovery_codes();
    replace_recovery_codes(&mut tx, tenant_id, user_id, &digests).await?;
    append_audit(
        &mut tx,
        entry(access, "account.recovery_codes_renewed", "success", context),
    )
    .await?;
    tx.commit().await?;
    Ok(Ok(codes))
}

/// Turns the second factor off, proven by a current code: how, or why the
/// code was refused. Without one on, there is nothing to turn off: an
/// enrolment that waits stays until replaced or purged.
pub async fn disable_second_factor(
    db: &Database,
    secrets: &SecretStore,
    access: &AccessContext,
    code: &SecondFactorCode,
    context: &RequestContext,
) -> anyhow::Result<Result<Proof, Refusal>> {
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let mut tx = db.begin(tenant_id).await?;
    let attempt = Attempt {
        cipher: secrets.cipher().ok(),
        context,
        place: "Désactivation",
    };
    let proof = match prove(&mut tx, user_id, code, &attempt).await? {
        Ok(proof) => proof,
        Err(refusal) => {
            tx.commit().await?;
            return Ok(Err(refusal));
        }
    };
    delete_second_factor(&mut tx, user_id).await?;
    append_audit(
        &mut tx,
        entry(access, "account.second_factor_disabled", "success", context)
            .metadata(serde_json::json!({ "proof": proof.as_str() })),
    )
    .await?;
    tx.commit().await?;
    Ok(Ok(proof))
}
