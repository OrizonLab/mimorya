//! An enrolment's two steps (ADR-200, ADR-201), from « Mon compte » or at
//! a sign-in when the company requires one: a new secret, which replaces an
//! enrolment that waited but never a second factor on; then the first code
//! of the app, which turns it on. The second factor's row is locked: two
//! first codes at once turn it on once.

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_core::second_factor::{accepted_step, base32, otpauth_uri, step_at};
use mimorya_db::TenantTx;
use mimorya_db::repositories::second_factors::{
    enable_second_factor, lock_second_factor, put_pending_second_factor, replace_recovery_codes,
};
use uuid::Uuid;

use super::secret::{new_recovery_codes, new_secret, open_secret};
use crate::secrets::SecretCipher;

pub(super) const NO_ENROLMENT: &str =
    "Aucun enrôlement n’est en cours : recommencez depuis le début";
pub const WRONG_FIRST_CODE: &str = "Code incorrect : vérifiez que l’heure de votre \
     téléphone est juste et saisissez le code affiché maintenant";
const ALREADY_ON: &str = "Le second facteur est déjà actif sur ce compte";

/// What the authenticator app is given.
#[derive(Debug, Clone)]
pub struct Enrolment {
    pub secret: String,
    pub uri: String,
}

/// Starts the enrolment of `user_id`, whose address is `email`, with a new
/// secret.
pub(super) async fn begin(
    tx: &mut TenantTx,
    cipher: &SecretCipher,
    user_id: Uuid,
    email: &str,
) -> anyhow::Result<Enrolment> {
    let tenant_id = tx.tenant_id();
    let (secret, sealed) = new_secret(cipher, tenant_id, user_id)?;
    if !put_pending_second_factor(tx, tenant_id, user_id, &sealed).await? {
        return Err(DomainError::conflict(ALREADY_ON).into());
    }
    Ok(Enrolment {
        secret: base32(&secret),
        uri: otpauth_uri(&secret, email),
    })
}

/// Turns the enrolment of `user_id` on with `code`: its recovery codes,
/// shown this once; `None` when the code is not the app's.
pub(super) async fn activate(
    tx: &mut TenantTx,
    cipher: &SecretCipher,
    user_id: Uuid,
    code: u32,
) -> anyhow::Result<Option<Vec<String>>> {
    let tenant_id = tx.tenant_id();
    let pending = lock_second_factor(tx, user_id)
        .await?
        .filter(|factor| factor.enabled_at.is_none())
        .ok_or_else(|| DomainError::conflict(NO_ENROLMENT))?;
    let secret = open_secret(cipher, tenant_id, user_id, &pending.secret)
        .map_err(|_| DomainError::conflict(NO_ENROLMENT))?;
    let now = step_at(Utc::now().timestamp());
    let Some(step) = accepted_step(&secret, code, now, pending.last_step) else {
        return Ok(None);
    };
    if !enable_second_factor(tx, user_id, step).await? {
        return Err(DomainError::conflict(NO_ENROLMENT).into());
    }
    let (codes, digests) = new_recovery_codes();
    replace_recovery_codes(tx, tenant_id, user_id, &digests).await?;
    Ok(Some(codes))
}
