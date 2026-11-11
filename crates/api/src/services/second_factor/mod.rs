//! The second factor of the accounts that sign in with a password
//! (ADR-200): six-digit codes of an authenticator app (RFC 6238) and
//! recovery codes. One's own, from « Mon compte » (`own.rs`); a code
//! checked, once, its refusals counted and traced (`proof.rs`); the secret
//! sealed under the master key and the recovery codes kept as digests
//! (`secret.rs`); a sign-in that waits for it after the password
//! (`challenge.rs`).

mod challenge;
mod own;
mod proof;
mod secret;

pub use challenge::{
    CHALLENGE_COOKIE, Completion, EXPIRED, Opened, challenge_cookie, cleared_challenge_cookie,
    complete_challenge, open_after_password,
};
pub use own::{
    Enrolment, OwnSecondFactor, confirm_enrolment, disable_second_factor, own_second_factor,
    renew_recovery_codes, start_enrolment,
};
pub use proof::{Proof, Refusal};
