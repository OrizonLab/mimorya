//! What is written in Slack when a run a person started there ends or
//! pauses (ADR-127), in Slack's markup: its answer as the conversation
//! shows it (a failure's included), cut when too long; a pause for
//! validation, said with where to give it, the actions awaiting the
//! person's own validation shown with their buttons (ADR-129); a stop. Each
//! says where its conversation is in the application, where its sources and
//! its details are.

use mimorya_core::gateways::ShownAction;
use mimorya_core::gateways::bounded;
use mimorya_core::gateways::slack::{approval_blocks, escaped, from_markdown};
use serde_json::Value;

/// The most of an answer written in Slack, which shows a message's text
/// best under 4 000 characters.
pub const MAX_ANSWER_CHARS: usize = 3_000;
/// The most actions shown with their buttons in a pause's message.
pub const MAX_SHOWN_ACTIONS: usize = 5;

/// The actions a pause awaits: the person's own validation, in Slack
/// (`own`, shown with their exact parameters and buttons) or in the
/// application (`in_application`: sending outside the company, not undone,
/// or whose parameters do not fit), and a manager's (`managed`).
pub struct Pause<'a> {
    pub own: &'a [ShownAction<'a>],
    pub in_application: usize,
    pub managed: usize,
}

/// What a pause is answered with: its text, and the blocks showing it with
/// a button for each action awaiting the person's own validation in Slack,
/// if any.
pub fn pause_reply(url: &str, pause: &Pause) -> (String, Option<Vec<Value>>) {
    let link = format!("<{}|ouvrir la conversation>", escaped(url));
    if pause.own.is_empty() {
        let text = if pause.in_application > 0 {
            format!(
                "Une action de cette demande attend votre validation, à donner dans \
                 l’application, qui en montre les paramètres exacts : {link}. La suite viendra \
                 dans ce fil, une fois la validation donnée."
            )
        } else if pause.managed > 0 {
            format!(
                "Une action de cette demande attend la validation de votre responsable : \
                 {link}. La suite viendra dans ce fil, une fois la validation donnée."
            )
        } else {
            reply_text("awaiting_approval", None, url)
        };
        return (text, None);
    }
    let shown = &pause.own[..pause.own.len().min(MAX_SHOWN_ACTIONS)];
    let mut intro = if pause.own.len() == 1 {
        format!(
            "Une action de cette demande attend votre validation : validez-la ou refusez-la \
             ci-dessous, après en avoir lu les paramètres exacts, ou dans l’application \
             ({link}). La suite viendra dans ce fil."
        )
    } else {
        format!(
            "Des actions de cette demande attendent votre validation : validez-les ou \
             refusez-les ci-dessous, après en avoir lu les paramètres exacts, ou dans \
             l’application ({link}). La suite viendra dans ce fil."
        )
    };
    if shown.len() < pause.own.len() || pause.in_application > 0 {
        intro.push_str(" D’autres sont à valider dans l’application.");
    }
    if pause.managed > 0 {
        intro.push_str(" D’autres attendent la validation de votre responsable.");
    }
    let blocks = approval_blocks(&intro, shown);
    (intro, Some(blocks))
}

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

    fn shown(parameters: &serde_json::Value) -> ShownAction<'_> {
        ShownAction {
            approval_id: uuid::Uuid::new_v4(),
            summary: "Appliquer le planning",
            risk: "write",
            parameters,
        }
    }

    #[test]
    fn a_pause_shows_the_person_s_own_actions_with_their_buttons() {
        let parameters = serde_json::json!({ "proposalId": "p-1" });
        let actions = [shown(&parameters), shown(&parameters)];
        let pause = Pause {
            own: &actions[..1],
            in_application: 0,
            managed: 0,
        };
        let (text, blocks) = pause_reply(URL, &pause);
        assert!(
            text.starts_with("Une action de cette demande attend votre validation"),
            "{text}"
        );
        assert!(text.contains("paramètres exacts"), "{text}");
        assert!(text.contains(LINK), "{text}");
        let blocks = blocks.expect("its buttons");
        assert_eq!(blocks.len(), 4);
        assert_eq!(
            blocks[3]["elements"][0]["value"],
            actions[0].approval_id.to_string()
        );
        // Several, some in the application, some a manager decides.
        let pause = Pause {
            own: &actions,
            in_application: 1,
            managed: 1,
        };
        let (text, blocks) = pause_reply(URL, &pause);
        assert!(text.starts_with("Des actions"), "{text}");
        assert!(
            text.contains("D’autres sont à valider dans l’application."),
            "{text}"
        );
        assert!(text.ends_with("validation de votre responsable."), "{text}");
        assert_eq!(blocks.expect("their buttons").len(), 7);
        // Beyond those shown, the rest in the application.
        let many: Vec<_> = (0..MAX_SHOWN_ACTIONS + 2)
            .map(|_| shown(&parameters))
            .collect();
        let (text, blocks) = pause_reply(
            URL,
            &Pause {
                own: &many,
                in_application: 0,
                managed: 0,
            },
        );
        assert!(
            text.contains("D’autres sont à valider dans l’application"),
            "{text}"
        );
        assert_eq!(
            blocks.expect("their buttons").len(),
            1 + 3 * MAX_SHOWN_ACTIONS
        );
    }

    #[test]
    fn a_pause_in_the_application_for_a_manager_or_already_decided_has_no_button() {
        let (text, blocks) = pause_reply(
            URL,
            &Pause {
                own: &[],
                in_application: 1,
                managed: 0,
            },
        );
        assert!(
            text.contains("à donner dans l’application, qui en montre les paramètres"),
            "{text}"
        );
        assert!(text.contains(LINK), "{text}");
        assert_eq!(blocks, None);
        let (text, blocks) = pause_reply(
            URL,
            &Pause {
                own: &[],
                in_application: 0,
                managed: 1,
            },
        );
        assert!(text.contains("validation de votre responsable"), "{text}");
        assert_eq!(blocks, None);
        let (text, blocks) = pause_reply(
            URL,
            &Pause {
                own: &[],
                in_application: 0,
                managed: 0,
            },
        );
        assert_eq!(text, reply_text("awaiting_approval", None, URL));
        assert_eq!(blocks, None);
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
