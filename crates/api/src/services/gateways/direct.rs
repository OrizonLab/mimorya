//! Whose direct conversation an event or a press comes from (ADR-131): its
//! signature proves Slack sent it, not that the conversation it names is
//! the direct one of the Slack user it names with the application. Whoever
//! holds the signing secret could name another's, and have what is meant
//! for one person written where another reads it. So Slack is asked
//! (`conversations.info`, `im:read`) before anything is written there or
//! started: a direct conversation, and that Slack user's. Another's, or one
//! the application cannot see: nothing said there, audited for the
//! company's administrators. Slack not saying: nothing written there
//! either, the person told in their own direct conversation, which Slack
//! finds from their identifier alone.

use mimorya_connectors::slack::{SlackApi, SlackFailure};
use uuid::Uuid;

/// Whose a conversation is, as Slack says.
pub(super) enum Whose {
    /// The sender's own direct conversation with the application.
    Theirs,
    /// Another's, or no direct conversation.
    Another,
    /// Slack did not say.
    Unsaid(SlackFailure),
}

/// Whose `channel` is, for the Slack user `person` of the gateway's
/// application.
pub(super) async fn whose_conversation(
    slack: &SlackApi,
    (gateway_id, token): (Uuid, &str),
    channel: &str,
    person: &str,
) -> Whose {
    match slack.conversation(token, channel).await {
        Ok(found) if found.is_direct_with(person) => Whose::Theirs,
        Ok(_) => {
            tracing::warn!(gateway = %gateway_id, "Slack event not from its person's direct conversation");
            Whose::Another
        }
        // One the application cannot see is no direct conversation of its.
        Err(SlackFailure::Refused(code)) if code == "channel_not_found" => {
            tracing::warn!(gateway = %gateway_id, "Slack event from a conversation not the application's");
            Whose::Another
        }
        Err(failure) => {
            let reason = failure.message();
            tracing::warn!(gateway = %gateway_id, %reason, "Slack direct conversation not verified");
            Whose::Unsaid(failure)
        }
    }
}
