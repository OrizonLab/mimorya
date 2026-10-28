//! What Slack sends to a company's gateway (ADR-125), before any session:
//! the request names its gateway, then proves itself by its signature,
//! checked with the gateway's signing secret, within five minutes. An
//! unknown gateway, a request not signed by its application, and a gateway
//! whose secret this server cannot open answer alike. Once signed: the
//! address check is answered with its challenge; an event of another
//! workspace, of a paused gateway, or already taken (Slack sends it again
//! when it was not answered in time) is taken and ignored. A person's own
//! message to the application, from someone who has not linked their Slack
//! identity, is answered with a link to do so (`link`). What a message says
//! is never logged nor audited.

use chrono::{DateTime, Utc};
use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::slack::{Envelope, direct_message, is_event, verify};
use mimorya_db::repositories::gateway_identities::find_gateway_identity;
use mimorya_db::repositories::gateways::{gateway_tenant, get_gateway, take_gateway_event};
use uuid::Uuid;

use super::{GatewayService, bot_token_name, signing_secret_name};

/// The longest challenge sent back: Slack's are a few dozen characters.
const MAX_CHALLENGE_CHARS: usize = 256;

/// The headers Slack signs its requests with.
#[derive(Debug, Clone, Copy, Default)]
pub struct SlackHeaders<'a> {
    pub timestamp: Option<&'a str>,
    pub signature: Option<&'a str>,
}

/// What became of a request Slack sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventOutcome {
    /// Not the gateway's application's: unknown gateway, signature missing,
    /// wrong or too old, or a secret this server cannot open, alike.
    Unknown,
    /// Signed, but not what Slack sends.
    Unreadable,
    /// The address check: its challenge, sent back.
    Challenge(String),
    /// Taken: acted upon, or ignored.
    Taken,
}

impl GatewayService {
    /// Takes what Slack sent to the gateway `gateway_id`, received at `now`.
    pub async fn receive_slack(
        &self,
        gateway_id: Uuid,
        headers: SlackHeaders<'_>,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> anyhow::Result<EventOutcome> {
        let kind = GatewayKind::Slack.as_str();
        let Some(tenant_id) = gateway_tenant(self.db.pool(), gateway_id, kind).await? else {
            return Ok(EventOutcome::Unknown);
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(gateway) = get_gateway(&mut tx, gateway_id).await? else {
            return Ok(EventOutcome::Unknown);
        };
        let secret_name = signing_secret_name(gateway_id);
        let Some(secret) = self.secret(&mut tx, tenant_id, &secret_name).await? else {
            // No master key, or another one: told the operator, answered to
            // the sender as an unknown gateway (Slack sends it again later).
            tracing::warn!(gateway = %gateway_id, "Slack gateway's signing secret unreadable");
            return Ok(EventOutcome::Unknown);
        };
        let signed = verify(
            &secret,
            headers.timestamp,
            headers.signature,
            body,
            now.timestamp(),
        );
        if signed.is_err() {
            return Ok(EventOutcome::Unknown);
        }
        let Ok(envelope) = serde_json::from_slice::<Envelope>(body) else {
            return Ok(EventOutcome::Unreadable);
        };
        let (team_id, event_id, event) = match envelope {
            Envelope::UrlVerification { challenge } => {
                let sendable = !challenge.is_empty()
                    && challenge.chars().count() <= MAX_CHALLENGE_CHARS
                    && challenge.chars().all(|c| c.is_ascii_graphic());
                return Ok(if sendable {
                    EventOutcome::Challenge(challenge)
                } else {
                    EventOutcome::Unreadable
                });
            }
            Envelope::Other => return Ok(EventOutcome::Taken),
            Envelope::EventCallback {
                team_id,
                event_id,
                event,
            } => (team_id, event_id, event),
        };
        // Signed by the application, yet of another workspace (one it was
        // installed in since): never taken for the company's.
        if !gateway.enabled || team_id != gateway.workspace_id || !is_event(&event_id) {
            return Ok(EventOutcome::Taken);
        }
        if !take_gateway_event(&mut tx, tenant_id, gateway_id, &event_id).await? {
            return Ok(EventOutcome::Taken);
        }
        let Some(message) = direct_message(&event, &gateway.bot_user_id) else {
            tx.commit().await?;
            return Ok(EventOutcome::Taken);
        };
        if find_gateway_identity(&mut tx, gateway_id, &message.person)
            .await?
            .is_some()
        {
            // A linked person's conversations come with lot 12c (ADR-125).
            tx.commit().await?;
            return Ok(EventOutcome::Taken);
        }
        let token = self
            .secret(&mut tx, tenant_id, &bot_token_name(gateway_id))
            .await?;
        tx.commit().await?;
        match token {
            Some(token) => self.offer_link(tenant_id, gateway_id, token, message, now)?,
            None => tracing::warn!(gateway = %gateway_id, "Slack gateway's bot token unreadable"),
        }
        Ok(EventOutcome::Taken)
    }
}
