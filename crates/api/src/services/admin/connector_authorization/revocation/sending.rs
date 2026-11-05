//! The tokens taken sent to the revocation endpoint (ADR-172), by a task of
//! their own, once the withdrawal is committed: the company's authorization
//! audited as one, the people's (ADR-174) four at a time and audited
//! together, how many were revoked and how many not, never who nor a token.

use futures::StreamExt;
use mimorya_connectors::mcp::oauth::{ClientAuth, NO_REVOCATION_ENDPOINT, OAuthDeclaration};
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::connector_authorizations::TakenAuthorization;
use serde_json::json;
use uuid::Uuid;

use super::super::super::connectors::ConnectorAdmin;
use super::{Revocation, letting_go};
use crate::secrets::SecretStore;
use crate::services::AccessContext;
use crate::services::connectors::{
    LetGo, access_sealed_as, given_under, person_access_sealed_as, person_refresh_sealed_as,
    refresh_sealed_as, revoke_within,
};

/// What the audit calls a revocation asked of the authorization server.
const AUTHORIZATION_REVOKED: &str = "connector.oauth_authorization_revoked";
/// And those of the people's authorizations, together.
const PERSON_AUTHORIZATIONS_REVOKED: &str = "connector.oauth_person_authorizations_revoked";
/// How many people's authorizations are revoked at once.
const PERSONS_AT_ONCE: usize = 4;
/// How long the request that let them go waits for the revocations: the
/// bound of one, and a margin (ADR-172).
const WAITED: std::time::Duration = std::time::Duration::from_secs(15);

/// What the task that revokes owns.
struct Revoking {
    db: Database,
    secrets: SecretStore,
    network: NetworkPolicy,
    tenant_id: Uuid,
    user_id: Uuid,
    revocation: Revocation,
}

/// Asks the authorization server to revoke what was taken, once the
/// withdrawal is committed, and audits how it went, never a token. Never
/// fails: the withdrawal stands whatever happens.
pub async fn revoke(admin: &ConnectorAdmin<'_>, access: &AccessContext, revocation: Revocation) {
    let (tenant_id, user_id) = letting_go(access);
    let revoking = Revoking {
        db: admin.db.clone(),
        secrets: admin.secrets.clone(),
        network: admin.connectors.network(),
        tenant_id,
        user_id,
        revocation,
    };
    // A task of its own: the request may be abandoned, not the revocation.
    // Waited for a while only: many people's tokens at a server that does
    // not answer go on being revoked, and audited, after the answer.
    match tokio::time::timeout(WAITED, tokio::spawn(revoking.run())).await {
        Ok(Ok(())) | Err(_) => {}
        Ok(Err(error)) => {
            tracing::error!(error = %error, "The revocation of a connector's tokens stopped");
        }
    }
}

impl Revoking {
    async fn run(self) {
        let id = self.revocation.connector_id;
        let entry = |action: &'static str, outcome: &'static str| {
            AuditEntry::new(self.tenant_id, "user", action, outcome)
                .by_user(self.user_id)
                .resource("connector", id)
        };
        let mut entries = Vec::new();
        if let Some(company) = &self.revocation.company {
            let names = (access_sealed_as(id), refresh_sealed_as(id));
            entries.push(match self.send(company, names).await {
                Ok(()) => entry(AUTHORIZATION_REVOKED, "success"),
                Err(reason) => entry(AUTHORIZATION_REVOKED, "failure").reason(bounded(&reason)),
            });
        }
        if !self.revocation.persons.is_empty() {
            // The futures made first: a stream's own `map` would ask more of
            // what they borrow than a spawned task can give.
            let sending: Vec<_> = (self.revocation.persons.iter())
                .map(|person| {
                    let user = person.user_id;
                    let names = (
                        person_access_sealed_as(id, user),
                        person_refresh_sealed_as(id, user),
                    );
                    self.send(&person.taken, names)
                })
                .collect();
            let sent = futures::stream::iter(sending)
                .buffer_unordered(PERSONS_AT_ONCE)
                .collect::<Vec<_>>()
                .await;
            let failures: Vec<String> = sent.into_iter().filter_map(Result::err).collect();
            let revoked = self.revocation.persons.len() - failures.len();
            let counts = json!({ "revoked": revoked, "failed": failures.len() });
            entries.push(match failures.first() {
                None => entry(PERSON_AUTHORIZATIONS_REVOKED, "success").metadata(counts),
                Some(reason) => entry(PERSON_AUTHORIZATIONS_REVOKED, "failure")
                    .reason(bounded(reason))
                    .metadata(counts),
            });
        }
        if let Err(error) = self.audit(entries).await {
            tracing::error!(
                connector = %id,
                error = format!("{error:#}"),
                "Failed to audit the revocation of a connector's tokens"
            );
        }
    }

    async fn audit(&self, entries: Vec<AuditEntry>) -> anyhow::Result<()> {
        let mut tx = self.db.begin(self.tenant_id).await?;
        for entry in entries {
            append_audit(&mut tx, entry).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// One authorization's tokens, opened under the names they were sealed
    /// as (access, refresh), sent if the declaration in force is the one
    /// they were given under.
    async fn send(
        &self,
        taken: &TakenAuthorization,
        names: (String, String),
    ) -> Result<(), String> {
        let revocation = &self.revocation;
        let declared = match &revocation.declared {
            None => {
                return Err(
                    "Déclaration OAuth illisible : retirez l’accord chez le serveur \
                            d’autorisation"
                        .to_string(),
                );
            }
            Some(declared)
                if !given_under(declared, &taken.issuer, &taken.client_id, &taken.resource) =>
            {
                return Err(
                    "Autorisation donnée sous une autre déclaration : retirez l’accord \
                            chez le serveur d’autorisation"
                        .to_string(),
                );
            }
            Some(declared) => declared,
        };
        if declared.revocation_endpoint.is_none() {
            return Err(NO_REVOCATION_ENDPOINT.to_string());
        }
        if declared.client_auth != ClientAuth::None && revocation.secret.is_none() {
            return Err(
                "Secret du client absent ou illisible : retirez l’accord chez le serveur \
                        d’autorisation"
                    .to_string(),
            );
        }
        self.opened_and_sent(declared, taken, names).await
    }

    async fn opened_and_sent(
        &self,
        declared: &OAuthDeclaration,
        taken: &TakenAuthorization,
        (access_name, refresh_name): (String, String),
    ) -> Result<(), String> {
        let unreadable = || {
            "Jetons illisibles pour la plateforme : retirez l’accord chez le serveur \
             d’autorisation"
                .to_string()
        };
        let cipher = self.secrets.cipher().map_err(|_| unreadable())?;
        let tenant_id = self.tenant_id;
        let access = cipher
            .open(tenant_id, &access_name, &taken.access)
            .map_err(|_| unreadable())?;
        let refresh = match &taken.refresh {
            Some(sealed) => Some(
                cipher
                    .open(tenant_id, &refresh_name, sealed)
                    .map_err(|_| unreadable())?,
            ),
            None => None,
        };
        let tokens = LetGo {
            access: &access,
            refresh: refresh.as_deref(),
        };
        let secret = self.revocation.secret.as_deref();
        revoke_within(declared, tokens, secret, self.network).await
    }
}

/// A reason as the audit keeps it.
fn bounded(reason: &str) -> String {
    reason.chars().take(300).collect()
}
