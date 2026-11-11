//! A code checked against an account's second factor (ADR-200), its row
//! locked: six digits of the authenticator app, accepted once, for a step
//! after the last one accepted, unless a hundred were refused in a row; or
//! one of the recovery codes, each once. A code accepted is spent in the
//! caller's transaction; a code refused is counted and traced there, which
//! the caller commits before answering. Neither code is ever written down.

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_core::second_factor::{
    REFUSALS_BEFORE_LOCK, SecondFactorCode, accepted_step, recovery_digest, step_at,
};
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::second_factors::{
    accept_step, count_refusal, lock_second_factor, second_factor_state, use_recovery_code,
};
use serde_json::json;
use uuid::Uuid;

use super::secret::open_secret;
use crate::plugins::RequestContext;
use crate::secrets::SecretCipher;

/// How a code was proven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proof {
    /// Six digits of the authenticator app.
    App,
    /// A recovery code, now spent.
    Recovery,
}

impl Proof {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Recovery => "recovery",
        }
    }
}

/// Why a code was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Not the account's code, or one already used.
    Wrong,
    /// A hundred app codes refused in a row: only a recovery code passes.
    TooMany,
    /// The secret does not open here (no master key, or another one).
    Unreadable,
    /// The account has no second factor on.
    Absent,
}

impl Refusal {
    fn reason(self) -> &'static str {
        match self {
            Self::Wrong => "code refusé",
            Self::TooMany => "trop de codes refusés à la suite",
            Self::Unreadable => "secret illisible sur ce serveur",
            Self::Absent => "aucun second facteur actif",
        }
    }

    /// The refusal said to the person: before sign-in, as an unknown
    /// credential; in a session, as a refused action.
    pub fn error(self, signing_in: bool) -> DomainError {
        let message = match self {
            Self::Wrong => "Code incorrect ou déjà utilisé",
            Self::TooMany => {
                "Trop de codes refusés à la suite : saisissez un code de secours, ou demandez à \
                 votre administrateur de réinitialiser votre second facteur"
            }
            Self::Unreadable => {
                "Les codes de votre application ne peuvent pas être vérifiés sur ce serveur \
                 (clé maîtresse absente ou changée) : saisissez un code de secours"
            }
            Self::Absent => return DomainError::conflict(NO_FACTOR),
        };
        if signing_in {
            DomainError::unauthenticated(message)
        } else {
            DomainError::forbidden(message)
        }
    }
}

pub(super) const NO_FACTOR: &str = "Aucun second facteur n’est actif sur ce compte";

/// Where a code is checked, for the audit log.
pub struct Attempt<'a> {
    pub cipher: Option<&'a SecretCipher>,
    pub context: &'a RequestContext,
    /// « Connexion », « Désactivation »…
    pub place: &'static str,
}

impl Attempt<'_> {
    fn entry(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        action: &str,
        outcome: &'static str,
    ) -> AuditEntry {
        let mut entry = AuditEntry::new(tenant_id, "user", action, outcome)
            .by_user(user_id)
            .resource("user", user_id)
            .ip(self.context.ip_string());
        entry.request_id = Some(self.context.id.to_string());
        entry
    }
}

/// Checks `code` against the second factor of `user_id`.
pub async fn prove(
    tx: &mut TenantTx,
    user_id: Uuid,
    code: &SecondFactorCode,
    attempt: &Attempt<'_>,
) -> anyhow::Result<Result<Proof, Refusal>> {
    let tenant_id = tx.tenant_id();
    let checked = check(tx, user_id, code, attempt.cipher).await?;
    match checked {
        Ok(Proof::Recovery) => {
            let left = second_factor_state(tx, user_id)
                .await?
                .map_or(0, |state| state.recovery_codes_left);
            let entry = attempt
                .entry(tenant_id, user_id, "auth.recovery_code_used", "success")
                .metadata(json!({ "place": attempt.place, "codesLeft": left }));
            append_audit(tx, entry).await?;
        }
        Ok(Proof::App) => {}
        Err(refusal) => {
            if refusal == Refusal::Wrong {
                count_refusal(tx, user_id).await?;
            }
            let entry = attempt
                .entry(tenant_id, user_id, "auth.second_factor_failed", "denied")
                .reason(format!("{} : {}", attempt.place, refusal.reason()));
            append_audit(tx, entry).await?;
        }
    }
    Ok(checked)
}

async fn check(
    tx: &mut TenantTx,
    user_id: Uuid,
    code: &SecondFactorCode,
    cipher: Option<&SecretCipher>,
) -> anyhow::Result<Result<Proof, Refusal>> {
    let Some(factor) = lock_second_factor(tx, user_id).await? else {
        return Ok(Err(Refusal::Absent));
    };
    if factor.enabled_at.is_none() {
        return Ok(Err(Refusal::Absent));
    }
    match code {
        SecondFactorCode::Recovery(code) => {
            let used = use_recovery_code(tx, user_id, &recovery_digest(code)).await?;
            Ok(if used {
                Ok(Proof::Recovery)
            } else {
                Err(Refusal::Wrong)
            })
        }
        SecondFactorCode::Totp(code) => {
            if factor.failed_attempts >= REFUSALS_BEFORE_LOCK {
                return Ok(Err(Refusal::TooMany));
            }
            let tenant_id = tx.tenant_id();
            let secret =
                cipher.and_then(|c| open_secret(c, tenant_id, user_id, &factor.secret).ok());
            let Some(secret) = secret else {
                return Ok(Err(Refusal::Unreadable));
            };
            let now = step_at(Utc::now().timestamp());
            let Some(step) = accepted_step(&secret, *code, now, factor.last_step) else {
                return Ok(Err(Refusal::Wrong));
            };
            // The row is locked: a concurrent use of the same code waits,
            // then finds the step taken.
            Ok(if accept_step(tx, user_id, step).await? {
                Ok(Proof::App)
            } else {
                Err(Refusal::Wrong)
            })
        }
    }
}
