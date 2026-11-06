//! Whose OAuth authorization a renewal is of (ADR-171, ADR-174): the
//! company's, or a person's own on a server each person authorizes. Each
//! is claimed, kept, said or removed in its own table, its tokens sealed
//! under its own names, and its removal audited with whose it was.

use mimorya_connectors::mcp::oauth::OAuthHolder;
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::AuditEntry;
use mimorya_db::repositories::connector_person_renewals::{
    claim_person_renewal, keep_person_renewal, record_person_renewal_failure,
    remove_claimed_person_authorization,
};
use mimorya_db::repositories::connector_renewals::{
    Claimed, ClaimedRenewal, RenewalFor, Renewed, claim_connector_renewal, keep_connector_renewal,
    record_renewal_failure, remove_claimed_authorization,
};
use serde_json::json;
use uuid::Uuid;

use super::super::grants::{AUTHORIZATION_REMOVED, PERSON_REMOVED};
use super::super::{person_refresh_sealed_as, refresh_sealed_as};

/// What a refresh token the platform can no longer open says: sealed under
/// another master key, or the master key gone.
const REFRESH_UNREADABLE: &str = "Jeton de renouvellement illisible pour la plateforme (clé de \
                                  chiffrement changée ?) : autorisez de nouveau ce connecteur";

/// The same of a person's own: they authorize it again.
const PERSON_REFRESH_UNREADABLE: &str = "Jeton de renouvellement illisible pour la plateforme \
     (clé de chiffrement changée ?) : autorisez de nouveau ce serveur dans « Mon compte »";

/// Whose authorization a renewal is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Whose {
    Company,
    /// A person's own, on a server each person authorizes (ADR-174).
    Person(Uuid),
}

impl Whose {
    /// Who must authorize the server for this authorization to serve.
    pub(super) fn holder(self) -> OAuthHolder {
        match self {
            Self::Company => OAuthHolder::Company,
            Self::Person(_) => OAuthHolder::Person,
        }
    }

    /// The person it is of, if it is a person's.
    pub(super) fn person(self) -> Option<Uuid> {
        match self {
            Self::Company => None,
            Self::Person(user) => Some(user),
        }
    }

    pub(super) fn refresh_sealed_as(self, connector_id: Uuid) -> String {
        match self {
            Self::Company => refresh_sealed_as(connector_id),
            Self::Person(user) => person_refresh_sealed_as(connector_id, user),
        }
    }

    /// What a refresh token that no longer opens says to whoever authorized
    /// the server.
    pub(super) fn refresh_unreadable(self) -> &'static str {
        match self {
            Self::Company => REFRESH_UNREADABLE,
            Self::Person(_) => PERSON_REFRESH_UNREADABLE,
        }
    }

    /// The system's audit of its removal, its grant gone.
    pub(super) fn removed(self, tenant_id: Uuid) -> AuditEntry {
        match self {
            Self::Company => AuditEntry::new(tenant_id, "system", AUTHORIZATION_REMOVED, "success"),
            Self::Person(user) => AuditEntry::new(tenant_id, "system", PERSON_REMOVED, "success")
                .metadata(json!({ "person": user })),
        }
    }

    /// Claims its renewal of the connector for `hold`, if it is due within
    /// `within`.
    pub(super) async fn claim(
        self,
        tx: &mut TenantTx,
        connector_id: Uuid,
        (within, hold): (std::time::Duration, std::time::Duration),
        asker: RenewalFor,
    ) -> sqlx::Result<Option<Claimed>> {
        match self {
            Self::Company => claim_connector_renewal(tx, connector_id, within, hold, asker).await,
            Self::Person(user) => {
                claim_person_renewal(tx, connector_id, user, within, hold, asker).await
            }
        }
    }

    /// Keeps what the renewal claimed brought; whether it was kept.
    pub(super) async fn keep(
        self,
        tx: &mut TenantTx,
        key: ClaimedRenewal,
        renewed: Renewed<'_>,
    ) -> sqlx::Result<bool> {
        Ok(match self {
            Self::Company => keep_connector_renewal(tx, key, renewed).await?.is_some(),
            Self::Person(_) => keep_person_renewal(tx, key, renewed).await?,
        })
    }

    /// Says why the renewal claimed failed.
    pub(super) async fn record_failure(
        self,
        tx: &mut TenantTx,
        key: ClaimedRenewal,
        reason: &str,
    ) -> sqlx::Result<()> {
        match self {
            Self::Company => record_renewal_failure(tx, key, reason).await?,
            Self::Person(_) => record_person_renewal_failure(tx, key, reason).await?,
        };
        Ok(())
    }

    /// Removes the authorization claimed, its grant gone; whether it was.
    pub(super) async fn remove_claimed(
        self,
        tx: &mut TenantTx,
        key: ClaimedRenewal,
    ) -> sqlx::Result<bool> {
        match self {
            Self::Company => remove_claimed_authorization(tx, key).await,
            Self::Person(_) => remove_claimed_person_authorization(tx, key).await,
        }
    }
}
