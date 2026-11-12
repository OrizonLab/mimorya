//! The second factor of the accounts that sign in with a password
//! (ADR-200): six-digit time-based codes (RFC 6238) from an authenticator
//! app, given its secret by an `otpauth://` address, and recovery codes for
//! whoever lost it. What a person types is one or the other. What a company
//! requires of whom (ADR-201).

mod enrolment;
mod recovery;
mod requirement;
mod totp;

pub use enrolment::{ISSUER, base32, otpauth_uri, read_base32};
pub use recovery::{
    CODE_BYTES, RECOVERY_CODES, read_recovery_code, recovery_code, recovery_digest,
};
pub use requirement::{ADMINISTRATION_PERMISSIONS, Requirement, administers};
pub use totp::{DIGITS, SECRET_LEN, STEP_SECONDS, accepted_step, code_at, read_code, step_at};

/// Codes refused in a row, of the app or recovery codes, after which only
/// a recovery code is accepted: NIST SP 800-63B limits an account's failed
/// attempts to a hundred.
pub const REFUSALS_BEFORE_LOCK: i32 = 100;

/// What a person typed to prove their second factor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecondFactorCode {
    /// Six digits of their authenticator app.
    Totp(u32),
    /// One of their recovery codes, as kept (`read_recovery_code`).
    Recovery(String),
}

impl SecondFactorCode {
    /// What `input` is, if either.
    pub fn read(input: &str) -> Option<Self> {
        read_code(input)
            .map(Self::Totp)
            .or_else(|| read_recovery_code(input).map(Self::Recovery))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_code_is_a_totp_or_a_recovery_code() {
        assert_eq!(
            SecondFactorCode::read(" 012 345 "),
            Some(SecondFactorCode::Totp(12_345))
        );
        assert_eq!(
            SecondFactorCode::read("ijbe-eqsc-ijbe-eqsc"),
            Some(SecondFactorCode::Recovery("IJBEEQSCIJBEEQSC".into()))
        );
        assert_eq!(SecondFactorCode::read("bonjour"), None);
    }
}
