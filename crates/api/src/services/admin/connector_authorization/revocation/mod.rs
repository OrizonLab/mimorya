//! The tokens of authorizations the administration lets go of, revoked at
//! their authorization server (RFC 7009, ADR-172): the company's, and each
//! person's when the declaration they were given under changes or the
//! connector goes (ADR-174). They are taken, with what revoking them needs,
//! in the transaction that withdraws them, before the client's secret goes
//! with a declaration: the declaration they were given under, if it still
//! is the one in force. Once that transaction is committed and nothing is
//! held, they are sent to the revocation endpoint the declaration names,
//! the refresh token first, within a bound, by a task of their own that goes
//! on to its audit if the request that withdrew them is abandoned
//! (`sending`). How it went is audited, never a token: a server that names
//! no revocation endpoint, does not answer or refuses keeps its grant, to be
//! withdrawn there; the withdrawal stands.

mod persons;
mod sending;

use mimorya_connectors::mcp::oauth::{ClientAuth, OAuthDeclaration};
use mimorya_db::TenantTx;
use mimorya_db::repositories::connector_authorizations::{
    TakenAuthorization, take_connector_authorization,
};
use mimorya_db::repositories::connector_person_authorizations::{
    TakenPersonAuthorization, take_person_authorizations_of_connector,
};
use uuid::Uuid;

use super::super::connectors::ConnectorAdmin;
use crate::secrets::SecretRead;
use crate::services::AccessContext;
use crate::services::connectors::client_secret_name;

pub use persons::{take_person, take_persons_of_user};
pub use sending::{revoke, revoke_all};

/// Authorizations of one connector taken away, with what revoking their
/// tokens needs.
pub struct Revocation {
    pub(in crate::services::admin) connector_id: Uuid,
    /// The declaration in force before the change, if it can be read: each
    /// authorization's tokens go only to the server it was given under.
    declared: Option<OAuthDeclaration>,
    secret: Option<String>,
    company: Option<TakenAuthorization>,
    persons: Vec<TakenPersonAuthorization>,
}

impl Revocation {
    /// Whether the company's authorization was taken.
    pub fn company_taken(&self) -> bool {
        self.company.is_some()
    }

    /// How many people's authorizations were taken.
    pub fn persons_taken(&self) -> usize {
        self.persons.len()
    }
}

/// Takes the connector's authorization in `tx`, if it has one, with what
/// revoking it needs under `declared`, the declaration in force before the
/// change (`None` if it cannot be read): to be called before the client's
/// secret is replaced or removed. The people's own authorizations stay.
pub async fn take_authorization(
    tx: &mut TenantTx,
    admin: &ConnectorAdmin<'_>,
    tenant_id: Uuid,
    connector_id: Uuid,
    declared: Option<&OAuthDeclaration>,
) -> anyhow::Result<Option<Revocation>> {
    let company = take_connector_authorization(tx, connector_id).await?;
    taken(
        tx,
        admin,
        tenant_id,
        connector_id,
        declared,
        company,
        Vec::new(),
    )
    .await
}

/// Takes the company's authorization and every person's (ADR-174), the
/// declaration they were given under changing or going, whoever it said
/// authorizes: a process that knew only the company's may have written it.
pub async fn take_authorizations(
    tx: &mut TenantTx,
    admin: &ConnectorAdmin<'_>,
    tenant_id: Uuid,
    connector_id: Uuid,
    declared: Option<&OAuthDeclaration>,
) -> anyhow::Result<Option<Revocation>> {
    let company = take_connector_authorization(tx, connector_id).await?;
    let persons = take_person_authorizations_of_connector(tx, connector_id).await?;
    taken(
        tx,
        admin,
        tenant_id,
        connector_id,
        declared,
        company,
        persons,
    )
    .await
}

/// What was taken, with the client's secret read before it goes, if it is
/// to be sent: `None` if nothing was.
async fn taken(
    tx: &mut TenantTx,
    admin: &ConnectorAdmin<'_>,
    tenant_id: Uuid,
    connector_id: Uuid,
    declared: Option<&OAuthDeclaration>,
    company: Option<TakenAuthorization>,
    persons: Vec<TakenPersonAuthorization>,
) -> anyhow::Result<Option<Revocation>> {
    if company.is_none() && persons.is_empty() {
        return Ok(None);
    }
    let secret = match declared {
        Some(declared) if declared.client_auth != ClientAuth::None => {
            let name = client_secret_name(connector_id);
            match admin.secrets.read(tx, tenant_id, &name).await? {
                SecretRead::Value(secret) => Some(secret),
                _ => None,
            }
        }
        _ => None,
    };
    Ok(Some(Revocation {
        connector_id,
        declared: declared.cloned(),
        secret,
        company,
        persons,
    }))
}

/// `previous`, the declaration an authorization was given under, revoked
/// at the endpoint `new` names when it names none and both are the same
/// server's: a declaration kept before revocation endpoints were (ADR-172).
pub fn revocable_under(previous: &OAuthDeclaration, new: &OAuthDeclaration) -> OAuthDeclaration {
    let mut revocable = previous.clone();
    if revocable.revocation_endpoint.is_none() && previous.issuer == new.issuer {
        revocable.revocation_endpoint = new.revocation_endpoint.clone();
    }
    revocable
}

/// Who let them go, for the audit.
pub(super) fn letting_go(access: &AccessContext) -> (Uuid, Uuid) {
    (access.subject.tenant_id, access.subject.user_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration(issuer: &str, revocation: Option<&str>) -> OAuthDeclaration {
        OAuthDeclaration {
            revocation_endpoint: revocation.map(String::from),
            registered: None,
            holder: Default::default(),
            issuer: issuer.into(),
            resource: "https://mcp.example.com/mcp".into(),
            authorization_endpoint: format!("{issuer}/authorize"),
            token_endpoint: format!("{issuer}/token"),
            names_issuer_on_return: false,
            client_id: "mimorya".into(),
            client_auth: ClientAuth::ClientSecretPost,
            scopes: Vec::new(),
        }
    }

    #[test]
    fn a_former_declaration_takes_only_its_own_server_s_revocation_endpoint() {
        let before = declaration("https://a.example", None);
        // Its server now names one: taken.
        let now = declaration("https://a.example", Some("https://a.example/revoke"));
        let revocable = revocable_under(&before, &now);
        assert_eq!(
            revocable.revocation_endpoint.as_deref(),
            Some("https://a.example/revoke")
        );
        // Another server's: never, its tokens and its client's secret are
        // not to be sent there.
        let elsewhere = declaration("https://b.example", Some("https://b.example/revoke"));
        assert_eq!(revocable_under(&before, &elsewhere), before);
        // One it named already: kept.
        let named = declaration("https://a.example", Some("https://a.example/revoquer"));
        assert_eq!(revocable_under(&named, &now), named);
    }
}
