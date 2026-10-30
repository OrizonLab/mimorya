//! What the Bot Framework sends a company's Teams bot (ADR-138), before any
//! session: the request names its gateway, then proves itself by the token
//! in its `Authorization` header, signed by the Bot Framework with a key it
//! publishes, endorsed for Teams, for the bot's application, naming the
//! service the activity came from (`verify_token`). An unknown gateway, an
//! activity not read, a token missing or proving nothing, and a gateway
//! whose keys could not be read answer alike; no connection to the database
//! is held while Microsoft is asked for the keys. Once proven: an activity
//! of a paused gateway, of another directory, of anything but a person's
//! own message to the bot in their personal conversation, naming a service
//! not Microsoft's Teams, or already taken in its conversation (the Bot
//! Framework sends it again when it was not answered in time) is taken and
//! ignored. A person's own message is answered: from someone who has not
//! linked their Teams identity, with a link to do so (`link`); from a
//! linked person, as their message asks (`answers`, ADR-140): a command
//! answered by the bot, their words passed on to the agent
//! (`conversations`), the answers of their personal conversation's threads
//! written from then on at the service it came from. What a message says
//! is never logged nor audited.

use chrono::{DateTime, Utc};
use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::teams::{
    Expected, TeamsApplication, bearer, escaped, key_id, read_activity, verify_token,
};
use mimorya_db::repositories::gateway_identities::find_gateway_identity;
use mimorya_db::repositories::gateway_threads::move_teams_threads;
use mimorya_db::repositories::gateways::{gateway_tenant, get_gateway, take_gateway_event};
use uuid::Uuid;

use super::TeamsMessage;
use super::answers::{Answer, answer_to};
use crate::services::gateways::{GatewayService, client_secret_name};

/// What became of an activity the Bot Framework sent.
#[derive(Debug)]
pub enum ActivityOutcome {
    /// Not proven the bot's: unknown gateway, activity not read, token
    /// missing, refused, or not checked, alike.
    Unknown,
    /// Taken: acted upon, or ignored.
    Taken,
    /// Taken: a linked person's words, for the agent.
    Message(Box<TeamsMessage>),
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
        // Read, then let go: no connection is held while Microsoft is asked.
        let mut tx = self.db.begin(tenant_id).await?;
        let gateway = get_gateway(&mut tx, gateway_id).await?;
        tx.commit().await?;
        let Some(gateway) = gateway else {
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
        let mut tx = self.db.begin(tenant_id).await?;
        if !take_gateway_event(&mut tx, tenant_id, gateway_id, &message.event_key()).await? {
            return Ok(ActivityOutcome::Taken);
        }
        let person = &message.person.aad_object_id;
        let identity = find_gateway_identity(&mut tx, gateway_id, person).await?;
        let answer = match &identity {
            Some(identity) => {
                let (to, chat) = ((gateway_id, identity.user_id), &message.conversation_id);
                // Its conversation's answers go to the service it came
                // from, a paused run's in a conversation closed since too.
                move_teams_threads(&mut tx, gateway_id, chat, &message.service_url).await?;
                let assistant = &self.assistant_name;
                Some(answer_to(&mut tx, to, chat, &message.text, assistant).await?)
            }
            None => None,
        };
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
        match answer {
            None => self.offer_teams_link((tenant_id, gateway_id), app, &message, now)?,
            Some(Answer::Say(text)) => {
                let conversation = (message.service_url, message.conversation_id);
                self.say_in_teams(gateway_id, app, conversation, escaped(&text));
            }
            Some(Answer::Converse(words)) => {
                return Ok(ActivityOutcome::Message(Box::new(TeamsMessage {
                    tenant_id,
                    gateway_id,
                    app,
                    message,
                    words,
                })));
            }
        }
        Ok(ActivityOutcome::Taken)
    }
}
