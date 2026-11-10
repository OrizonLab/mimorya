//! Accounts linked at a provider that revokes one grant (Google, ADR-194),
//! the platform letting go of them: taken, with their sealed refresh
//! tokens, in the transaction that removes them (the person's link
//! removed, their account disabled, the connector deleted), with where
//! their people signed in, read before the connector may go. Once that
//! transaction is committed, they are sent to the revocation endpoint the
//! provider's discovery names (`endpoint`), by a task of their own that
//! goes on to its audit if the request is abandoned (`sending`). How it
//! went is audited per connector, counted, never a token nor whose: a
//! provider that does not answer, names no endpoint or refuses keeps the
//! grant, which the person withdraws there; the removal stands. A link a
//! new one replaces is not revoked: at Google, the grant is the same, and
//! revoking the old token would take the new one with it. Microsoft
//! revokes no single grant (`LinkKind::revokes`): its links are only
//! removed.

mod endpoint;
mod sending;

use std::collections::BTreeMap;

use mimorya_core::account_links::LinkKind;
use mimorya_db::TenantTx;
use mimorya_db::repositories::account_links::TakenAccountLink;
use mimorya_db::repositories::connectors::get_connector_instance;
use uuid::Uuid;

use super::AccountLinkService;
use super::application::LinkApp;

/// The links of one connector let go of, their grants to revoke at its
/// provider.
pub struct LinksLetGo {
    connector_id: Uuid,
    kind: LinkKind,
    /// Where its people signed in, or why it cannot be read (in French).
    issuer: Result<String, String>,
    taken: Vec<TakenAccountLink>,
}

impl AccountLinkService {
    /// Of the links taken in `tx`, those whose provider revokes a grant,
    /// by connector, with where its people signed in: to be called before
    /// the connector goes.
    pub(crate) async fn let_go_of(
        &self,
        tx: &mut TenantTx,
        taken: Vec<TakenAccountLink>,
    ) -> anyhow::Result<Vec<LinksLetGo>> {
        let mut by_connector: BTreeMap<Uuid, Vec<TakenAccountLink>> = BTreeMap::new();
        for link in taken {
            by_connector
                .entry(link.connector_id)
                .or_default()
                .push(link);
        }
        let mut let_go = Vec::new();
        for (connector_id, taken) in by_connector {
            // A link goes with its connector: one taken has its connector.
            let Some(instance) = get_connector_instance(tx, connector_id).await? else {
                continue;
            };
            let Some(kind) = LinkKind::of(&instance.connector_type) else {
                continue;
            };
            if !kind.revokes() {
                continue;
            }
            let issuer = match LinkApp::read(&instance) {
                Ok(app) => Ok(self.issuer_of(&app).to_string()),
                Err(_) => Err(format!(
                    "Déclaration de l’application {} illisible : retirez l’accord dans le \
                     compte {}",
                    kind.product(),
                    kind.provider()
                )),
            };
            let_go.push(LinksLetGo {
                connector_id,
                kind,
                issuer,
                taken,
            });
        }
        Ok(let_go)
    }
}
