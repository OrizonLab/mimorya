//! What the bot writes in Telegram when a run a person started there ends
//! or pauses (ADR-135), in the HTML Telegram reads: its answer as the
//! conversation shows it (a failure's included), cut when too long; a
//! pause for validation, said with where to give it; a stop. Each says
//! where its conversation is in the application, where its sources and its
//! details are. Should Telegram not read the HTML, the same is said in
//! plain text.

use mimorya_core::gateways::bounded;
use mimorya_core::gateways::telegram::{from_markdown, link};

/// The most of an answer written in Telegram, which takes 4 096
/// characters of text in a message.
pub const MAX_ANSWER_CHARS: usize = 3_000;

/// How a text is written: in Telegram's HTML, or in plain text, once
/// Telegram refused the HTML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Markup {
    Html,
    Plain,
}

impl Markup {
    /// Where the conversation is, at `url`.
    fn conversation(self, url: &str) -> String {
        match self {
            Self::Html => link(url, "ouvrir la conversation"),
            Self::Plain => url.to_owned(),
        }
    }

    /// The agent's `markdown` answer.
    fn answer(self, markdown: &str) -> String {
        match self {
            Self::Html => from_markdown(markdown),
            Self::Plain => markdown.to_owned(),
        }
    }

    /// A note set apart from the answer.
    fn note(self, text: &str) -> String {
        match self {
            Self::Html => format!("<i>{text}</i>"),
            Self::Plain => text.to_owned(),
        }
    }
}

/// The actions a pause awaits: the person's own validation (`own`), and a
/// manager's (`managed`). Both are given in the application, which shows
/// their exact parameters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pause {
    pub own: usize,
    pub managed: usize,
}

/// What the end `status` of a run (`awaiting_approval` for a pause,
/// awaiting `pause`) is answered with, in `markup`: `answer`, its latest
/// message in the conversation, whose address is `url`.
pub fn reply_text(
    markup: Markup,
    status: &str,
    answer: Option<&str>,
    url: &str,
    pause: Pause,
) -> String {
    let link = markup.conversation(url);
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
            format!("{awaited} : {link}. La suite viendra ici, une fois la validation donnée.")
        }
        ("cancelled", _) => format!("La demande a été arrêtée : {link}."),
        (_, Some(answer)) => {
            let (text, cut) = bounded(answer, MAX_ANSWER_CHARS);
            let rest = if cut {
                "La suite est dans l’application"
            } else {
                "Dans l’application"
            };
            let note = markup.note(&format!("{rest} : {link}."));
            format!("{}\n\n{note}", markup.answer(&text))
        }
        _ => format!("La demande s’est terminée sans réponse : {link}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://app.example/chat/42";
    const LINK: &str = "<a href=\"https://app.example/chat/42\">ouvrir la conversation</a>";

    fn ended(status: &str, answer: Option<&str>) -> String {
        reply_text(Markup::Html, status, answer, URL, Pause::default())
    }

    #[test]
    fn an_answer_is_written_in_html_with_its_conversation() {
        assert_eq!(
            ended("completed", Some("**Trois** visites <demain>")),
            format!("<b>Trois</b> visites &lt;demain&gt;\n\n<i>Dans l’application : {LINK}.</i>")
        );
        // A failure, as the conversation shows it.
        let failed = ended("failed", Some("Je n’ai pas pu terminer : délai"));
        assert!(
            failed.starts_with("Je n’ai pas pu terminer : délai\n\n"),
            "{failed}"
        );
        // In plain text, as written, with the address itself.
        assert_eq!(
            reply_text(
                Markup::Plain,
                "completed",
                Some("**Trois** visites <demain>"),
                URL,
                Pause::default()
            ),
            format!("**Trois** visites <demain>\n\nDans l’application : {URL}.")
        );
    }

    #[test]
    fn a_long_answer_says_where_the_rest_is() {
        let long = "mot ".repeat(MAX_ANSWER_CHARS);
        let text = ended("completed", Some(&long));
        assert!(text.chars().count() < MAX_ANSWER_CHARS + 200);
        assert!(
            text.ends_with(&format!(
                "…\n\n<i>La suite est dans l’application : {LINK}.</i>"
            )),
            "{text}"
        );
    }

    #[test]
    fn a_pause_says_whose_validation_it_awaits_and_where() {
        let paused = |own, managed| {
            reply_text(
                Markup::Html,
                "awaiting_approval",
                None,
                URL,
                Pause { own, managed },
            )
        };
        let own = paused(1, 1);
        assert!(
            own.starts_with("Une action de cette demande attend votre validation"),
            "{own}"
        );
        assert!(own.contains("paramètres exacts"), "{own}");
        assert!(own.contains(LINK), "{own}");
        assert!(own.ends_with("La suite viendra ici, une fois la validation donnée."));
        assert!(paused(0, 1).contains("validation de votre responsable"));
        assert!(paused(0, 0).contains("attend une validation avant de continuer"));
        let plain = reply_text(
            Markup::Plain,
            "awaiting_approval",
            None,
            URL,
            Pause { own: 1, managed: 0 },
        );
        assert!(plain.contains(&format!(" : {URL}. ")), "{plain}");
    }

    #[test]
    fn a_stop_and_an_empty_end_say_where_to_look() {
        assert_eq!(
            ended("cancelled", Some("ignoré")),
            format!("La demande a été arrêtée : {LINK}.")
        );
        assert_eq!(
            ended("completed", Some("  ")),
            format!("La demande s’est terminée sans réponse : {LINK}.")
        );
    }
}
