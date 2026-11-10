//! Accounts linked at a provider that revokes one grant (Google, ADR-194),
//! the platform letting go of them: taken, with their sealed refresh
//! tokens, in the transaction that removes them (the person's link
//! removed, their account disabled, the connector deleted, the company
//! released), with where their people signed in, read before the connector
//! may go; and the fresh refresh token of a return refused once its code
//! was redeemed, which no link keeps. Once that transaction is committed,
//! they are sent to the revocation endpoint the provider's discovery names
//! (`endpoint`), by a task of their own that goes on to its audit if the
//! request is abandoned (`sending`). How it went is audited per connector,
//! counted, never a token nor whose: a provider that does not answer,
//! names no endpoint or refuses keeps the grant, which the person withdraws
//! there; the removal stands. A link a new one replaces is not revoked:
//! at Google, the grant is the same, and revoking the old token would take
//! the new one with it; for the same reason, nothing is sent for a person
//! who has a link to the same account at the connector again by then.
//! Microsoft revokes no single grant (`LinkKind::revokes`): its links are
//! only removed.

mod endpoint;
mod sending;

pub use sending::LinksOutcome;

use std::collections::BTreeMap;

use mimorya_core::account_links::LinkKind;
use mimorya_db::TenantTx;
use mimorya_db::repositories::account_links::{TakenAccountLink, take_account_links_of_connector};
use mimorya_db::repositories::connectors::{get_connector_instance, list_connector_instances};
use uuid::Uuid;

use super::AccountLinkService;
use super::application::LinkApp;

/// A grant let go of, and the token that revokes it.
enum Held {
    /// A link taken away, its refresh token sealed for it.
    Taken(TakenAccountLink),
    /// A return refused once its code was redeemed: its refresh token, in
    /// hand, kept by no link, and the account its token named, if it was
    /// verified.
    Fresh {
        user_id: Uuid,
        subject: Option<String>,
        refresh_token: String,
    },
}

impl Held {
    fn user_id(&self) -> Uuid {
        match self {
            Self::Taken(link) => link.user_id,
            Self::Fresh { user_id, .. } => *user_id,
        }
    }

    /// The provider's account whose grant it is, if known.
    fn subject(&self) -> Option<&str> {
        match self {
            Self::Taken(link) => Some(&link.subject),
            Self::Fresh { subject, .. } => subject.as_deref(),
        }
    }
}

/// The grants of one connector let go of, to revoke at its provider.
pub struct LinksLetGo {
    connector_id: Uuid,
    kind: LinkKind,
    /// Where its people signed in, or why it cannot be read (in French).
    issuer: Result<String, String>,
    held: Vec<Held>,
}

impl LinksLetGo {
    /// The connector whose grants they are.
    pub fn connector_id(&self) -> Uuid {
        self.connector_id
    }

    /// How many grants are let go of.
    pub fn count(&self) -> usize {
        self.held.len()
    }
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
                Err(_) => Err(unreadable_application(kind)),
            };
            let held = taken.into_iter().map(Held::Taken).collect();
            let_go.push(LinksLetGo {
                connector_id,
                kind,
                issuer,
                held,
            });
        }
        Ok(let_go)
    }

    /// Every link of the company in `tx` at a connector whose provider
    /// revokes a grant, taken (its release, ADR-193).
    pub(crate) async fn take_company_links(
        &self,
        tx: &mut TenantTx,
    ) -> anyhow::Result<Vec<LinksLetGo>> {
        let revoking: Vec<Uuid> = list_connector_instances(tx, false)
            .await?
            .into_iter()
            .filter(|instance| {
                LinkKind::of(&instance.connector_type).is_some_and(LinkKind::revokes)
            })
            .map(|instance| instance.id)
            .collect();
        let mut taken = Vec::new();
        for connector_id in revoking {
            taken.extend(take_account_links_of_connector(tx, connector_id).await?);
        }
        self.let_go_of(tx, taken).await
    }

    /// The fresh grant of a return refused once its code was redeemed, at
    /// the application `app` of `connector_id`, if its provider revokes
    /// one: of the account `subject` its token named, if it was verified.
    pub(super) fn refused_return(
        &self,
        app: &LinkApp,
        connector_id: Uuid,
        user_id: Uuid,
        subject: Option<String>,
        refresh_token: String,
    ) -> Option<LinksLetGo> {
        app.kind.revokes().then(|| LinksLetGo {
            connector_id,
            kind: app.kind,
            issuer: Ok(self.issuer_of(app).to_string()),
            held: vec![Held::Fresh {
                user_id,
                subject,
                refresh_token,
            }],
        })
    }
}

fn unreadable_application(kind: LinkKind) -> String {
    format!(
        "Déclaration de l’application {} illisible : retirez l’accord dans le compte {}",
        kind.product(),
        kind.provider()
    )
}
