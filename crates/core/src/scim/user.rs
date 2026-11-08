//! What an account the identity provider sends may hold, and what is kept
//! of it (ADR-183): its address (`userName`, an e-mail address, the
//! account's for good), a name (`displayName`, else `name.formatted`, else
//! the given and family names, else the address's local part), a job title
//! (`title`), the provider's own identifier (`externalId`) and whether it is
//! active. Anything else the provider sends is read by no one.

use validator::ValidateEmail;

use crate::accounts::{EMAIL_MAX_LEN, display_name_problem};

/// Longest job title, in characters, as the administration's.
pub const TITLE_MAX_CHARS: usize = 120;
/// Longest identifier the provider gives an account, in characters.
pub const EXTERNAL_ID_MAX_CHARS: usize = 255;

// « Le nom affiché (displayName) compte de 2 à 120 caractères ».
const WHOM: &str = "affiché (displayName)";

/// What an account sent by the provider holds, as the API read it: each
/// field absent when the provider did not send it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScimUserFields {
    pub user_name: Option<String>,
    pub display_name: Option<String>,
    pub formatted_name: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub title: Option<String>,
    pub external_id: Option<String>,
    pub active: Option<bool>,
}

/// An account to create, as kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewScimUser {
    /// Lowercase, as every account's.
    pub email: String,
    pub display_name: String,
    pub title: Option<String>,
    pub external_id: Option<String>,
    pub active: bool,
}

/// The account `fields` describe, or why it cannot be created, said in
/// French for the provider's logs.
pub fn read_new_user(fields: ScimUserFields) -> Result<NewScimUser, String> {
    refuse_nul(&fields)?;
    let email = email_of(fields.user_name.as_deref())?;
    let display_name = name_given(&fields)
        .unwrap_or_else(|| email.split('@').next().unwrap_or(&email).to_string());
    check_name(&display_name)?;
    Ok(NewScimUser {
        title: title_of(fields.title.as_deref())?,
        external_id: external_id_of(fields.external_id)?,
        active: fields.active.unwrap_or(true),
        email,
        display_name,
    })
}

/// Refused when an attribute holds a nul character, which no text keeps.
pub(super) fn refuse_nul(fields: &ScimUserFields) -> Result<(), String> {
    let nul = [
        &fields.user_name,
        &fields.display_name,
        &fields.formatted_name,
        &fields.given_name,
        &fields.family_name,
        &fields.title,
        &fields.external_id,
    ]
    .into_iter()
    .flatten()
    .any(|value| value.contains('\0'));
    if nul {
        return Err(NUL.into());
    }
    Ok(())
}

/// Why a text holding a nul character is refused.
pub(super) const NUL: &str = "Un attribut contient un caractère nul";

/// The account's address, `userName`: an e-mail address, lowercase.
pub(super) fn email_of(user_name: Option<&str>) -> Result<String, String> {
    filled(user_name)
        .map(str::to_lowercase)
        .filter(|email| email.chars().count() as u64 <= EMAIL_MAX_LEN && email.validate_email())
        .ok_or_else(|| "userName : l’adresse e-mail du compte, obligatoire".into())
}

/// Why `name` cannot be the account's, if it cannot.
pub(super) fn check_name(name: &str) -> Result<(), String> {
    match display_name_problem(name, WHOM) {
        Some(problem) => Err(problem),
        None => Ok(()),
    }
}

/// The job title `title` gives: none when empty, refused when too long.
pub(super) fn title_of(title: Option<&str>) -> Result<Option<String>, String> {
    let title = filled(title).map(str::to_string);
    if title
        .as_deref()
        .is_some_and(|title| title.chars().count() > TITLE_MAX_CHARS)
    {
        return Err(format!("title : {TITLE_MAX_CHARS} caractères au plus"));
    }
    Ok(title)
}

/// The provider's identifier `external_id` gives, kept exactly: none when
/// empty, refused when too long.
pub(super) fn external_id_of(external_id: Option<String>) -> Result<Option<String>, String> {
    let external_id = external_id.filter(|id| !id.trim().is_empty());
    if external_id
        .as_deref()
        .is_some_and(|id| id.chars().count() > EXTERNAL_ID_MAX_CHARS)
    {
        return Err(format!(
            "externalId : {EXTERNAL_ID_MAX_CHARS} caractères au plus"
        ));
    }
    Ok(external_id)
}

/// `value` trimmed, unless it is empty.
pub(super) fn filled(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// The first name the provider gives the account: `displayName`, else
/// `name.formatted`, else the given and family names; none if it gives none.
pub(super) fn name_given(fields: &ScimUserFields) -> Option<String> {
    let parts = [fields.given_name.as_deref(), fields.family_name.as_deref()]
        .into_iter()
        .filter_map(filled)
        .collect::<Vec<_>>()
        .join(" ");
    filled(fields.display_name.as_deref())
        .or_else(|| filled(fields.formatted_name.as_deref()))
        .or_else(|| filled(Some(&parts)))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sent(user_name: &str) -> ScimUserFields {
        ScimUserFields {
            user_name: Some(user_name.into()),
            ..ScimUserFields::default()
        }
    }

    #[test]
    fn keeps_the_address_lowercase_and_the_first_name_given() {
        let user = read_new_user(ScimUserFields {
            display_name: Some("  Léa Martin ".into()),
            formatted_name: Some("Mme Léa Martin".into()),
            given_name: Some("Léa".into()),
            title: Some(" Technicienne ".into()),
            external_id: Some("a1b2".into()),
            active: Some(false),
            ..sent(" Lea.Martin@Neo.example ")
        })
        .unwrap();
        assert_eq!(
            user,
            NewScimUser {
                email: "lea.martin@neo.example".into(),
                display_name: "Léa Martin".into(),
                title: Some("Technicienne".into()),
                external_id: Some("a1b2".into()),
                active: false,
            }
        );
    }

    #[test]
    fn names_an_account_from_what_the_provider_gives() {
        let name = |fields: ScimUserFields| read_new_user(fields).unwrap().display_name;
        assert_eq!(
            name(ScimUserFields {
                formatted_name: Some("Léa Martin".into()),
                ..sent("lea@neo.example")
            }),
            "Léa Martin"
        );
        assert_eq!(
            name(ScimUserFields {
                given_name: Some("Léa".into()),
                family_name: Some("Martin".into()),
                display_name: Some("  ".into()),
                ..sent("lea@neo.example")
            }),
            "Léa Martin"
        );
        assert_eq!(name(sent("lea.martin@neo.example")), "lea.martin");
        let user = read_new_user(sent("lea@neo.example")).unwrap();
        assert!(user.active);
        assert_eq!((user.title, user.external_id), (None, None));
    }

    #[test]
    fn says_why_a_name_is_refused_as_the_provider_names_it() {
        let refused = read_new_user(ScimUserFields {
            display_name: Some("L".into()),
            ..sent("lea@neo.example")
        });
        assert_eq!(
            refused,
            Err("Le nom affiché (displayName) compte de 2 à 120 caractères".into())
        );
    }

    #[test]
    fn refuses_what_the_account_could_not_hold() {
        for fields in [
            ScimUserFields::default(),
            sent("  "),
            sent("lea"),
            sent(&format!("{}@neo.example", "l".repeat(250))),
            sent("lea\0@neo.example"),
            ScimUserFields {
                display_name: Some("L".into()),
                ..sent("lea@neo.example")
            },
            ScimUserFields {
                display_name: Some("é".repeat(121)),
                ..sent("lea@neo.example")
            },
            // Its local part, too short a name.
            sent("l@neo.example"),
            ScimUserFields {
                title: Some("t".repeat(121)),
                ..sent("lea@neo.example")
            },
            ScimUserFields {
                external_id: Some("x".repeat(256)),
                ..sent("lea@neo.example")
            },
        ] {
            assert!(read_new_user(fields.clone()).is_err(), "{fields:?}");
        }
    }
}
