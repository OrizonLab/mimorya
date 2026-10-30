//! The company's Microsoft Teams bot (ADR-138): what its administrators
//! declare of it (its Azure bot's application, the Microsoft Entra
//! directory both belong to, the application's client secret), how an
//! activity the Bot Framework sends proves it comes from Microsoft's
//! channel (`token`), which activities reach the platform (`activity`),
//! where the bot writes back (`service_url`), when Teams proves a Teams user
//! is a person (`identity`), and how the bot's text is written there
//! (`text`).

mod activity;
mod identity;
mod markdown;
mod service_url;
mod text;
mod token;

pub use activity::{
    MAX_MESSAGE_CHARS, PersonalMessage, ReceivedActivity, TeamsUser, guid, read_activity, same_guid,
};
pub use identity::{IdentityRefusal, TeamsMember, identity_refusal};
pub use markdown::from_markdown;
pub use service_url::ServiceUrls;
pub use text::{escaped, link};
pub use token::{BotKey, Expected, MAX_SKEW_SECONDS, TokenProblem, bearer, key_id, verify_token};

/// The channel Teams' activities come from, which the Bot Framework's keys
/// must be endorsed for.
pub const CHANNEL: &str = "msteams";
/// Who issues the tokens of the activities the Bot Framework sends.
pub const BOT_FRAMEWORK_ISSUER: &str = "https://api.botframework.com";
/// What the bot asks Microsoft Entra a token for, to write back.
pub const BOT_FRAMEWORK_SCOPE: &str = "https://api.botframework.com/.default";
/// The longest client secret taken: Entra's are about forty characters.
const MAX_SECRET_CHARS: usize = 256;

/// What a company declares of its Teams bot, checked.
#[derive(Clone, PartialEq, Eq)]
pub struct TeamsApplication {
    /// The bot's application (its Microsoft App ID), lowercase.
    pub app_id: String,
    /// The Entra directory (tenant) the bot and its people belong to.
    pub directory_id: String,
    pub client_secret: String,
}

/// Never the secret, whatever prints it.
impl std::fmt::Debug for TeamsApplication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TeamsApplication")
            .field("app_id", &self.app_id)
            .field("directory_id", &self.directory_id)
            .finish_non_exhaustive()
    }
}

/// What a company declares of its bot, once checked, or why it is refused
/// (in French).
pub fn teams_application(
    app_id: &str,
    directory_id: &str,
    client_secret: &str,
) -> Result<TeamsApplication, &'static str> {
    let app_id = guid(app_id).ok_or(
        "Identifiant de l’application (Microsoft App ID) du bot : un GUID \
         (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)",
    )?;
    let directory_id = guid(directory_id).ok_or(
        "Identifiant de l’annuaire (ID de locataire Microsoft Entra) : un GUID \
         (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)",
    )?;
    Ok(TeamsApplication {
        app_id,
        directory_id,
        client_secret: client_secret_of(client_secret)?,
    })
}

/// A client secret, as Entra shows it, or why not (in French).
pub fn client_secret_of(secret: &str) -> Result<String, &'static str> {
    let secret = secret.trim();
    let chars = secret.chars().count();
    if !(16..=MAX_SECRET_CHARS).contains(&chars) || !secret.chars().all(|c| c.is_ascii_graphic()) {
        return Err(
            "Secret client de l’application : celui que Microsoft Entra montre à sa création \
             (16 à 256 caractères, sans espace)",
        );
    }
    Ok(secret.to_owned())
}

/// Whether `id` names a Teams user as kept: their Entra object, lowercase.
pub fn is_person(id: &str) -> bool {
    guid(id).is_some_and(|guid| guid == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "6F1C2B7A-0D3E-4C5B-9A8F-7E6D5C4B3A21";
    const DIRECTORY: &str = "0b2c3d4e-5f60-4718-8293-a4b5c6d7e8f9";
    const SECRET: &str = "Abc8Q~fictif.secret_client-0123456789";

    #[test]
    fn a_bot_is_declared_with_its_application_directory_and_secret() {
        let declared = teams_application(APP, DIRECTORY, &format!(" {SECRET}\n")).unwrap();
        assert_eq!(declared.app_id, APP.to_lowercase());
        assert_eq!(declared.directory_id, DIRECTORY);
        assert_eq!(declared.client_secret, SECRET);
        assert!(!format!("{declared:?}").contains(SECRET));
        assert!(teams_application("pas-un-guid", DIRECTORY, SECRET).is_err());
        assert!(teams_application(APP, "", SECRET).is_err());
        for secret in [
            "court",
            "secret avec des espaces 0123456789",
            &"a".repeat(257),
        ] {
            assert!(
                teams_application(APP, DIRECTORY, secret).is_err(),
                "{secret}"
            );
        }
    }

    #[test]
    fn a_teams_user_is_kept_by_their_entra_object() {
        assert!(is_person(DIRECTORY));
        assert!(!is_person(APP));
        assert!(!is_person("29:1abc"));
    }
}
