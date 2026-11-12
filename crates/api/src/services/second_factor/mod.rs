//! The second factor of the accounts that sign in with a password
//! (ADR-200): six-digit codes of an authenticator app (RFC 6238) and
//! recovery codes. One's own, from « Mon compte » (`own.rs`); a code
//! checked, once, its refusals counted and traced (`proof.rs`); the secret
//! sealed under the master key and the recovery codes kept as digests
//! (`secret.rs`); a sign-in that waits for it after the password
//! (`challenge.rs`). What the company requires of whom (`requirement.rs`),
//! an enrolment's two steps (`activation.rs`) and the enrolment a sign-in
//! waits for when the company requires one (`sign_in_enrolment.rs`,
//! ADR-201).

mod activation;
mod challenge;
mod own;
mod proof;
mod requirement;
mod secret;
mod sign_in_enrolment;

pub use activation::{Enrolment, WRONG_FIRST_CODE};
pub use challenge::{
    CHALLENGE_COOKIE, Completion, EXPIRED, Opened, challenge_cookie, challenge_purpose,
    cleared_challenge_cookie, complete_challenge, open_after_password,
};
pub use own::{
    Deactivable, OwnSecondFactor, confirm_enrolment, deactivable, disable_second_factor,
    own_second_factor, renew_recovery_codes, start_enrolment,
};
pub use proof::{Proof, Refusal};
pub use requirement::{company_requirement, required_of, second_factor_required};
pub use sign_in_enrolment::{EnrolledSignIn, complete_sign_in_enrolment, start_sign_in_enrolment};
