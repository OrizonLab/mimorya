//! Which accounts may get a reset link, whoever sends it (ADR-028,
//! ADR-029): an active account that has a password, in an active company.

use mimorya_db::repositories::password_resets::ResetRecipient;

/// Why an account gets no reset link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetRefusal {
    /// It never chose a password: its invitation link serves.
    Invited,
    Disabled,
    /// Active without a password: it signs in through its company's
    /// identity provider (ADR-064).
    NoPassword,
    CompanySuspended,
}

pub fn reset_refusal(recipient: &ResetRecipient) -> Option<ResetRefusal> {
    match recipient.status.as_str() {
        "invited" => Some(ResetRefusal::Invited),
        "active" if !recipient.has_password => Some(ResetRefusal::NoPassword),
        "active" if recipient.tenant_status != "active" => Some(ResetRefusal::CompanySuspended),
        "active" => None,
        _ => Some(ResetRefusal::Disabled),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipient(status: &str, tenant_status: &str, has_password: bool) -> ResetRecipient {
        ResetRecipient {
            email: "zoe.leroy@neo-maintenance.example".into(),
            display_name: "Zoé Leroy".into(),
            company: "Néo Maintenance".into(),
            status: status.into(),
            tenant_status: tenant_status.into(),
            has_password,
        }
    }

    #[test]
    fn only_an_active_account_with_a_password_in_an_active_company() {
        let cases = [
            (("active", "active", true), None),
            (("invited", "active", false), Some(ResetRefusal::Invited)),
            (("disabled", "active", true), Some(ResetRefusal::Disabled)),
            (("active", "active", false), Some(ResetRefusal::NoPassword)),
            (
                ("active", "suspended", true),
                Some(ResetRefusal::CompanySuspended),
            ),
        ];
        for ((status, tenant, password), expected) in cases {
            assert_eq!(
                reset_refusal(&recipient(status, tenant, password)),
                expected,
                "{status} {tenant} {password}"
            );
        }
    }
}
