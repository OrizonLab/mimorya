//! When Slack proves that a Slack user is the person of an account
//! (ADR-125): a person of the application's own workspace, neither deleted
//! nor a bot, whose address Slack gives, and gives as the account's own.
//! The address is the proof, as at the providers people link their mailbox
//! at (ADR-116): the company manages both, and a link opened by someone
//! else, sent the link of another Slack user, links nothing.

/// What Slack says of a Slack user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlackProfile<'a> {
    pub workspace: Option<&'a str>,
    pub address: Option<&'a str>,
    pub deleted: bool,
    pub bot: bool,
}

/// Why a Slack user is not taken for the person of an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityRefusal {
    Deleted,
    Bot,
    OtherWorkspace,
    NoAddress,
    OtherAddress,
}

impl IdentityRefusal {
    /// As audited.
    pub fn code(self) -> &'static str {
        match self {
            Self::Deleted => "deleted",
            Self::Bot => "bot",
            Self::OtherWorkspace => "other_workspace",
            Self::NoAddress => "no_address",
            Self::OtherAddress => "other_address",
        }
    }

    /// What the person is told, in French: never the Slack user's address.
    pub fn message(self) -> &'static str {
        match self {
            Self::Deleted => "Ce compte Slack est désactivé : il ne peut pas être relié",
            Self::Bot => "Ce compte Slack est celui d’un bot : il ne peut pas être relié",
            Self::OtherWorkspace => {
                "Ce compte Slack n’est pas de l’espace de travail de votre entreprise : il ne \
                 peut pas être relié"
            }
            Self::NoAddress => {
                "Slack ne donne pas l’adresse de ce compte : l’application de votre entreprise \
                 doit avoir la permission users:read.email (votre administrateur peut la lui \
                 donner)"
            }
            Self::OtherAddress => {
                "L’adresse de ce compte Slack n’est pas celle de votre compte : la liaison est \
                 refusée. Ouvrez le lien que l’application vous a envoyé dans Slack, depuis votre \
                 propre compte Slack"
            }
        }
    }
}

/// Why `profile` is not the person whose account address is
/// `account_email`, in the workspace `workspace`; `None` when it is.
pub fn identity_refusal(
    profile: &SlackProfile<'_>,
    workspace: &str,
    account_email: &str,
) -> Option<IdentityRefusal> {
    if profile.deleted {
        return Some(IdentityRefusal::Deleted);
    }
    if profile.bot {
        return Some(IdentityRefusal::Bot);
    }
    if profile.workspace != Some(workspace) {
        return Some(IdentityRefusal::OtherWorkspace);
    }
    let Some(address) = profile.address.map(str::trim).filter(|a| !a.is_empty()) else {
        return Some(IdentityRefusal::NoAddress);
    };
    if !address.eq_ignore_ascii_case(account_email.trim()) {
        return Some(IdentityRefusal::OtherAddress);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKSPACE: &str = "T0NEO";
    const LEA: &str = "lea.martin@neo-maintenance.example";

    fn profile(address: Option<&str>) -> SlackProfile<'_> {
        SlackProfile {
            workspace: Some(WORKSPACE),
            address,
            deleted: false,
            bot: false,
        }
    }

    #[test]
    fn the_person_s_own_address_proves_them() {
        assert_eq!(identity_refusal(&profile(Some(LEA)), WORKSPACE, LEA), None);
        // Case and surrounding spaces aside.
        let written = " Lea.Martin@Neo-Maintenance.example ";
        assert_eq!(
            identity_refusal(&profile(Some(written)), WORKSPACE, LEA),
            None
        );
    }

    #[test]
    fn nothing_else_does() {
        let check = |profile: SlackProfile<'_>| identity_refusal(&profile, WORKSPACE, LEA);
        assert_eq!(
            check(profile(Some("paul.durand@neo-maintenance.example"))),
            Some(IdentityRefusal::OtherAddress)
        );
        // An address merely containing the account's.
        assert_eq!(
            check(profile(Some("x.lea.martin@neo-maintenance.example"))),
            Some(IdentityRefusal::OtherAddress)
        );
        assert_eq!(check(profile(None)), Some(IdentityRefusal::NoAddress));
        assert_eq!(check(profile(Some("  "))), Some(IdentityRefusal::NoAddress));
        let foreign = SlackProfile {
            workspace: Some("T0AUTRE"),
            ..profile(Some(LEA))
        };
        assert_eq!(check(foreign), Some(IdentityRefusal::OtherWorkspace));
        let unknown = SlackProfile {
            workspace: None,
            ..profile(Some(LEA))
        };
        assert_eq!(check(unknown), Some(IdentityRefusal::OtherWorkspace));
        let deleted = SlackProfile {
            deleted: true,
            ..profile(Some(LEA))
        };
        assert_eq!(check(deleted), Some(IdentityRefusal::Deleted));
        let bot = SlackProfile {
            bot: true,
            ..profile(Some(LEA))
        };
        assert_eq!(check(bot), Some(IdentityRefusal::Bot));
    }

    #[test]
    fn a_refusal_never_quotes_an_address() {
        for refusal in [
            IdentityRefusal::Deleted,
            IdentityRefusal::Bot,
            IdentityRefusal::OtherWorkspace,
            IdentityRefusal::NoAddress,
            IdentityRefusal::OtherAddress,
        ] {
            assert!(!refusal.message().contains('@'), "{refusal:?}");
            assert!(!refusal.code().is_empty());
        }
    }
}
