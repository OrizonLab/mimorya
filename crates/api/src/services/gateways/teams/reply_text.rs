//! What the bot writes in Teams when a run a person started there ends or
//! pauses (ADR-140), in the Markdown Teams reads: its answer as the
//! conversation shows it (a failure's included), rebuilt from its Markdown
//! (`from_markdown`), cut when too long; a pause for validation, said with
//! whose it is: the person's own shown on the card the text comes with, to
//! validate there or in the application (ADR-141), the others in the
//! application, which shows the actions' exact parameters; once every
//! action of its card is decided, that they are (`decided_text`); a stop.
//! Each says where its conversation is in the application, where its
//! sources and its details are.

use mimorya_core::gateways::bounded;
use mimorya_core::gateways::teams::{escaped, from_markdown, link};

/// The most of an answer written in Teams, which takes about 28 kibibytes
/// a message: as in the other services.
pub const MAX_ANSWER_CHARS: usize = 3_000;

/// The actions a pause awaits: the person's own validation, shown on the
/// card (`shown`) or to give in the application (`in_application`), or a
/// manager's (`managed`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Pause {
    pub shown: usize,
    pub in_application: usize,
    pub managed: usize,
}

/// What the end `status` of a run (`awaiting_approval` for a pause,
/// awaiting `pause`) is answered with: `answer`, its latest message in the
/// conversation, whose address is `url`.
pub fn reply_text(status: &str, answer: Option<&str>, url: &str, pause: Pause) -> String {
    let link = link(url, "ouvrir la conversation").unwrap_or_else(|| escaped(url));
    let answer = answer.map(str::trim).filter(|a| !a.is_empty());
    match (status, answer) {
        ("awaiting_approval", _) if pause.shown > 0 => {
            let (awaits, them) = if pause.shown == 1 {
                (
                    "Une action de cette demande attend",
                    "validez-la ou refusez-la",
                )
            } else {
                (
                    "Des actions de cette demande attendent",
                    "validez-les ou refusez-les",
                )
            };
            let mut text = format!(
                "{}{link}{}",
                escaped(&format!(
                    "{awaits} votre validation : {them} ci-dessous, après en avoir lu les \
                     paramètres exacts, ou dans l’application ("
                )),
                escaped("). La suite viendra ici.")
            );
            text.push_str(&others(pause));
            text
        }
        ("awaiting_approval", _) => {
            let awaited = if pause.in_application > 0 {
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

/// What a pause's card is written again with once none of its actions
/// awaits a decision any more, its buttons all taken off (ADR-141).
pub fn decided_text(url: &str, pause: Pause) -> String {
    let link = link(url, "ouvrir la conversation").unwrap_or_else(|| escaped(url));
    let decided = if pause.shown == 1 {
        "L’action de cette demande montrée ci-dessous est décidée ("
    } else {
        "Les actions de cette demande montrées ci-dessous sont décidées ("
    };
    format!(
        "{}{link}{}{}",
        escaped(decided),
        escaped("). La suite viendra ici."),
        others(pause)
    )
}

/// What a pause with a card says of its other actions.
fn others(pause: Pause) -> String {
    let mut text = String::new();
    if pause.in_application > 0 {
        text.push_str(&escaped(" D’autres sont à valider dans l’application."));
    }
    if pause.managed > 0 {
        text.push_str(&escaped(
            " D’autres attendent la validation de votre responsable.",
        ));
    }
    text
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
        let own = Pause {
            in_application: 1,
            ..Pause::default()
        };
        let text = reply_text("awaiting_approval", Some("ignoré"), URL, own);
        assert!(
            text.starts_with(
                "Une action de cette demande attend votre validation, à donner dans l’application"
            ),
            "{text}"
        );
        assert!(text.contains(LINK), "{text}");
        let managed = Pause {
            managed: 2,
            ..Pause::default()
        };
        let text = reply_text("awaiting_approval", None, URL, managed);
        assert!(
            text.contains("la validation de votre responsable"),
            "{text}"
        );
    }

    #[test]
    fn a_pause_with_its_card_says_to_validate_below_or_in_the_application() {
        let one = Pause {
            shown: 1,
            ..Pause::default()
        };
        assert_eq!(
            reply_text("awaiting_approval", None, URL, one),
            format!(
                "Une action de cette demande attend votre validation : validez\\-la ou \
                 refusez\\-la ci\\-dessous, après en avoir lu les paramètres exacts, ou dans \
                 l’application \\({LINK}\\)\\. La suite viendra ici\\."
            )
        );
        let several = Pause {
            shown: 2,
            in_application: 1,
            managed: 1,
        };
        let text = reply_text("awaiting_approval", None, URL, several);
        assert!(
            text.starts_with("Des actions de cette demande attendent votre validation"),
            "{text}"
        );
        assert!(
            text.ends_with(
                " D’autres sont à valider dans l’application\\. D’autres attendent la \
                 validation de votre responsable\\."
            ),
            "{text}"
        );
    }

    #[test]
    fn a_card_whose_actions_are_all_decided_says_so() {
        let one = Pause {
            shown: 1,
            ..Pause::default()
        };
        assert_eq!(
            decided_text(URL, one),
            format!(
                "L’action de cette demande montrée ci\\-dessous est décidée \\({LINK}\\)\\. \
                 La suite viendra ici\\."
            )
        );
        let several = Pause {
            shown: 2,
            in_application: 1,
            managed: 0,
        };
        let text = decided_text(URL, several);
        assert!(
            text.starts_with("Les actions de cette demande montrées ci\\-dessous sont décidées"),
            "{text}"
        );
        assert!(
            text.ends_with(" D’autres sont à valider dans l’application\\."),
            "{text}"
        );
    }
}
