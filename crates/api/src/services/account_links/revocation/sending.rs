//! The grants let go of sent to their provider's revocation endpoint
//! (ADR-194), once their removal is committed, by a task of their own: the
//! request that let them go (a person, an administrator, the identity
//! provider through SCIM) may be abandoned, not the revocation. Each
//! connector's are sent four at a time and audited together, how many were
//! revoked and how many not, with the first reason, never whose nor a
//! token. The request waits for them a while.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use mimorya_core::account_links::sealed_as;
use mimorya_db::repositories::account_links::TakenAccountLink;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;
use uuid::Uuid;

use super::LinksLetGo;
use super::endpoint::{revocation_endpoint, revoke_token};
use crate::services::account_links::AccountLinkService;

/// What the audit calls the revocation of a connector's links' grants.
pub const LINKS_REVOKED: &str = "account.links_revoked";
/// How many tokens of one connector are sent at once.
const AT_ONCE: usize = 4;
/// How long the request that let them go waits: a provider's answer is
/// bounded (`ProviderHttp`), discovery then a token, and a margin.
const WAITED: Duration = Duration::from_secs(25);

/// How the revocation of one connector's links went: counted, with the
/// first reason one was not revoked. Never a token nor whose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinksOutcome {
    pub connector_id: Uuid,
    pub revoked: usize,
    pub failed: usize,
    pub reason: Option<String>,
}

impl AccountLinkService {
    /// Asks the providers to revoke the grants `let_go`, once their removal
    /// is committed, audited as let go by `by` (or by the platform), and
    /// waits a while. Never fails: the removal stands whatever happens.
    pub(crate) async fn revoke(
        self: &Arc<Self>,
        tenant_id: Uuid,
        by: Option<Uuid>,
        let_go: Vec<LinksLetGo>,
    ) {
        if let_go.is_empty() {
            return;
        }
        let service = self.clone();
        let task = tokio::spawn(async move {
            for links in let_go {
                service.revoke_links(tenant_id, by, links).await;
            }
        });
        // Waited for a while only: the task goes on to its audit.
        let _ = tokio::time::timeout(WAITED, task).await;
    }

    async fn revoke_links(
        &self,
        tenant_id: Uuid,
        by: Option<Uuid>,
        links: LinksLetGo,
    ) -> LinksOutcome {
        let provider = links.kind.provider();
        let endpoint = match &links.issuer {
            Ok(issuer) => revocation_endpoint(&self.http, issuer, provider).await,
            Err(reason) => Err(reason.clone()),
        };
        let sent: Vec<Result<(), String>> = match endpoint {
            Err(reason) => links.taken.iter().map(|_| Err(reason.clone())).collect(),
            Ok(endpoint) => {
                // The futures made first: a stream's own `map` would ask
                // more of what they borrow than a spawned task can give.
                let sending: Vec<_> = (links.taken.iter())
                    .map(|link| self.revoke_one(tenant_id, &endpoint, link, provider))
                    .collect();
                futures::stream::iter(sending)
                    .buffer_unordered(AT_ONCE)
                    .collect()
                    .await
            }
        };
        let outcome = counted(links.connector_id, sent);
        self.audit(tenant_id, by, &outcome).await;
        outcome
    }

    /// One link's refresh token, opened under the name it was sealed as,
    /// sent to `endpoint`.
    async fn revoke_one(
        &self,
        tenant_id: Uuid,
        endpoint: &str,
        link: &TakenAccountLink,
        provider: &str,
    ) -> Result<(), String> {
        let unreadable = || {
            format!(
                "Jeton de la liaison illisible (clé maîtresse du serveur changée ?) : retirez \
                 l’accord dans le compte {provider}"
            )
        };
        let cipher = self.cipher.as_deref().ok_or_else(unreadable)?;
        let name = sealed_as(link.connector_id, link.user_id);
        let token =
            (cipher.open(tenant_id, &name, &link.refresh_token)).map_err(|_| unreadable())?;
        revoke_token(&self.http, endpoint, &token, provider).await
    }

    /// Writes how it went; a failure is logged, never a token.
    async fn audit(&self, tenant_id: Uuid, by: Option<Uuid>, outcome: &LinksOutcome) {
        let status = if outcome.failed == 0 {
            "success"
        } else {
            "failure"
        };
        let entry = match by {
            Some(user_id) => {
                AuditEntry::new(tenant_id, "user", LINKS_REVOKED, status).by_user(user_id)
            }
            None => AuditEntry::new(tenant_id, "system", LINKS_REVOKED, status),
        };
        let mut entry = entry
            .resource("connector", outcome.connector_id)
            .metadata(json!({ "revoked": outcome.revoked, "failed": outcome.failed }));
        if let Some(reason) = &outcome.reason {
            entry = entry.reason(reason.clone());
        }
        let written = async {
            let mut tx = self.db.begin(tenant_id).await?;
            append_audit(&mut tx, entry).await?;
            tx.commit().await?;
            anyhow::Ok(())
        };
        if let Err(error) = written.await {
            tracing::error!(
                connector = %outcome.connector_id,
                error = format!("{error:#}"),
                "Failed to audit the revocation of linked accounts' grants"
            );
        }
    }
}

/// The outcome of one connector's tokens sent, the first reason bounded.
fn counted(connector_id: Uuid, sent: Vec<Result<(), String>>) -> LinksOutcome {
    let failures: Vec<String> = sent.iter().filter_map(|s| s.clone().err()).collect();
    LinksOutcome {
        connector_id,
        revoked: sent.len() - failures.len(),
        failed: failures.len(),
        reason: failures
            .first()
            .map(|reason| reason.chars().take(300).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_outcome_counts_and_keeps_the_first_reason_bounded() {
        let id = Uuid::new_v4();
        let long = "x".repeat(400);
        let outcome = counted(id, vec![Ok(()), Err(long), Err("autre".into()), Ok(())]);
        assert_eq!((outcome.revoked, outcome.failed), (2, 2));
        assert_eq!(outcome.reason.as_deref().map(str::len), Some(300));
        let all = counted(id, vec![Ok(())]);
        assert_eq!((all.revoked, all.failed, all.reason), (1, 0, None));
    }
}
