//! What a person's message to a gateway starts, said alike in Slack
//! (ADR-127), in Telegram (ADR-135) and in Teams (ADR-140): the title of the conversation it
//! opens, and why nothing was started when the service stops or the
//! request fails.

use crate::services::runs::TITLE_LENGTH;

/// A request that failed to start, for a reason not the person's.
pub(super) const NOT_STARTED: &str =
    "La demande n’a pas pu être lancée : réessayez dans un instant.";
/// A request written while the service stops.
pub(super) const STOPPING: &str =
    "Le service redémarre : rien n’a été lancé. Réessayez dans un instant.";

/// The title of the conversation `text` opens: its first words, on one
/// line however it was written.
pub(super) fn title_of(text: &str) -> String {
    let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
    words.chars().take(TITLE_LENGTH).collect()
}
