//! The second factor of the accounts that sign in with a password
//! (ADR-200): what a sign-in asks for next, the state of one's own second
//! factor, its enrolment and recovery codes, and the code a person types.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

/// `POST /auth/login`, `POST /auth/password-reset/complete`: whether the
/// session is open, or waits for the second factor.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SignIn {
    pub next: SignInNext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SignInNext {
    /// The session cookie is set.
    SignedIn,
    /// A code of the account's second factor is asked for
    /// (`POST /auth/second-factor`); only then does the session open.
    SecondFactor,
}

impl SignIn {
    pub const SIGNED_IN: SignIn = SignIn {
        next: SignInNext::SignedIn,
    };
    pub const SECOND_FACTOR: SignIn = SignIn {
        next: SignInNext::SecondFactor,
    };
}

/// `GET /account/second-factor`: one's own second factor.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecondFactorStatus {
    /// The account signs in with a password: an account that signs in
    /// through its company's identity provider has its second factor there.
    pub applicable: bool,
    /// The server can keep a secret (`MIMORYA_MASTER_KEY`).
    pub available: bool,
    pub enabled: bool,
    pub enabled_at: Option<String>,
    /// Recovery codes not used yet.
    pub recovery_codes_left: i64,
}

/// `POST /account/second-factor/enrolment`: what the authenticator app is
/// given, shown this once.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecondFactorEnrolment {
    /// The secret in base32, to type by hand.
    pub secret: String,
    /// The `otpauth://` address its QR code holds.
    pub uri: String,
}

/// `POST /account/second-factor/confirmation` and
/// `POST /account/second-factor/recovery-codes`: the recovery codes,
/// shown this once.
#[derive(Debug, Serialize, TS)]
pub struct RecoveryCodes {
    pub codes: Vec<String>,
}

/// Body of `POST /account/second-factor/enrolment`: the account's password,
/// typed again: a session alone does not change how the account signs in.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct SecondFactorEnrolmentRequest {
    #[validate(length(min = 1, max = 512))]
    pub password: String,
}

/// Body of the routes that take a code of one's second factor: six digits
/// of the app or a recovery code, checked by the server.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct SecondFactorCodeInput {
    #[validate(length(min = 1, max = 64))]
    pub code: String,
}
