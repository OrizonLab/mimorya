//! Time-based one-time codes (RFC 6238, over RFC 4226's HOTP), as the
//! authenticator apps compute them: HMAC-SHA1 of the number of 30-second
//! steps since the Unix epoch, truncated to six digits (ADR-200). A code is
//! accepted for the server's step or the one on either side, a phone's
//! clock being off by up to thirty seconds, and only for a step after the
//! last one a code was accepted for: no code passes twice, nor an older one.

use ring::hmac;

/// The secret's length: 160 bits, as RFC 4226 recommends.
pub const SECRET_LEN: usize = 20;
/// How long a code lasts.
pub const STEP_SECONDS: i64 = 30;
/// The digits of a code.
pub const DIGITS: u32 = 6;
/// The steps accepted on either side of the server's.
const DRIFT_STEPS: i64 = 1;

/// The step of the instant `unix_seconds`.
pub fn step_at(unix_seconds: i64) -> i64 {
    unix_seconds.div_euclid(STEP_SECONDS)
}

/// The code of `secret` at `step`, `digits` long (RFC 4226 §5.3).
pub fn code_at(secret: &[u8], step: i64, digits: u32) -> u32 {
    let key = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, secret);
    let tag = hmac::sign(&key, &step.to_be_bytes());
    let hash = tag.as_ref();
    let offset = usize::from(hash[hash.len() - 1] & 0x0f);
    let binary = u32::from_be_bytes([
        hash[offset] & 0x7f,
        hash[offset + 1],
        hash[offset + 2],
        hash[offset + 3],
    ]);
    binary % 10u32.pow(digits)
}

/// The six digits a person types, if `input` is a code (spaces aside).
pub fn read_code(input: &str) -> Option<u32> {
    let digits: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.len() != DIGITS as usize || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The step `code` was computed for, near `now_step` and after `last_step`
/// (the last one a code was accepted for); none if it matches no such step.
pub fn accepted_step(secret: &[u8], code: u32, now_step: i64, last_step: i64) -> Option<i64> {
    (now_step - DRIFT_STEPS..=now_step + DRIFT_STEPS)
        .filter(|step| *step > last_step)
        .find(|step| code_at(secret, *step, DIGITS) == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238, appendix B: the SHA-1 secret and its eight-digit codes.
    const RFC_SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn computes_the_codes_of_rfc_6238() {
        for (time, code) in [
            (59, 94_287_082),
            (1_111_111_109, 7_081_804),
            (1_111_111_111, 14_050_471),
            (1_234_567_890, 89_005_924),
            (2_000_000_000, 69_279_037),
            (20_000_000_000, 65_353_130),
        ] {
            assert_eq!(code_at(RFC_SECRET, step_at(time), 8), code, "at {time}");
        }
        // Six digits: the same truncation, modulo a million.
        assert_eq!(code_at(RFC_SECRET, step_at(59), DIGITS), 287_082);
    }

    #[test]
    fn reads_six_digits_only() {
        assert_eq!(read_code("287082"), Some(287_082));
        assert_eq!(read_code(" 287 082 "), Some(287_082));
        assert_eq!(read_code("012345"), Some(12_345));
        for input in ["28708", "2870821", "28708a", "", "２８７０８２"] {
            assert_eq!(read_code(input), None, "{input}");
        }
    }

    #[test]
    fn accepts_a_code_once_within_a_step_either_side() {
        let now = step_at(1_111_111_111);
        let code = |step| code_at(RFC_SECRET, step, DIGITS);
        // This step, the one before and the one after.
        assert_eq!(accepted_step(RFC_SECRET, code(now), now, 0), Some(now));
        assert_eq!(
            accepted_step(RFC_SECRET, code(now - 1), now, 0),
            Some(now - 1)
        );
        assert_eq!(
            accepted_step(RFC_SECRET, code(now + 1), now, 0),
            Some(now + 1)
        );
        // Two steps away: refused.
        assert_eq!(accepted_step(RFC_SECRET, code(now - 2), now, 0), None);
        assert_eq!(accepted_step(RFC_SECRET, code(now + 2), now, 0), None);
        // Once accepted, neither it nor an older one passes again.
        assert_eq!(accepted_step(RFC_SECRET, code(now), now, now), None);
        assert_eq!(accepted_step(RFC_SECRET, code(now - 1), now, now), None);
        assert_eq!(
            accepted_step(RFC_SECRET, code(now + 1), now, now),
            Some(now + 1)
        );
    }
}
