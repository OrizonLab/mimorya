//! What the bot writes to a person in Telegram (ADR-133, ADR-135), in plain
//! text: never what they wrote. Of an account, only whose it is: the one a
//! link would link their Telegram account to, or the one it is linked to,
//! so that no one's is linked to another's unknowingly. The agent's answers
//! are written apart (`reply_text`).

use mimorya_core::gateways::telegram::{HELP, NEW_CONVERSATION};

/// Where in the application a person links their Telegram account.
const LINKS_PAGE: &str = "page « Mon compte », section « Messageries reliées »";

/// To someone who has not linked their Telegram account.
pub(super) fn unlinked(assistant: &str) -> String {
    format!(
        "Bonjour ! Pour parler à {assistant} ici, reliez d’abord votre compte Telegram depuis \
         l’application : {LINKS_PAGE}."
    )
}

/// To someone who opened the link of the person `owner`'s code.
pub(super) fn offer(owner: &str) -> String {
    format!(
        "Ce lien relie votre compte Telegram au compte de {owner} dans l’application. Si c’est \
         bien votre compte, envoyez /oui. Sinon, ignorez ce message : rien ne sera relié."
    )
}

/// To someone who accepted the link of the person `owner`'s code.
pub(super) fn claimed(owner: &str) -> String {
    format!(
        "C’est noté : votre compte Telegram sera relié au compte de {owner} une fois la \
         liaison confirmée dans l’application, {LINKS_PAGE}."
    )
}

/// To someone who accepts a link no one offered them.
pub(super) const NOTHING_OFFERED: &str = "Aucune liaison n’attend votre accord : ouvrez \
     d’abord le lien de liaison demandé depuis l’application, page « Mon compte ».";

/// To someone who sent back a code that links nothing.
pub(super) const CODE_INVALID: &str = "Ce lien de liaison n’est plus valable : demandez-en un \
     nouveau depuis l’application, page « Mon compte ».";

/// To someone whose Telegram account is linked to another person than the
/// one whose code they opened.
pub(super) const LINKED_ELSEWHERE: &str = "Ce compte Telegram est déjà relié à un autre \
     compte de l’application : seule la personne de ce compte peut le retirer, depuis sa page \
     « Mon compte ».";

/// To a linked person who starts the bot again or asks for help: to whose
/// account theirs is linked, and how to talk with the assistant here.
pub(super) fn help(assistant: &str, person: &str) -> String {
    format!(
        "Votre compte Telegram est relié au compte de {person} dans l’application. Écrivez \
         votre question : {assistant} vous répond ici, avec les droits de ce compte, dans votre \
         conversation en cours, que vous retrouvez aussi dans l’application. Envoyez \
         {NEW_CONVERSATION} pour en ouvrir une nouvelle, {HELP} pour relire ce message."
    )
}

/// To a linked person who asked for a new conversation, without a word
/// for it yet.
pub(super) const NEW_CONVERSATION_NEXT: &str =
    "C’est noté : votre prochain message ouvrira une nouvelle conversation.";
