//! When Teams proves that a Teams user is the person of an account
//! (ADR-138): the member of the personal conversation the link was sent in,
//! as the Bot Framework describes them now, of the company's own Microsoft
//! Entra directory, the same Entra object the link was sent to, whose
//! address (their mail, or their sign-in name) is the account's own. The
//! address is the proof, as in Slack (ADR-125): the company manages both,
//! and a link opened by someone else, sent to another Teams user, links
//! nothing.

use super::activity::same_guid;

/// What Teams says of the member of a personal conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamsMember<'a> {
    pub aad_object_id: Option<&'a str>,
    pub tenant_id: Option<&'a str>,
    pub email: Option<&'a str>,
    pub user_principal_name: Option<&'a str>,
}

/// Why a Teams user is not taken for the person of an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityRefusal {
    /// Teams names another Entra object than the link was sent to.
    OtherPerson,
    OtherDirectory,
    NoAddress,
    OtherAddress,
}

impl IdentityRefusal {
    /// As audited.
    pub fn code(self) -> &'static str {
        match self {
            Self::OtherPerson => "other_person",
            Self::OtherDirectory => "other_directory",
            Self::NoAddress => "no_address",
            Self::OtherAddress => "other_address",
        }
    }

    /// What the person is told, in French: never the Teams user's address.
    pub fn message(self) -> &'static str {
        match self {
            Self::OtherPerson => {
                "Teams ne dit plus que ce compte est celui à qui le lien a été envoyé : la \
                 liaison est refusée. Écrivez de nouveau au bot dans Teams pour recevoir un \
                 autre lien"
            }
            Self::OtherDirectory => {
                "Ce compte Teams n’est pas de l’annuaire Microsoft de votre entreprise : il ne \
                 peut pas être relié"
            }
            Self::NoAddress => {
                "Teams ne donne pas l’adresse de ce compte : il ne peut pas être relié"
            }
            Self::OtherAddress => {
                "L’adresse de ce compte Teams n’est pas celle de votre compte : la liaison est \
                 refusée. Ouvrez le lien que le bot vous a envoyé dans Teams, depuis votre \
                 propre compte Teams"
            }
        }
    }
}

/// Why `member` is not the person whose account address is
/// `account_email`, the Entra object `aad_object_id` of the directory
/// `directory_id`; `None` when it is.
pub fn identity_refusal(
    member: &TeamsMember<'_>,
    (aad_object_id, directory_id): (&str, &str),
    account_email: &str,
) -> Option<IdentityRefusal> {
    if !member
        .aad_object_id
        .is_some_and(|object| same_guid(object, aad_object_id))
    {
        return Some(IdentityRefusal::OtherPerson);
    }
    if !member
        .tenant_id
        .is_some_and(|tenant| same_guid(tenant, directory_id))
    {
        return Some(IdentityRefusal::OtherDirectory);
    }
    let addresses: Vec<&str> = [member.email, member.user_principal_name]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .collect();
    if addresses.is_empty() {
        return Some(IdentityRefusal::NoAddress);
    }
    let account = account_email.trim();
    if !addresses
        .iter()
        .any(|address| address.eq_ignore_ascii_case(account))
    {
        return Some(IdentityRefusal::OtherAddress);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIRECTORY: &str = "0b2c3d4e-5f60-4718-8293-a4b5c6d7e8f9";
    const PERSON: &str = "d6b9e1f2-3a4b-4c5d-8e9f-0a1b2c3d4e5f";
    const ACCOUNT: &str = "marc.lefevre@neo-maintenance.example";

    fn member<'a>() -> TeamsMember<'a> {
        TeamsMember {
            aad_object_id: Some(PERSON),
            tenant_id: Some(DIRECTORY),
            email: Some("Marc.Lefevre@neo-maintenance.example"),
            user_principal_name: Some("mlefevre@neo-maintenance.example"),
        }
    }

    fn refusal(member: &TeamsMember) -> Option<IdentityRefusal> {
        identity_refusal(member, (PERSON, DIRECTORY), ACCOUNT)
    }

    #[test]
    fn the_member_with_the_account_s_address_in_the_directory_is_the_person() {
        assert_eq!(refusal(&member()), None);
        // By their sign-in name, when it is the account's address.
        let by_name = TeamsMember {
            email: None,
            user_principal_name: Some(ACCOUNT),
            ..member()
        };
        assert_eq!(refusal(&by_name), None);
    }

    #[test]
    fn another_person_directory_or_address_is_refused() {
        let other = TeamsMember {
            aad_object_id: Some("11111111-2222-4333-8444-555555555555"),
            ..member()
        };
        assert_eq!(refusal(&other), Some(IdentityRefusal::OtherPerson));
        let unnamed = TeamsMember {
            aad_object_id: None,
            ..member()
        };
        assert_eq!(refusal(&unnamed), Some(IdentityRefusal::OtherPerson));
        let elsewhere = TeamsMember {
            tenant_id: Some("11111111-2222-4333-8444-555555555555"),
            ..member()
        };
        assert_eq!(refusal(&elsewhere), Some(IdentityRefusal::OtherDirectory));
        let silent = TeamsMember {
            email: Some(" "),
            user_principal_name: None,
            ..member()
        };
        assert_eq!(refusal(&silent), Some(IdentityRefusal::NoAddress));
        let someone_else = TeamsMember {
            email: Some("lea.font@neo-maintenance.example"),
            user_principal_name: Some("lfont@neo-maintenance.example"),
            ..member()
        };
        assert_eq!(refusal(&someone_else), Some(IdentityRefusal::OtherAddress));
        for refused in [
            IdentityRefusal::OtherPerson,
            IdentityRefusal::OtherDirectory,
            IdentityRefusal::NoAddress,
            IdentityRefusal::OtherAddress,
        ] {
            assert!(!refused.message().contains('@'));
        }
    }
}
