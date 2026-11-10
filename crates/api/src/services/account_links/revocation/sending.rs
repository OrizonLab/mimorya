//! The grants let go of sent to their provider's revocation endpoint
//! (ADR-194), once their removal is committed, by a task of their own: the
//! request that let them go (a person, an administrator, the identity
//! provider through SCIM) may be abandoned, not the revocation. Each
//! connector's are sent four at a time and audited together, how many were
//! revoked, kept and not revoked, with the first reason, never whose nor a
//! token. A grant is kept, not sent, when its person has a link to the
//! same account at the connector again by then: the provider's grant is
//! theirs again. A
//! request waits for them a while; a company's release (ADR-193) waits for
//! every one, and says how each went.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use mimorya_core::account_links::sealed_as;
use mimorya_db::repositories::account_links::has_account_link_to;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;
use uuid::Uuid;

use super::endpoint::{revocation_endpoint, revoke_token};
use super::{Held, LinksLetGo};
use crate::services::account_links::AccountLinkService;

/// What the audit calls the revocation of a connector's links' grants.
const LINKS_REVOKED: &str = "account.links_revoked";
/// How many tokens of one connector are sent at once.
const AT_ONCE: usize = 4;
/// How long the request that let them go waits: a provider's answer is
/// bounded (`ProviderHttp`), discovery then a token, and a margin.
const WAITED: Duration = Duration::from_secs(25);
/// Said of what a revocation that stopped before its outcome did not revoke.
const STOPPED: &str = "La révocation s’est arrêtée avant son issue : retirez l’accord dans le \
                       compte du fournisseur";

/// How the revocation of one connector's grants went: counted, with the
/// first reason one was not revoked. Never a token nor whose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinksOutcome {
    pub connector_id: Uuid,
    pub revoked: usize,
    /// Not sent: their person had a link to the same account at the
    /// connector again.
    pub kept: usize,
    pub failed: usize,
    pub reason: Option<String>,
}

/// What became of one grant.
enum Sent {
    Revoked,
    Kept,
    Failed(String),
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
        let task = self.spawned(tenant_id, by, let_go);
        // Waited for a while only: the task goes on to its audit. One that
        // stopped before it is said in the log, never a token.
        if let Ok(Err(error)) = tokio::time::timeout(WAITED, task).await {
            tracing::error!(error = %error, "The revocation of linked accounts' grants stopped");
        }
    }

    /// `revoke`, waited for until every grant has its outcome: how each
    /// connector's went. A task that stopped says what it took not revoked.
    pub(crate) async fn revoke_waited(
        self: &Arc<Self>,
        tenant_id: Uuid,
        by: Option<Uuid>,
        let_go: Vec<LinksLetGo>,
    ) -> Vec<LinksOutcome> {
        let taken: Vec<(Uuid, usize)> = let_go
            .iter()
            .map(|links| (links.connector_id, links.held.len()))
            .collect();
        self.spawned(tenant_id, by, let_go)
            .await
            .unwrap_or_else(|error| {
                tracing::error!(error = %error, "The revocation of linked accounts' grants stopped");
                (taken.into_iter())
                    .map(|(connector_id, count)| LinksOutcome {
                        connector_id,
                        revoked: 0,
                        kept: 0,
                        failed: count,
                        reason: (count > 0).then(|| STOPPED.to_string()),
                    })
                    .collect()
            })
    }

    fn spawned(
        self: &Arc<Self>,
        tenant_id: Uuid,
        by: Option<Uuid>,
        let_go: Vec<LinksLetGo>,
    ) -> tokio::task::JoinHandle<Vec<LinksOutcome>> {
        let service = self.clone();
        tokio::spawn(async move {
            let mut outcomes = Vec::new();
            for links in let_go {
                outcomes.push(service.revoke_links(tenant_id, by, links).await);
            }
            outcomes
        })
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
        let id = links.connector_id;
        let sent: Vec<Sent> = match endpoint {
            Err(reason) => (links.held.iter())
                .map(|_| Sent::Failed(reason.clone()))
                .collect(),
            Ok(endpoint) => {
                // The futures made first: a stream's own `map` would ask
                // more of what they borrow than a spawned task can give.
                let sending: Vec<_> = (links.held.iter())
                    .map(|held| self.revoke_one(tenant_id, id, &endpoint, held, provider))
                    .collect();
                futures::stream::iter(sending)
                    .buffer_unordered(AT_ONCE)
                    .collect()
                    .await
            }
        };
        let outcome = counted(id, sent);
        self.audit(tenant_id, by, &outcome).await;
        outcome
    }

    /// One grant: kept if its person has a link to the same account at the
    /// connector again, otherwise its refresh token, opened under the name
    /// it was sealed as if a link kept it, sent to `endpoint`.
    async fn revoke_one(
        &self,
        tenant_id: Uuid,
        connector_id: Uuid,
        endpoint: &str,
        held: &Held,
        provider: &str,
    ) -> Sent {
        let again = self.linked_again(tenant_id, connector_id, held.user_id(), held.subject());
        match again.await {
            Ok(true) => return Sent::Kept,
            Ok(false) => {}
            Err(error) => {
                tracing::error!(error = format!("{error:#}"), "A link could not be read");
                return Sent::Failed(format!(
                    "Liaison illisible : retirez l’accord dans le compte {provider}"
                ));
            }
        }
        let token = match held {
            Held::Fresh { refresh_token, .. } => refresh_token.clone(),
            Held::Taken(link) => {
                let unreadable = || {
                    Sent::Failed(format!(
                        "Jeton de la liaison illisible (clé maîtresse du serveur changée ?) : \
                         retirez l’accord dans le compte {provider}"
                    ))
                };
                let Some(cipher) = self.cipher.as_deref() else {
                    return unreadable();
                };
                let name = sealed_as(link.connector_id, link.user_id);
                match cipher.open(tenant_id, &name, &link.refresh_token) {
                    Ok(token) => token,
                    Err(_) => return unreadable(),
                }
            }
        };
        match revoke_token(&self.http, endpoint, &token, provider).await {
            Ok(()) => Sent::Revoked,
            Err(reason) => Sent::Failed(reason),
        }
    }

    /// Whether the person has a link to the account `subject` at the
    /// connector again; an account not known is no link's.
    async fn linked_again(
        &self,
        tenant_id: Uuid,
        connector_id: Uuid,
        user_id: Uuid,
        subject: Option<&str>,
    ) -> anyhow::Result<bool> {
        let Some(subject) = subject else {
            return Ok(false);
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let linked = has_account_link_to(&mut tx, connector_id, user_id, subject).await?;
        tx.commit().await?;
        Ok(linked)
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
        let counts = json!({
            "revoked": outcome.revoked, "kept": outcome.kept, "failed": outcome.failed,
        });
        let mut entry = entry
            .resource("connector", outcome.connector_id)
            .metadata(counts);
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

/// The outcome of one connector's grants, the first reason bounded.
fn counted(connector_id: Uuid, sent: Vec<Sent>) -> LinksOutcome {
    let mut outcome = LinksOutcome {
        connector_id,
        revoked: 0,
        kept: 0,
        failed: 0,
        reason: None,
    };
    for sent in sent {
        match sent {
            Sent::Revoked => outcome.revoked += 1,
            Sent::Kept => outcome.kept += 1,
            Sent::Failed(reason) => {
                outcome.failed += 1;
                outcome
                    .reason
                    .get_or_insert_with(|| reason.chars().take(300).collect());
            }
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_outcome_counts_and_keeps_the_first_reason_bounded() {
        let id = Uuid::new_v4();
        let long = "x".repeat(400);
        let sent = vec![
            Sent::Revoked,
            Sent::Failed(long),
            Sent::Kept,
            Sent::Failed("autre".into()),
            Sent::Revoked,
        ];
        let outcome = counted(id, sent);
        assert_eq!((outcome.revoked, outcome.kept, outcome.failed), (2, 1, 2));
        assert_eq!(outcome.reason.as_deref().map(str::len), Some(300));
        let all = counted(id, vec![Sent::Revoked]);
        assert_eq!(
            (all.revoked, all.kept, all.failed, all.reason),
            (1, 0, 0, None)
        );
    }
}
