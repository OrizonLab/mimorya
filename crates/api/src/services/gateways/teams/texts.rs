//! What the company's Teams bot says of itself (ADR-138, ADR-140), in plain
//! French, escaped as Teams' Markdown when written: never what the person
//! wrote. Of an account, only whose it is: the one theirs is linked to, so
//! that no one talks with the assistant as someone else unknowingly. The
//! agent's answers are written apart (`reply_text`).

use mimorya_core::gateways::{HELP, NEW_CONVERSATION};

/// To a linked person who asks for help, writes another command, or
/// nothing: to whose account theirs is linked, and how to talk with the
/// assistant here.
pub(super) fn help(assistant: &str, person: &str) -> String {
    format!(
        "Votre compte Teams est relié au compte de {person} dans l’application. Écrivez votre \
         question : {assistant} vous répond ici, avec les droits de ce compte, dans votre \
         conversation en cours, que vous retrouvez aussi dans l’application. Envoyez \
         {NEW_CONVERSATION} pour en ouvrir une nouvelle, {HELP} pour relire ce message."
    )
}

/// To a linked person who asked for a new conversation, without a word
/// for it yet.
pub(super) const NEW_CONVERSATION_NEXT: &str =
    "C’est noté : votre prochain message ouvrira une nouvelle conversation.";

/// To someone whose Teams account was unlinked since they wrote.
pub(super) const UNLINKED: &str = "Votre compte Teams n’est plus relié à un compte de \
     l’application : écrivez de nouveau au bot pour recevoir un lien de liaison.";
