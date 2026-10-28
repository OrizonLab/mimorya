//! How a request Slack sends proves it comes from the company's application
//! (ADR-125): Slack signs `v0:<timestamp>:<body>` with the application's
//! signing secret (HMAC-SHA256), and sends the signature and the timestamp
//! in headers. A request whose signature does not match, or signed more than
//! five minutes from now (a replay), is not Slack's.

use ring::hmac;

/// The version of the signature Slack sends.
pub const VERSION: &str = "v0";
/// How far from now a signed request may have been signed.
pub const MAX_SKEW_SECONDS: i64 = 5 * 60;
/// A timestamp, in seconds since the epoch: twelve digits last until the
/// year 33658.
const MAX_TIMESTAMP_DIGITS: usize = 12;

/// Why a request is not taken as Slack's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureProblem {
    /// No signature or no timestamp.
    Missing,
    /// Signed too long ago, or in the future.
    Stale,
    /// Not signed by this secret, or not written as Slack writes it.
    Mismatch,
}

/// The signature Slack sends of `body`, sent at `timestamp`, under
/// `secret`: `v0=` then the HMAC-SHA256, in lowercase hexadecimal.
pub fn signature(secret: &str, timestamp: &str, body: &[u8]) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let tag = hmac::sign(&key, &signed(timestamp, body));
    let hex: String = tag.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    format!("{VERSION}={hex}")
}

/// Whether `body`, received with these headers at `now` (seconds since the
/// epoch), was signed by `secret` within five minutes of now. The signature
/// is compared in constant time.
pub fn verify(
    secret: &str,
    timestamp: Option<&str>,
    sent_signature: Option<&str>,
    body: &[u8],
    now: i64,
) -> Result<(), SignatureProblem> {
    let (Some(timestamp), Some(sent_signature)) = (timestamp, sent_signature) else {
        return Err(SignatureProblem::Missing);
    };
    let digits = timestamp.len() <= MAX_TIMESTAMP_DIGITS
        && !timestamp.is_empty()
        && timestamp.bytes().all(|b| b.is_ascii_digit());
    let Some(sent_at) = digits.then(|| timestamp.parse::<i64>().ok()).flatten() else {
        return Err(SignatureProblem::Mismatch);
    };
    if (now - sent_at).abs() > MAX_SKEW_SECONDS {
        return Err(SignatureProblem::Stale);
    }
    let tag = sent_signature
        .strip_prefix(VERSION)
        .and_then(|rest| rest.strip_prefix('='))
        .and_then(decode_hex)
        .ok_or(SignatureProblem::Mismatch)?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    hmac::verify(&key, &signed(timestamp, body), &tag).map_err(|_| SignatureProblem::Mismatch)
}

/// What Slack signs.
fn signed(timestamp: &str, body: &[u8]) -> Vec<u8> {
    let mut text = format!("{VERSION}:{timestamp}:").into_bytes();
    text.extend_from_slice(body);
    text
}

/// A SHA-256 tag written in hexadecimal (64 digits, either case).
fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "8f742231b10e8888abcd99yyyzzz85a5";
    const NOW: i64 = 1_531_420_618;

    #[test]
    fn slack_s_own_example_is_verified() {
        // The example of Slack's documentation ("Verifying requests from Slack").
        let body = b"token=xyzz0WbapA4vBCDEFasx0q6G&team_id=T1DC2JH3J&team_domain=testteamnow&channel_id=G8PSS9T3V&channel_name=foobar&user_id=U2CERLKJA&user_name=roadrunner&command=%2Fwebhook-collect&text=&response_url=https%3A%2F%2Fhooks.slack.com%2Fcommands%2FT1DC2JH3J%2F397700885554%2F96rGlfmibIGlgcZRskXaIFfN&trigger_id=398738663015.47445629121.803a0bc887a14d10d2c447fce8b6703c";
        let expected = "v0=a2114d57b48eac39b9ad189dd8316235a7b4a8d21a10bd27519666489c69b503";
        assert_eq!(signature(SECRET, "1531420618", body), expected);
        assert_eq!(
            verify(SECRET, Some("1531420618"), Some(expected), body, NOW),
            Ok(())
        );
        // Either case of hexadecimal digits.
        let upper = format!("v0={}", expected[3..].to_uppercase());
        assert_eq!(
            verify(SECRET, Some("1531420618"), Some(&upper), body, NOW),
            Ok(())
        );
    }

    #[test]
    fn another_body_secret_or_time_is_refused() {
        let body = br#"{"type":"event_callback"}"#;
        let signed = signature(SECRET, "1531420618", body);
        let check = |secret: &str, ts: &str, sig: &str, body: &[u8], now: i64| {
            verify(secret, Some(ts), Some(sig), body, now)
        };
        assert_eq!(check(SECRET, "1531420618", &signed, body, NOW), Ok(()));
        let mismatch = Err(SignatureProblem::Mismatch);
        assert_eq!(check("autre", "1531420618", &signed, body, NOW), mismatch);
        assert_eq!(check(SECRET, "1531420618", &signed, b"{}", NOW), mismatch);
        // The timestamp is signed: another one, though recent, does not match.
        assert_eq!(check(SECRET, "1531420619", &signed, body, NOW), mismatch);
        // Five minutes either way, not more.
        assert_eq!(
            check(SECRET, "1531420618", &signed, body, NOW + 300),
            Ok(())
        );
        assert_eq!(
            check(SECRET, "1531420618", &signed, body, NOW - 300),
            Ok(())
        );
        let stale = Err(SignatureProblem::Stale);
        assert_eq!(check(SECRET, "1531420618", &signed, body, NOW + 301), stale);
        assert_eq!(check(SECRET, "1531420618", &signed, body, NOW - 301), stale);
    }

    #[test]
    fn only_the_signature_as_slack_writes_it_is_read() {
        let body = b"{}";
        let signed = signature(SECRET, "1531420618", body);
        let hex = &signed[3..];
        let mismatch = Err(SignatureProblem::Mismatch);
        for written in [
            hex.to_owned(),
            format!("v1={hex}"),
            format!("v0:{hex}"),
            format!("v0={}", &hex[..62]),
            format!("v0={hex}00"),
            format!("v0= {}", &hex[1..]),
            format!("v0={}é", &hex[..62]),
        ] {
            assert_eq!(
                verify(SECRET, Some("1531420618"), Some(&written), body, NOW),
                mismatch,
                "{written}"
            );
        }
        for timestamp in [
            "",
            "+1531420618",
            "1531420618.5",
            " 1531420618",
            "-1",
            "9999999999999",
        ] {
            assert_eq!(
                verify(SECRET, Some(timestamp), Some(&signed), body, NOW),
                mismatch,
                "{timestamp}"
            );
        }
        let missing = Err(SignatureProblem::Missing);
        assert_eq!(verify(SECRET, None, Some(&signed), body, NOW), missing);
        assert_eq!(verify(SECRET, Some("1531420618"), None, body, NOW), missing);
    }
}
