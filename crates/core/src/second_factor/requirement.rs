//! What a company requires of its password accounts (ADR-201): a second
//! factor for no one (it stays each person's choice), for its
//! administrators, or for everyone. An administrator is an account holding
//! one of the permissions that administer the company, in any scope: the
//! non-delegable ones, but for validating one's team's actions and
//! depositing documents, which are no administration.

/// Who must have a second factor to sign in with a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Requirement {
    /// No one: each person chooses.
    #[default]
    Optional,
    /// The accounts that administer the company.
    Administrators,
    /// Every password account.
    Everyone,
}

impl Requirement {
    /// Every value, in the order the database constraint lists them.
    pub const ALL: [Self; 3] = [Self::Optional, Self::Administrators, Self::Everyone];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Optional => "optional",
            Self::Administrators => "administrators",
            Self::Everyone => "everyone",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "optional" => Some(Self::Optional),
            "administrators" => Some(Self::Administrators),
            "everyone" => Some(Self::Everyone),
            _ => None,
        }
    }

    /// Whether an account must have a second factor, `administers` telling
    /// whether it holds one of `ADMINISTRATION_PERMISSIONS`.
    pub fn applies(self, administers: bool) -> bool {
        match self {
            Self::Optional => false,
            Self::Administrators => administers,
            Self::Everyone => true,
        }
    }
}

/// The permissions that make an account one of the company's
/// administrators.
pub const ADMINISTRATION_PERMISSIONS: [&str; 12] = [
    "org:manage",
    "user:manage",
    "user:erase",
    "role:manage",
    "policy:manage",
    "memory:manage",
    "skill:manage",
    "connector:manage",
    "audit:read",
    "audit:export",
    "settings:manage",
    "retention:manage",
];

/// Whether `holds` holds one of `ADMINISTRATION_PERMISSIONS`.
pub fn administers(holds: impl Fn(&str) -> bool) -> bool {
    ADMINISTRATION_PERMISSIONS
        .iter()
        .any(|permission| holds(permission))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::authz::{PLATFORM_PERMISSIONS, is_non_delegable};

    #[test]
    fn reads_back_what_it_writes() {
        for requirement in Requirement::ALL {
            assert_eq!(Requirement::parse(requirement.as_str()), Some(requirement));
        }
        assert_eq!(Requirement::parse("tous"), None);
        assert_eq!(Requirement::default(), Requirement::Optional);
    }

    #[test]
    fn applies_to_whom_the_company_says() {
        assert!(!Requirement::Optional.applies(true));
        assert!(Requirement::Administrators.applies(true));
        assert!(!Requirement::Administrators.applies(false));
        assert!(Requirement::Everyone.applies(false));
    }

    #[test]
    fn administration_is_every_non_delegable_permission_but_two() {
        // A new non-delegable permission must be placed knowingly: here, or
        // among the two that administer nothing.
        let not_administration = ["approval:decide", "document:write"];
        let expected: BTreeSet<&str> = PLATFORM_PERMISSIONS
            .iter()
            .map(|p| p.permission)
            .filter(|p| is_non_delegable(p) && !not_administration.contains(p))
            .collect();
        let listed: BTreeSet<&str> = ADMINISTRATION_PERMISSIONS.into_iter().collect();
        assert_eq!(listed, expected);
        assert!(administers(|p| p == "audit:read"));
        assert!(!administers(|p| p == "agent:use" || p == "approval:decide"));
    }
}
