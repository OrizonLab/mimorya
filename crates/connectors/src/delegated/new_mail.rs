//! The e-mail a tool sends in a person's name, whichever provider sends it
//! (ADR-119, ADR-123), as the model asks it and the person validates it:
//! its recipients, its subject and its text, each hiding nothing
//! (`compose`); and its description, whom first.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::compose::{addresses, excerpt, full_text, visible_line};

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct NewMail {
    /// Destinataires : adresses e-mail, 1 à 10
    #[validate(length(min = 1, max = 10), custom(function = "addresses"))]
    pub to: Vec<String>,
    /// Destinataires en copie : adresses e-mail, 10 au plus
    #[serde(default)]
    #[validate(length(max = 10), custom(function = "addresses"))]
    pub cc: Vec<String>,
    /// Objet, sur une ligne
    #[validate(length(min = 1, max = 255), custom(function = "visible_line"))]
    pub subject: String,
    /// Texte du message, en texte brut (retours à la ligne permis)
    #[validate(length(min = 1, max = 20000), custom(function = "full_text"))]
    pub body: String,
}

impl NewMail {
    /// Whom first, what the model wrote last: a subject cannot pass itself
    /// off as the recipients.
    pub fn described(&self, product: &str) -> String {
        let copy = match self.cc.is_empty() {
            true => String::new(),
            false => format!(" (copie : {})", self.cc.join(", ")),
        };
        format!(
            "Envoyer depuis la boîte {product} reliée un e-mail à {}{copy} — objet : « {} » ; \
             texte : « {} »",
            self.to.join(", "),
            self.subject,
            excerpt(&self.body)
        )
    }

    /// How many it is sent to, copies included.
    pub fn recipients(&self) -> usize {
        self.to.len() + self.cc.len()
    }
}
