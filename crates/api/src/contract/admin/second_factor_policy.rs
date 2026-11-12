//! What the company requires of its password accounts (ADR-201), read and
//! set by those who manage its settings.

use mimorya_core::second_factor::Requirement;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

/// Who must have a second factor to sign in with a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SecondFactorRequirement {
    /// No one: each person chooses.
    Optional,
    /// The accounts that administer the company (a non-delegable permission
    /// in any scope, but validating and depositing documents).
    Administrators,
    /// Every password account.
    Everyone,
}

impl From<Requirement> for SecondFactorRequirement {
    fn from(requirement: Requirement) -> Self {
        match requirement {
            Requirement::Optional => Self::Optional,
            Requirement::Administrators => Self::Administrators,
            Requirement::Everyone => Self::Everyone,
        }
    }
}

impl From<SecondFactorRequirement> for Requirement {
    fn from(requirement: SecondFactorRequirement) -> Self {
        match requirement {
            SecondFactorRequirement::Optional => Self::Optional,
            SecondFactorRequirement::Administrators => Self::Administrators,
            SecondFactorRequirement::Everyone => Self::Everyone,
        }
    }
}

/// `GET /admin/second-factor`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecondFactorPolicyView {
    pub requirement: SecondFactorRequirement,
    /// The server can keep a secret (`MIMORYA_MASTER_KEY`): without one, no
    /// second factor can be required.
    pub secrets_available: bool,
}

/// Body of `PUT /admin/second-factor`.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct SecondFactorPolicyUpdate {
    pub requirement: SecondFactorRequirement,
}
