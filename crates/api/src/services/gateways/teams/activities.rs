//! What the Bot Framework sends a company's Teams bot (ADR-138), before any
//! session: the request names its gateway, then proves itself by the token
//! in its `Authorization` header, signed by the Bot Framework with a key it
//! publishes, endorsed for Teams, for the bot's application, naming the
//! service the activity came from (`verify_token`). An unknown gateway, an
//! activity not read, a token missing or proving nothing, and a gateway
//! whose keys could not be read answer alike. Once proven: an activity of a
//! paused gateway, of another directory, of anything but a person's own
//! message to the bot in their personal conversation, naming a service not
//! Microsoft's Teams, or already taken (the Bot Framework sends it again
//! when it was not answered in time) is taken and ignored. A person's own
//! message is answered: from someone who has not linked their Teams
//! identity, with a link to do so (`link`); from a linked person, by
//! saying the agent is not talked with in Teams yet. What a message says is
//! never logged nor audited.

use chrono::{DateTime, Utc};
use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::teams::{
    Expected, TeamsApplication, bearer, key_id, read_activity, verify_token,
};
use mimorya_db::repositories::gateway_identities::find_gateway_identity;
use mimorya_db::repositories::gateways::{gateway_tenant, get_gateway, take_gateway_event};
use uuid::Uuid;

use super::texts;
use crate::services::gateways::{GatewayService, client_secret_name};

/// What became of an activity the Bot Framework sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityOutcome {
    /// Not proven the bot's: unknown gateway, activity not read, token
    /// missing, refused, or not checked, alike.
    Unknown,
    /// Taken: acted upon, or ignored.
    Taken,
}

impl GatewayService {
    /// Takes the activity `body` the Bot Framework sent the gateway
    /// `gateway_id` with the `Authorization` header `authorization`,
    /// received at `now`.
    pub async fn receive_teams(
        &self,
        gateway_id: Uuid,
        authorization: Option<&str>,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> anyhow::Result<ActivityOutcome> {
        let kind = GatewayKind::Teams.as_str();
        let Some(tenant_id) = gateway_tenant(self.db.pool(), gateway_id, kind).await? else {
            return Ok(ActivityOutcome::Unknown);
        };
        let (Some(token), Some(activity)) = (bearer(authorization), read_activity(body)) else {
            return Ok(ActivityOutcome::Unknown);
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(gateway) = get_gateway(&mut tx, gateway_id).await? else {
            return Ok(ActivityOutcome::Unknown);
        };
        let keys = match self.teams.keys(key_id(token).as_deref()).await {
            Ok(keys) => keys,
            Err(failure) => {
                // Told the operator; answered as unknown (the Bot Framework
                // sends it again later).
                let reason = failure.message();
                tracing::warn!(gateway = %gateway_id, %reason, "Bot Framework keys unread");
                return Ok(ActivityOutcome::Unknown);
            }
        };
        let expected = Expected {
            app_id: &gateway.bot_user_id,
            service_url: &activity.service_url,
            now: now.timestamp(),
        };
        if verify_token(token, &keys, expected).is_err() {
            return Ok(ActivityOutcome::Unknown);
        }
        let message = activity.personal_message(&gateway.bot_user_id, &gateway.workspace_id);
        let Some(message) = message.filter(|_| gateway.enabled) else {
            return Ok(ActivityOutcome::Taken);
        };
        if !self.teams.services().trust(&message.service_url) {
            tracing::warn!(gateway = %gateway_id, "Teams activity of an unknown service ignored");
            return Ok(ActivityOutcome::Taken);
        }
        if !take_gateway_event(&mut tx, tenant_id, gateway_id, &message.activity_id).await? {
            return Ok(ActivityOutcome::Taken);
        }
        let person = &message.person.aad_object_id;
        let linked = (find_gateway_identity(&mut tx, gateway_id, person).await?).is_some();
        let secret = self
            .secret(&mut tx, tenant_id, &client_secret_name(gateway_id))
            .await?;
        tx.commit().await?;
        let Some(client_secret) = secret else {
            tracing::warn!(gateway = %gateway_id, "Teams bot's client secret unreadable");
            return Ok(ActivityOutcome::Taken);
        };
        let app = TeamsApplication {
            app_id: gateway.bot_user_id,
            directory_id: gateway.workspace_id,
            client_secret,
        };
        if linked {
            let conversation = (message.service_url, message.conversation_id);
            let text = texts::not_yet(&self.assistant_name);
            self.say_in_teams(gateway_id, app, conversation, text);
        } else {
            self.offer_teams_link((tenant_id, gateway_id), app, &message, now)?;
        }
        Ok(ActivityOutcome::Taken)
    }
}
