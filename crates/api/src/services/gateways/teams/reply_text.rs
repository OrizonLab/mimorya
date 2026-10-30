//! What the bot writes in Teams when a run a person started there ends or
//! pauses (ADR-140), in the Markdown Teams reads: its answer as the
//! conversation shows it (a failure's included), rebuilt from its Markdown
//! (`from_markdown`), cut when too long; a pause for validation, said with
//! whose it is, to give in the application, which shows the actions' exact
//! parameters; a stop. Each says where its conversation is in the
//! application, where its sources and its details are.

use mimorya_core::gateways::bounded;
use mimorya_core::gateways::teams::{escaped, from_markdown, link};

/// The most of an answer written in Teams, which takes about 28 kibibytes
/// a message: as in the other services.
pub const MAX_ANSWER_CHARS: usize = 3_000;

/// The actions a pause awaits: the person's own validation (`own`) or a
/// manager's (`managed`), in the application.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pause {
    pub own: usize,
    pub managed: usize,
}

/// What the end `status` of a run (`awaiting_approval` for a pause,
/// awaiting `pause`) is answered with: `answer`, its latest message in the
/// conversation, whose address is `url`.
pub fn reply_text(status: &str, answer: Option<&str>, url: &str, pause: Pause) -> String {
    let link = link(url, "ouvrir la conversation").unwrap_or_else(|| escaped(url));
    let answer = answer.map(str::trim).filter(|a| !a.is_empty());
    match (status, answer) {
        ("awaiting_approval", _) => {
            let awaited = if pause.own > 0 {
                "Une action de cette demande attend votre validation, à donner dans \
                 l’application, qui en montre les paramètres exacts"
            } else if pause.managed > 0 {
                "Une action de cette demande attend la validation de votre responsable"
            } else {
                "Une action de cette demande attend une validation avant de continuer"
            };
            format!("{awaited} : {link}\\. La suite viendra ici, une fois la validation donnée\\.")
        }
        ("cancelled", _) => format!("La demande a été arrêtée : {link}\\."),
        (_, Some(answer)) => {
            let (text, cut) = bounded(answer, MAX_ANSWER_CHARS);
            let rest = if cut {
                "La suite est dans l’application"
            } else {
                "Dans l’application"
            };
            format!("{}\n\n*{rest} : {link}\\.*", from_markdown(&text))
        }
        _ => format!("La demande s’est terminée sans réponse : {link}\\."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://app.example/chat/42";
    const LINK: &str = "[ouvrir la conversation](https://app.example/chat/42)";

    #[test]
    fn an_answer_is_written_in_teams_markdown_with_its_conversation() {
        let text = reply_text(
            "completed",
            Some("**Trois** visites <demain>"),
            URL,
            Pause::default(),
        );
        assert_eq!(
            text,
            format!("**Trois** visites &lt;demain&gt;\n\n*Dans l’application : {LINK}\\.*")
        );
        let failed = reply_text("failed", Some("Échec : délai"), URL, Pause::default());
        assert!(failed.starts_with("Échec : délai\n\n"), "{failed}");
        let none = reply_text("completed", Some("  "), URL, Pause::default());
        assert_eq!(
            none,
            format!("La demande s’est terminée sans réponse : {LINK}\\.")
        );
        let stopped = reply_text("cancelled", None, URL, Pause::default());
        assert_eq!(stopped, format!("La demande a été arrêtée : {LINK}\\."));
    }

    #[test]
    fn a_long_answer_says_where_the_rest_is() {
        let long = "mot ".repeat(MAX_ANSWER_CHARS);
        let text = reply_text("completed", Some(&long), URL, Pause::default());
        assert!(text.chars().count() < MAX_ANSWER_CHARS + 200);
        assert!(
            text.ends_with(&format!(
                "…\n\n*La suite est dans l’application : {LINK}\\.*"
            )),
            "{text}"
        );
    }

    #[test]
    fn a_pause_says_whose_validation_it_awaits_in_the_application() {
        let own = Pause { own: 1, managed: 0 };
        let text = reply_text("awaiting_approval", Some("ignoré"), URL, own);
        assert!(
            text.starts_with(
                "Une action de cette demande attend votre validation, à donner dans l’application"
            ),
            "{text}"
        );
        assert!(text.contains(LINK), "{text}");
        let managed = Pause { own: 0, managed: 2 };
        let text = reply_text("awaiting_approval", None, URL, managed);
        assert!(
            text.contains("la validation de votre responsable"),
            "{text}"
        );
    }
}
