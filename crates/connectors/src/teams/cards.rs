//! The cards a company's Teams bot writes (ADR-141): a message with an
//! Adaptive Card, whose activity Teams numbers when it takes it (kept, so
//! that a button pressed on the card can be known to be one of its own),
//! and the same card written again in place of it once some of its actions
//! are decided.

use mimorya_core::gateways::teams::TeamsApplication;
use serde_json::{Value, json};

use super::{TeamsApi, TeamsFailure};

/// The longest activity identifier kept, as the database bounds it.
const MAX_ACTIVITY_ID_CHARS: usize = 512;

impl TeamsApi {
    /// Writes `text` (Teams' Markdown) with `card` (an Adaptive Card) in the
    /// conversation `to` of its service, as the bot `app`: the activity
    /// Teams numbered it, if it said one.
    pub async fn send_card(
        &self,
        app: &TeamsApplication,
        to: (&str, &str),
        text: &str,
        card: &Value,
    ) -> Result<Option<String>, TeamsFailure> {
        let answer = (self.write_activity(app, to, None, &with_card(text, card))).await?;
        let id = (answer.get("id").and_then(Value::as_str)).filter(|id| {
            !id.is_empty()
                && id.chars().count() <= MAX_ACTIVITY_ID_CHARS
                && id.chars().all(|c| c.is_ascii_graphic())
        });
        Ok(id.map(str::to_owned))
    }

    /// Writes `text` with `card` in place of the bot's activity
    /// `activity_id` in the conversation `to`.
    pub async fn update_card(
        &self,
        app: &TeamsApplication,
        to: (&str, &str),
        activity_id: &str,
        (text, card): (&str, &Value),
    ) -> Result<(), TeamsFailure> {
        let body = with_card(text, card);
        (self.write_activity(app, to, Some(activity_id), &body))
            .await
            .map(drop)
    }
}

/// A message of `text`, in Teams' Markdown, with `card` attached.
fn with_card(text: &str, card: &Value) -> Value {
    json!({
        "type": "message",
        "text": text,
        "textFormat": "markdown",
        "attachments": [{
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": card,
        }],
    })
}
