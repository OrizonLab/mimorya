//! A renewal claimed, with what calling the token endpoint needs (ADR-171,
//! ADR-174): only if the authorization is due, not claimed already, and
//! still under the declaration in force, for whoever that declaration says
//! authorizes the server.

use mimorya_connectors::mcp::RENEW_WITHIN;
use mimorya_connectors::mcp::oauth::{ClientAuth, OAuthDeclaration};
use mimorya_db::repositories::connector_renewals::{Claimed, RenewalFor};
use mimorya_db::repositories::connectors::get_connector_instance;
use uuid::Uuid;

use super::super::credential::{given_under, oauth_of};
use super::super::{ConnectorService, client_secret_name};
use super::{RENEWAL_HOLD, Whose};
use crate::secrets::SecretRead;

/// What a claimed renewal needs to call the token endpoint.
pub(super) struct Call {
    pub claimed: Claimed,
    pub declared: OAuthDeclaration,
    pub secret: Option<String>,
}

impl ConnectorService {
    /// The renewal claimed, with what calling the token endpoint needs, if
    /// it is due, not claimed already, and still under the declaration in
    /// force, for whoever it says authorizes the server.
    pub(super) async fn claim(
        &self,
        tenant_id: Uuid,
        connector_id: Uuid,
        whose: Whose,
        asker: RenewalFor,
    ) -> anyhow::Result<Option<Call>> {
        let mut tx = self.db.begin(tenant_id).await?;
        let bounds = (RENEW_WITHIN, RENEWAL_HOLD);
        let Some(claimed) = whose.claim(&mut tx, connector_id, bounds, asker).await? else {
            return Ok(None);
        };
        let instance = get_connector_instance(&mut tx, connector_id).await?;
        let declared = instance.as_ref().and_then(oauth_of).filter(|declared| {
            declared.holder == whose.holder()
                && given_under(
                    declared,
                    &claimed.issuer,
                    &claimed.client_id,
                    &claimed.resource,
                )
        });
        // Never sent under another declaration than the one it was given
        // under, nor for another holder than the declaration's (ADR-174):
        // the company's would never be used where each person authorizes,
        // nor a person's where the company does.
        let Some(declared) = declared else {
            tx.commit().await?;
            return Ok(None);
        };
        let secret = match declared.client_auth {
            ClientAuth::None => None,
            _ => match self
                .secrets
                .read(&mut tx, tenant_id, &client_secret_name(connector_id))
                .await?
            {
                SecretRead::Value(secret) => Some(secret),
                _ => None,
            },
        };
        tx.commit().await?;
        Ok(Some(Call {
            claimed,
            declared,
            secret,
        }))
    }
}
