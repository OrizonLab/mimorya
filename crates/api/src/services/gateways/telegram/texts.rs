//! What the bot writes to a person in Telegram (ADR-133), in plain text:
//! never what they wrote, never anything of their account.

/// To someone who has not linked their Telegram account.
pub(super) fn unlinked(assistant: &str) -> String {
    format!(
        "Bonjour ! Pour parler à {assistant} ici, reliez d’abord votre compte Telegram depuis \
         l’application : page « Mon compte », section « Messageries »."
    )
}

/// To someone who sent back a code their session asked for.
pub(super) const CLAIMED: &str = "C’est bien reçu. Confirmez maintenant la liaison dans \
     l’application, page « Mon compte » : votre compte Telegram n’est relié qu’une fois la \
     liaison confirmée.";

/// To someone who sent back a code that links nothing.
pub(super) const CODE_INVALID: &str = "Ce lien de liaison n’est plus valable : demandez-en un \
     nouveau depuis l’application, page « Mon compte ».";

/// To a linked person, until they talk with the assistant here.
pub(super) fn linked(assistant: &str) -> String {
    format!(
        "Votre compte Telegram est relié. La conversation avec {assistant} ici arrive bientôt : \
         d’ici là, écrivez-lui dans l’application."
    )
}
