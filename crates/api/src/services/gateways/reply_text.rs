//! What is written in Slack when a run a person started there ends or
//! pauses (ADR-127), in Slack's markup: its answer as the conversation
//! shows it (a failure's included), cut when too long; a pause for
//! validation, said with where to give it; a stop. Each says where its
//! conversation is in the application, where its sources and its details
//! are.

use mimorya_core::gateways::slack::{bounded, escaped, from_markdown};

/// The most of an answer written in Slack, which shows a message's text
/// best under 4 000 characters.
pub const MAX_ANSWER_CHARS: usize = 3_000;

/// What the end `status` of a run (`awaiting_approval` for a pause) is
/// answered with: `answer`, its latest message in the conversation, whose
/// address is `url`.
pub fn reply_text(status: &str, answer: Option<&str>, url: &str) -> String {
    let link = format!("<{}|ouvrir la conversation>", escaped(url));
    let answer = answer.map(str::trim).filter(|a| !a.is_empty());
    match (status, answer) {
        ("awaiting_approval", _) => format!(
            "Une action de cette demande attend une validation avant de continuer : {link}. La \
             suite viendra dans ce fil, une fois la validation donnée."
        ),
        ("cancelled", _) => format!("La demande a été arrêtée : {link}."),
        (_, Some(answer)) => {
            let (text, cut) = bounded(answer, MAX_ANSWER_CHARS);
            let rest = if cut {
                "La suite est dans l’application"
            } else {
                "Dans l’application"
            };
            format!("{}\n\n_{rest} : {link}._", from_markdown(&text))
        }
        _ => format!("La demande s’est terminée sans réponse : {link}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://app.example/chat/42";
    const LINK: &str = "<https://app.example/chat/42|ouvrir la conversation>";

    #[test]
    fn an_answer_is_written_in_slack_s_markup_with_its_conversation() {
        let text = reply_text("completed", Some("**Trois** visites <demain>"), URL);
        assert_eq!(
            text,
            format!("*Trois* visites &lt;demain&gt;\n\n_Dans l’application : {LINK}._")
        );
        // A failure, as the conversation shows it.
        let failed = reply_text("failed", Some("Je n’ai pas pu terminer : délai"), URL);
        assert!(
            failed.starts_with("Je n’ai pas pu terminer : délai\n\n"),
            "{failed}"
        );
    }

    #[test]
    fn a_long_answer_says_where_the_rest_is() {
        let long = "mot ".repeat(MAX_ANSWER_CHARS);
        let text = reply_text("completed", Some(&long), URL);
        assert!(text.chars().count() < MAX_ANSWER_CHARS + 200);
        assert!(
            text.ends_with(&format!("…\n\n_La suite est dans l’application : {LINK}._")),
            "{text}"
        );
    }

    #[test]
    fn a_pause_a_stop_and_an_empty_end_say_where_to_look() {
        let paused = reply_text("awaiting_approval", None, URL);
        assert!(paused.contains("attend une validation"), "{paused}");
        assert!(paused.contains(LINK), "{paused}");
        assert_eq!(
            reply_text("cancelled", Some("ignoré"), URL),
            format!("La demande a été arrêtée : {LINK}.")
        );
        assert_eq!(
            reply_text("completed", Some("  "), URL),
            format!("La demande s’est terminée sans réponse : {LINK}.")
        );
    }
}
