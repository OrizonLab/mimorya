//! The tokens of an authorization the administration lets go of, revoked at
//! its authorization server (RFC 7009, ADR-172). They are taken, with what
//! revoking them needs, in the transaction that withdraws the authorization,
//! before the client's secret goes with a declaration: the declaration it
//! was given under, if it still is the one in force. Once that transaction
//! is committed and nothing is held, they are sent to the revocation
//! endpoint the declaration names, the refresh token first, within a bound,
//! by a task of their own that goes on to its audit if the request that
//! withdrew them is abandoned. How it went is audited, never a token: a
//! server that names no revocation endpoint, does not answer or refuses
//! keeps its grant, to be withdrawn there; the withdrawal stands.

use mimorya_connectors::mcp::oauth::{ClientAuth, NO_REVOCATION_ENDPOINT, OAuthDeclaration};
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::connector_authorizations::{
    TakenAuthorization, take_connector_authorization,
};
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

use super::super::connectors::ConnectorAdmin;
use crate::secrets::{SecretRead, SecretStore};
use crate::services::AccessContext;
use crate::services::connectors::{
    LetGo, access_sealed_as, client_secret_name, given_under, refresh_sealed_as, revoke_within,
};

/// What the audit calls a revocation asked of the authorization server.
const AUTHORIZATION_REVOKED: &str = "connector.oauth_authorization_revoked";

/// What revoking needs to know of the declaration in force.
enum Declared {
    /// The one the authorization was given under.
    Under(OAuthDeclaration),
    /// Another one: its server is never sent these tokens.
    Other,
    /// None the platform can read.
    Unreadable,
}

/// An authorization taken away, with what revoking its tokens needs.
pub struct Revocation {
    connector_id: Uuid,
    declared: Declared,
    secret: Option<String>,
    taken: TakenAuthorization,
}

/// Takes the connector's authorization in `tx`, if it has one, with what
/// revoking it needs under `declared`, the declaration in force before the
/// change (`None` if it cannot be read): to be called before the client's
/// secret is replaced or removed.
pub async fn take_authorization(
    tx: &mut TenantTx,
    admin: &ConnectorAdmin<'_>,
    tenant_id: Uuid,
    connector_id: Uuid,
    declared: Option<&OAuthDeclaration>,
) -> anyhow::Result<Option<Revocation>> {
    let Some(taken) = take_connector_authorization(tx, connector_id).await? else {
        return Ok(None);
    };
    // Never sent under another declaration than the one it was given under.
    let declared = match declared {
        None => Declared::Unreadable,
        Some(declared)
            if given_under(declared, &taken.issuer, &taken.client_id, &taken.resource) =>
        {
            Declared::Under(declared.clone())
        }
        Some(_) => Declared::Other,
    };
    let secret = match &declared {
        Declared::Under(declared) if declared.client_auth != ClientAuth::None => {
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
        declared,
        secret,
        taken,
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
    let revoking = Revoking {
        db: admin.db.clone(),
        secrets: admin.secrets.clone(),
        network: admin.connectors.network(),
        tenant_id: access.subject.tenant_id,
        user_id: access.subject.user_id,
        revocation,
    };
    // A task of its own: the request may be abandoned, not the revocation.
    if let Err(error) = tokio::spawn(revoking.run()).await {
        tracing::error!(error = %error, "The revocation of a connector's tokens stopped");
    }
}

impl Revoking {
    async fn run(self) {
        let id = self.revocation.connector_id;
        let outcome = match &self.revocation.declared {
            Declared::Unreadable => Err("Déclaration OAuth illisible : retirez l’accord chez le \
                                         serveur d’autorisation"
                .to_string()),
            Declared::Other => Err("Autorisation donnée sous une autre déclaration : retirez \
                                    l’accord chez le serveur d’autorisation"
                .to_string()),
            Declared::Under(declared) if declared.revocation_endpoint.is_none() => {
                Err(NO_REVOCATION_ENDPOINT.to_string())
            }
            Declared::Under(declared)
                if declared.client_auth != ClientAuth::None && self.revocation.secret.is_none() =>
            {
                Err(
                    "Secret du client absent ou illisible : retirez l’accord chez le serveur \
                     d’autorisation"
                        .to_string(),
                )
            }
            Declared::Under(declared) => self.send(declared).await,
        };
        let entry = |outcome: &'static str| {
            AuditEntry::new(self.tenant_id, "user", AUTHORIZATION_REVOKED, outcome)
                .by_user(self.user_id)
                .resource("connector", id)
        };
        let entry = match outcome {
            Ok(()) => entry("success"),
            Err(reason) => entry("failure").reason(reason.chars().take(300).collect::<String>()),
        };
        if let Err(error) = self.audit(entry).await {
            tracing::error!(
                connector = %id,
                error = format!("{error:#}"),
                "Failed to audit the revocation of a connector's tokens"
            );
        }
    }

    async fn audit(&self, entry: AuditEntry) -> anyhow::Result<()> {
        let mut tx = self.db.begin(self.tenant_id).await?;
        append_audit(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(())
    }

    /// The tokens opened and sent.
    async fn send(&self, declared: &OAuthDeclaration) -> Result<(), String> {
        let unreadable = || {
            "Jetons illisibles pour la plateforme : retirez l’accord chez le serveur \
             d’autorisation"
                .to_string()
        };
        let cipher = self.secrets.cipher().map_err(|_| unreadable())?;
        let (tenant_id, id) = (self.tenant_id, self.revocation.connector_id);
        let taken = &self.revocation.taken;
        let access = cipher
            .open(tenant_id, &access_sealed_as(id), &taken.access)
            .map_err(|_| unreadable())?;
        let refresh = match &taken.refresh {
            Some(sealed) => Some(
                cipher
                    .open(tenant_id, &refresh_sealed_as(id), sealed)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration(issuer: &str, revocation: Option<&str>) -> OAuthDeclaration {
        OAuthDeclaration {
            revocation_endpoint: revocation.map(String::from),
            registered: None,
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
