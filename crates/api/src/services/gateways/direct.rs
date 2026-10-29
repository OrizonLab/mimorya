//! Whose direct conversation an event or a press comes from (ADR-131): its
//! signature proves Slack sent it, not that the conversation it names is
//! the direct one of the Slack user it names with the application. Whoever
//! holds the signing secret could name another's, and have what is meant
//! for one person written where another reads it. So Slack is asked
//! (`conversations.info`, `im:read`) before anything is written there or
//! started: a direct conversation, and that Slack user's. Slack not saying
//! is not so; nothing is said where it could not be checked.

use mimorya_connectors::slack::SlackApi;
use uuid::Uuid;

/// Whether `channel` is, as Slack says, the direct conversation of the
/// Slack user `person` with the gateway's application.
pub(super) async fn their_direct_conversation(
    slack: &SlackApi,
    (gateway_id, token): (Uuid, &str),
    channel: &str,
    person: &str,
) -> bool {
    match slack.conversation(token, channel).await {
        Ok(found) => {
            let theirs = found.direct && found.person.as_deref() == Some(person);
            if !theirs {
                tracing::warn!(gateway = %gateway_id, "Slack event not from its person's direct conversation");
            }
            theirs
        }
        Err(failure) => {
            let reason = failure.message();
            tracing::warn!(gateway = %gateway_id, %reason, "Slack direct conversation not verified");
            false
        }
    }
}
